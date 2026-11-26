//! Review-only legacy import bundles. No live or publishing authorization.
//! A single immutable envelope is the atomic commit unit. Save copies never
//! overwrite an existing file, including the original legacy source.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use hmi_legacy_import::import_portable;
use hmi_project_model::model::Project;
use hmi_project_model::serialization::{load_project, save_project};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::fs::FileSystem;
use crate::persistence::stage_temp;
use crate::{ProjectSession, ProjectSnapshot, SessionConfig, Transaction, TransactionError, TransactionOutcome};

const MARKER: &str = "hmistudio.importQuarantine";
const FORMAT: &str = "hmi-studio.legacy-review";
const MAX_BUNDLE_BYTES: usize = 64 * 1024 * 1024;

fn marker() -> Value {
    json!({"version":1,"state":"reviewRequired"})
}

pub(crate) fn reserved_extension(key: &str) -> bool {
    matches!(key, MARKER | "hmiforge.idMap" | "hmiforge.standardCatalog")
}

/// Fail closed on any import marker, including malformed or older provenance.
pub fn requires_review(project: &Project) -> bool {
    project.extensions.contains_key(MARKER) || project.extensions.contains_key("hmiforge.idMap")
}

/// Required import-specific precondition for future production publishing
/// and live configuration. Passing is not general runtime authorization.
/// No acceptance/clearance command is provided in this milestone.
pub fn check_production_admission(project: &Project) -> Result<(), ImportError> {
    if requires_review(project) {
        Err(ImportError::ReviewRequired)
    } else {
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("legacy input was refused; no project was admitted")]
    Refused,
    #[error("import requires engineering review")]
    ReviewRequired,
    #[error("invalid or incomplete review bundle")]
    InvalidBundle,
    #[error("review bundle exceeds size limit")]
    SizeLimit,
    #[error("bundle I/O failed at {0}")]
    Io(&'static str),
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Payload {
    original_project: String,
    project: String,
    report: Value,
    resources: BTreeMap<String, Vec<u8>>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Envelope {
    format: String,
    version: u32,
    sha256: String,
    payload: Payload,
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn canonical(project: &Project) -> Result<String, ImportError> {
    save_project(project).map_err(|_| ImportError::InvalidBundle)
}

fn decode_project(text: &str) -> Result<Project, ImportError> {
    let project = load_project(text).map_err(|_| ImportError::InvalidBundle)?.project;
    if project.extensions.get(MARKER) != Some(&marker()) || canonical(&project)? != text {
        return Err(ImportError::InvalidBundle);
    }
    Ok(project)
}

fn verify_resources(project: &Project, resources: &BTreeMap<String, Vec<u8>>) -> Result<(), ImportError> {
    let data = serde_json::to_value(project).map_err(|_| ImportError::InvalidBundle)?;
    let mut used = std::collections::BTreeSet::new();
    for resource in data.get("resources").and_then(Value::as_array).into_iter().flatten() {
        let path = resource["path"].as_str().ok_or(ImportError::InvalidBundle)?;
        let expected = resource["sha256"].as_str().ok_or(ImportError::InvalidBundle)?;
        let extension = match resource["mediaType"].as_str() {
            Some("image/png") => "png",
            Some("image/jpeg") => "jpg",
            Some("image/svg+xml") => "svg",
            _ => return Err(ImportError::InvalidBundle),
        };
        let bytes = resources.get(path).ok_or(ImportError::InvalidBundle)?;
        if digest(bytes) != expected || path != format!("assets/{expected}.{extension}") {
            return Err(ImportError::InvalidBundle);
        }
        used.insert(path);
    }
    if used.len() != resources.len() {
        return Err(ImportError::InvalidBundle);
    }
    Ok(())
}

fn verify_report(report: &Value, original: &Project) -> Result<(), ImportError> {
    let value = serde_json::to_value(original).map_err(|_| ImportError::InvalidBundle)?;
    let findings = report["findings"].as_array().ok_or(ImportError::InvalidBundle)?;
    let mut paths = std::collections::BTreeSet::new();
    let source_hash = report["sourceSha256"].as_str().ok_or(ImportError::InvalidBundle)?;
    if source_hash.len() != 64
        || !source_hash.bytes().all(|b| b.is_ascii_hexdigit())
        || report["destinationAuditCompleted"] != true
        || report["sourceLeafCount"].as_u64() != Some(findings.len() as u64)
        || report["unresolvedTargets"] != json!([])
        || report["idMap"] != value["extensions"]["hmiforge.idMap"]
    {
        return Err(ImportError::InvalidBundle);
    }
    for finding in findings {
        let path = finding["path"].as_str().ok_or(ImportError::InvalidBundle)?;
        let target = finding["target"].as_str().ok_or(ImportError::InvalidBundle)?;
        if !paths.insert(path)
            || !matches!(finding["disposition"].as_str(), Some("mapped" | "preserved"))
            || finding["severity"] == "blocking"
            || value.pointer(target).is_none()
        {
            return Err(ImportError::InvalidBundle);
        }
    }
    Ok(())
}

fn verify_payload(payload: &Payload) -> Result<Project, ImportError> {
    let original = decode_project(&payload.original_project)?;
    let current = decode_project(&payload.project)?;
    verify_report(&payload.report, &original)?;
    verify_resources(&original, &payload.resources)?;
    verify_resources(&current, &payload.resources)?;
    if original.id != current.id || original.extensions.get("hmiforge.idMap") != current.extensions.get("hmiforge.idMap") {
        return Err(ImportError::InvalidBundle);
    }
    Ok(current)
}

/// Engineering session with immutable import evidence. The inner session
/// cannot be extracted or sent to normal project persistence.
pub struct QuarantinedImport {
    session: ProjectSession,
    payload: Payload,
    path: PathBuf,
}

impl QuarantinedImport {
    /// Import immutable source bytes, persist the complete review bundle,
    /// and only then return an editable engineering session.
    pub fn import_new(
        source: &[u8],
        target: &Path,
        config: SessionConfig,
        fs: Arc<dyn FileSystem>,
    ) -> Result<Self, ImportError> {
        let outcome = import_portable(source);
        if !outcome.report.has_complete_mapping() {
            return Err(ImportError::Refused);
        }
        let project = outcome.project.ok_or(ImportError::Refused)?;
        let text = canonical(&project)?;
        let payload = Payload {
            original_project: text.clone(),
            project: text,
            report: serde_json::to_value(outcome.report).map_err(|_| ImportError::InvalidBundle)?,
            resources: outcome.resource_bytes,
        };
        let project = verify_payload(&payload)?;
        write_new(&payload, target, fs.as_ref())?;
        Ok(Self {
            session: ProjectSession::with_fs(project, config, fs),
            payload,
            path: target.to_path_buf(),
        })
    }

    /// Open only the review envelope, verifying checksums and evidence.
    pub fn reopen(path: &Path, config: SessionConfig, fs: Arc<dyn FileSystem>) -> Result<Self, ImportError> {
        let bytes = fs.read(path).map_err(|_| ImportError::Io("read"))?;
        if bytes.len() > MAX_BUNDLE_BYTES {
            return Err(ImportError::SizeLimit);
        }
        let envelope: Envelope = serde_json::from_slice(&bytes).map_err(|_| ImportError::InvalidBundle)?;
        let serialized = serde_json::to_vec(&envelope.payload).map_err(|_| ImportError::InvalidBundle)?;
        if envelope.format != FORMAT || envelope.version != 1 || envelope.sha256 != digest(&serialized) {
            return Err(ImportError::InvalidBundle);
        }
        let project = verify_payload(&envelope.payload)?;
        Ok(Self {
            session: ProjectSession::with_fs(project, config, fs),
            payload: envelope.payload,
            path: path.to_path_buf(),
        })
    }

    pub fn snapshot(&self) -> ProjectSnapshot {
        self.session.snapshot()
    }

    pub fn report(&self) -> &Value {
        &self.payload.report
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn execute(&mut self, transaction: Transaction) -> Result<TransactionOutcome, TransactionError> {
        self.session.execute(transaction)
    }

    /// Persist edits in a new review bundle; never overwrite an existing
    /// bundle or source. On failure the current in-memory state is unchanged.
    pub fn save_copy(&mut self, target: &Path) -> Result<(), ImportError> {
        let mut payload = self.payload.clone();
        payload.project = canonical(self.session.project())?;
        verify_payload(&payload)?;
        write_new(&payload, target, self.session.fs.as_ref())?;
        self.session.storage.saved_state = Some(self.session.current_state_id());
        self.payload = payload;
        self.path = target.to_path_buf();
        Ok(())
    }

    /// All imports remain refused until a future explicit acceptance flow.
    pub fn check_production_admission(&self) -> Result<(), ImportError> {
        Err(ImportError::ReviewRequired)
    }
}

fn write_new(payload: &Payload, target: &Path, fs: &dyn FileSystem) -> Result<(), ImportError> {
    let body = serde_json::to_vec(payload).map_err(|_| ImportError::InvalidBundle)?;
    let envelope = Envelope {
        format: FORMAT.into(),
        version: 1,
        sha256: digest(&body),
        payload: payload.clone(),
    };
    let bytes = serde_json::to_vec(&envelope).map_err(|_| ImportError::InvalidBundle)?;
    if bytes.len() > MAX_BUNDLE_BYTES {
        return Err(ImportError::SizeLimit);
    }
    let tmp = stage_temp(fs, target, &bytes).map_err(|_| ImportError::Io("stage"))?;
    // Unlike rename, an exclusive same-volume hard link cannot replace an
    // existing destination even if it appears after the staging operation.
    let result = fs.link_new(&tmp, target);
    let _ = fs.remove_file(&tmp);
    result.map_err(|_| ImportError::Io("publish new bundle"))?;
    let parent = target.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
    // Publication already committed; do not report rollback after this point.
    let _ = fs.sync_dir(parent);
    Ok(())
}
