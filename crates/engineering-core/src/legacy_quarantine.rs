//! Review-only legacy import bundles. No live or publishing authorization.
//! A single immutable envelope is the atomic commit unit. Save copies never
//! overwrite an existing file, including the original legacy source.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use hmi_legacy_import::{import_portable, replay_portable};
pub use hmi_legacy_import::{package as package_transport, revision as revision_transport};
use hmi_project_model::model::Project;
use hmi_project_model::serialization::{load_project, save_project};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::fs::FileSystem;
use crate::persistence::{sidecar_dir, stage_temp};
use crate::{
    AutosaveOutcome, ProjectSession, ProjectSnapshot, SessionConfig, Transaction, TransactionError,
    TransactionOutcome,
};

const MARKER: &str = "hmistudio.importQuarantine";
const FORMAT: &str = "hmi-studio.legacy-review";
const RECOVERY_FORMAT: &str = "hmi-studio.legacy-review-recovery";
pub fn review_recovery_path(bundle: &Path) -> PathBuf {
    sidecar_dir(bundle).join("review-recovery.json")
}
const MAX_BUNDLE_BYTES: usize = 64 * 1024 * 1024;
/// Native import readers use the same bound as the portable decoder.
pub const MAX_LEGACY_SOURCE_BYTES: usize = hmi_legacy_import::MAX_INPUT_BYTES;

fn marker() -> Value {
    json!({"version":1,"state":"reviewRequired"})
}

pub(crate) fn reserved_extension(key: &str) -> bool {
    matches!(key, MARKER | "hmiforge.idMap" | "hmiforge.standardCatalog")
}

/// Fail closed on any import marker, including malformed or older provenance.
pub fn requires_review(project: &Project) -> bool {
    is_legacy_id(project.id.as_str())
        || project.extensions.keys().any(|key| reserved_extension(key))
}

fn is_legacy_id(id: &str) -> bool {
    id.starts_with("legacy-project-")
}

// Recognize reserved provenance even if the rest of the document cannot be
// loaded as a valid project (for example a review envelope renamed as JSON).
pub(crate) fn storage_requires_review(bytes: &[u8]) -> bool {
    serde_json::from_slice::<Value>(bytes).is_ok_and(|value| {
        matches!(value["format"].as_str(), Some(FORMAT | RECOVERY_FORMAT))
            || value["id"].as_str().is_some_and(is_legacy_id)
            || value["extensions"]
                .as_object()
                .is_some_and(|extensions| extensions.keys().any(|key| reserved_extension(key)))
    })
}

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("legacy input was refused; no project was admitted")]
    Refused,
    #[error("invalid or incomplete review bundle")]
    InvalidBundle,
    #[error("review recovery does not match the saved bundle or is damaged")]
    InvalidRecovery,
    #[error("review bundle exceeds size limit")]
    SizeLimit,
    #[error("bundle I/O failed at {0}")]
    Io(&'static str),
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Payload {
    // Private local evidence; never include in reports or telemetry.
    source: String,
    original_project: String,
    project: String,
    report: Value,
    resources: BTreeMap<String, Vec<u8>>,
    // Omit on v2 portable bundles to preserve historical checksum bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    revision_source: Option<RevisionSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    package_source: Option<PackageSource>,
}

/// Exact private local evidence for the selected revision, not executable
/// history. The native caller must explicitly obtain consent to retain it.
/// No Debug implementation or source-data access through the workspace IPC.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RevisionSource {
    descriptor: String,
    head: String,
    revision: String,
}

impl RevisionSource {
    /// Validate the complete transport before retaining any bytes.
    pub fn from_records(
        descriptor: &[u8],
        head: &[u8],
        revision: &[u8],
    ) -> Result<Self, ImportError> {
        let selection = revision_transport::select_current_revision(descriptor, head)
            .map_err(|_| ImportError::Refused)?;
        revision_transport::decode_current_revision(&selection, revision)
            .map_err(|_| ImportError::Refused)?;
        let text = |bytes: &[u8]| {
            std::str::from_utf8(bytes)
                .map(str::to_owned)
                .map_err(|_| ImportError::Refused)
        };
        Ok(Self {
            descriptor: text(descriptor)?,
            head: text(head)?,
            revision: text(revision)?,
        })
    }

    fn project_bytes(&self) -> Result<&[u8], ImportError> {
        let selection = revision_transport::select_current_revision(
            self.descriptor.as_bytes(),
            self.head.as_bytes(),
        )
        .map_err(|_| ImportError::InvalidBundle)?;
        let revision =
            revision_transport::decode_current_revision(&selection, self.revision.as_bytes())
                .map_err(|_| ImportError::InvalidBundle)?;
        Ok(revision.project_bytes())
    }
}

/// Private source archive, retained without extraction or publisher trust.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PackageSource {
    decoder_version: u32,
    archive: Vec<u8>,
    archive_sha256: String,
}

impl PackageSource {
    fn decode(&self) -> Result<package_transport::DecodedRuntimePackage, ImportError> {
        if self.decoder_version != 1
            || self.archive.len() > package_transport::MAX_PACKAGE_BYTES
            || digest(&self.archive) != self.archive_sha256
        {
            return Err(ImportError::InvalidBundle);
        }
        package_transport::decode_runtime_package(&self.archive)
            .map_err(|_| ImportError::InvalidBundle)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Envelope {
    format: String,
    version: u32,
    sha256: String,
    payload: Payload,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RecoveryBody {
    base_sha256: String,
    project: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RecoveryEnvelope {
    format: String,
    version: u32,
    sha256: String,
    body: RecoveryBody,
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn read_recovery_bytes(bundle: &Path, fs: &dyn FileSystem) -> Result<Option<Vec<u8>>, ImportError> {
    match fs.read_limited(&review_recovery_path(bundle), MAX_BUNDLE_BYTES) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(ImportError::InvalidRecovery),
    }
}

/// Identify even malformed recovery data before asking for a user's decision.
/// Only absence is treated as no recovery; unreadable data must be preserved.
pub fn review_recovery_identity(
    bundle: &Path,
    fs: &dyn FileSystem,
) -> Result<Option<String>, ImportError> {
    Ok(read_recovery_bytes(bundle, fs)?.map(|bytes| digest(&bytes)))
}

fn canonical(project: &Project) -> Result<String, ImportError> {
    save_project(project).map_err(|_| ImportError::InvalidBundle)
}

fn decode_project(text: &str) -> Result<Project, ImportError> {
    let project = load_project(text)
        .map_err(|_| ImportError::InvalidBundle)?
        .project;
    if project.extensions.get(MARKER) != Some(&marker()) || canonical(&project)? != text {
        return Err(ImportError::InvalidBundle);
    }
    Ok(project)
}

fn verify_resources(
    project: &Project,
    resources: &BTreeMap<String, Vec<u8>>,
) -> Result<(), ImportError> {
    let data = serde_json::to_value(project).map_err(|_| ImportError::InvalidBundle)?;
    let mut used = std::collections::BTreeSet::new();
    for resource in data
        .get("resources")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let path = resource["path"]
            .as_str()
            .ok_or(ImportError::InvalidBundle)?;
        let expected = resource["sha256"]
            .as_str()
            .ok_or(ImportError::InvalidBundle)?;
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
    let findings = report["findings"]
        .as_array()
        .ok_or(ImportError::InvalidBundle)?;
    let mut paths = std::collections::BTreeSet::new();
    let source_hash = report["sourceSha256"]
        .as_str()
        .ok_or(ImportError::InvalidBundle)?;
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
        let target = finding["target"]
            .as_str()
            .ok_or(ImportError::InvalidBundle)?;
        if !paths.insert(path)
            || !matches!(
                finding["disposition"].as_str(),
                Some("mapped" | "preserved")
            )
            || finding["severity"] == "blocking"
            || value.pointer(target).is_none()
        {
            return Err(ImportError::InvalidBundle);
        }
    }
    Ok(())
}

fn verify_payload(payload: &Payload) -> Result<Project, ImportError> {
    if payload.package_source.is_some() && payload.revision_source.is_some() {
        return Err(ImportError::InvalidBundle);
    }
    if let Some(package) = &payload.package_source {
        let decoded = package.decode()?;
        if decoded.project_bytes() != payload.source.as_bytes() {
            return Err(ImportError::InvalidBundle);
        }
    }
    if let Some(revision) = &payload.revision_source {
        if revision.project_bytes()? != payload.source.as_bytes() {
            return Err(ImportError::InvalidBundle);
        }
    }
    let original = decode_project(&payload.original_project)?;
    verify_report(&payload.report, &original)?;
    // A checksum is not proof of report completeness. Replay the exact admitted
    // bytes and compare all immutable import evidence, including resources.
    let replay = replay_portable(payload.source.as_bytes(), &payload.report)
        .ok_or(ImportError::InvalidBundle)?;
    if !replay.report.has_complete_mapping()
        || serde_json::to_value(replay.report).map_err(|_| ImportError::InvalidBundle)?
            != payload.report
        || replay.resource_bytes != payload.resources
        || canonical(&replay.project.ok_or(ImportError::InvalidBundle)?)?
            != payload.original_project
    {
        return Err(ImportError::InvalidBundle);
    }
    verify_resources(&original, &payload.resources)?;
    let current = decode_project(&payload.project)?;
    verify_review_edit(&original, &current, &payload.resources)?;
    Ok(current)
}

fn verify_review_edit(
    original: &Project,
    current: &Project,
    resources: &BTreeMap<String, Vec<u8>>,
) -> Result<(), ImportError> {
    verify_resources(current, resources)?;
    if original.id != current.id
        || [MARKER, "hmiforge.idMap", "hmiforge.standardCatalog"]
            .iter()
            .any(|key| original.extensions.get(*key) != current.extensions.get(*key))
    {
        return Err(ImportError::InvalidBundle);
    }
    Ok(())
}

/// Engineering session with immutable import evidence. The inner session
/// cannot be extracted or sent to normal project persistence.
pub struct QuarantinedImport {
    session: ProjectSession,
    payload: Payload,
    path: PathBuf,
    warnings: Vec<&'static str>,
    recovery_sha256: Option<String>,
    base_sha256: String,
}

/// A validated, still-unpublished package candidate. No caller can replace
/// its private evidence or bypass the portable mapping/replay checks.
pub struct PreparedPackageReview {
    payload: Payload,
    project: Project,
}

impl PreparedPackageReview {
    pub fn publish(
        self,
        target: &Path,
        config: SessionConfig,
        fs: Arc<dyn FileSystem>,
    ) -> Result<QuarantinedImport, ImportError> {
        QuarantinedImport::publish_prepared(self.payload, self.project, target, config, fs)
    }
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
        Self::import_with_source(source, None, None, target, config, fs)
    }

    /// Admit a verified revision snapshot through the same quarantine gate.
    /// The full source records are retained only in private review storage.
    pub fn import_revision(
        source: RevisionSource,
        target: &Path,
        config: SessionConfig,
        fs: Arc<dyn FileSystem>,
    ) -> Result<Self, ImportError> {
        let project_bytes = source.project_bytes()?.to_vec();
        Self::import_with_source(&project_bytes, Some(source), None, target, config, fs)
    }

    /// Admit a legacy runtime archive only through complete portable mapping
    /// and review quarantine. Its settings and signature grant no authority.
    /// The native caller must obtain consent to retain the exact archive.
    pub fn import_package(
        archive: &[u8],
        target: &Path,
        config: SessionConfig,
        fs: Arc<dyn FileSystem>,
    ) -> Result<Self, ImportError> {
        Self::prepare_package(archive)?.publish(target, config, fs)
    }

    /// Validate the complete candidate without filesystem writes, before the
    /// native caller obtains a destination lock or switches its workspace.
    pub fn prepare_package(archive: &[u8]) -> Result<PreparedPackageReview, ImportError> {
        let decoded =
            package_transport::decode_runtime_package(archive).map_err(|_| ImportError::Refused)?;
        let evidence = PackageSource {
            decoder_version: 1,
            archive: archive.to_vec(),
            archive_sha256: decoded.archive_sha256().to_owned(),
        };
        let (payload, project) =
            Self::prepare_import(decoded.project_bytes(), None, Some(evidence))?;
        Ok(PreparedPackageReview { payload, project })
    }

    fn import_with_source(
        source: &[u8],
        revision_source: Option<RevisionSource>,
        package_source: Option<PackageSource>,
        target: &Path,
        config: SessionConfig,
        fs: Arc<dyn FileSystem>,
    ) -> Result<Self, ImportError> {
        let (payload, project) = Self::prepare_import(source, revision_source, package_source)?;
        Self::publish_prepared(payload, project, target, config, fs)
    }

    fn prepare_import(
        source: &[u8],
        revision_source: Option<RevisionSource>,
        package_source: Option<PackageSource>,
    ) -> Result<(Payload, Project), ImportError> {
        let outcome = import_portable(source);
        if !outcome.report.has_complete_mapping() {
            return Err(ImportError::Refused);
        }
        let project = outcome.project.ok_or(ImportError::Refused)?;
        let text = canonical(&project)?;
        let payload = Payload {
            source: std::str::from_utf8(source)
                .map_err(|_| ImportError::Refused)?
                .to_owned(),
            original_project: text.clone(),
            project: text,
            report: serde_json::to_value(outcome.report).map_err(|_| ImportError::InvalidBundle)?,
            resources: outcome.resource_bytes,
            revision_source,
            package_source,
        };
        let project = verify_payload(&payload)?;
        Ok((payload, project))
    }

    fn publish_prepared(
        payload: Payload,
        project: Project,
        target: &Path,
        config: SessionConfig,
        fs: Arc<dyn FileSystem>,
    ) -> Result<Self, ImportError> {
        let (warnings, base_sha256) = write_new(&payload, target, fs.as_ref())?;
        Ok(Self {
            session: ProjectSession::with_fs(project, config, fs),
            payload,
            path: target.to_path_buf(),
            warnings,
            recovery_sha256: None,
            base_sha256,
        })
    }

    /// Open only the review envelope, verifying checksums and evidence.
    pub fn reopen(
        path: &Path,
        config: SessionConfig,
        fs: Arc<dyn FileSystem>,
    ) -> Result<Self, ImportError> {
        let bytes = fs.read_limited(path, MAX_BUNDLE_BYTES).map_err(|error| {
            if error.kind() == std::io::ErrorKind::InvalidData {
                ImportError::SizeLimit
            } else {
                ImportError::Io("read")
            }
        })?;
        let envelope: Envelope =
            serde_json::from_slice(&bytes).map_err(|_| ImportError::InvalidBundle)?;
        let serialized =
            serde_json::to_vec(&envelope.payload).map_err(|_| ImportError::InvalidBundle)?;
        if envelope.format != FORMAT
            || envelope.version != payload_version(&envelope.payload)
            || envelope.sha256 != digest(&serialized)
        {
            return Err(ImportError::InvalidBundle);
        }
        let project = verify_payload(&envelope.payload)?;
        Ok(Self {
            session: ProjectSession::with_fs(project, config, fs),
            payload: envelope.payload,
            path: path.to_path_buf(),
            warnings: Vec::new(),
            recovery_sha256: None,
            base_sha256: digest(&bytes),
        })
    }

    /// Recovery is deliberately separate from ordinary project autosave.
    /// The native caller must hold the bundle's sidecar writer lock.
    pub fn recover(
        path: &Path,
        config: SessionConfig,
        fs: Arc<dyn FileSystem>,
    ) -> Result<Self, ImportError> {
        let mut review = Self::reopen(path, config.clone(), fs.clone())?;
        let (project, recovery_sha256) = review.read_recovery()?;
        review.verify_recovery_base()?;
        review.session = ProjectSession::bare(project, config, fs);
        review.session.storage.autosaved_state = Some(review.session.current_state_id());
        review.recovery_sha256 = Some(recovery_sha256);
        Ok(review)
    }

    fn verify_recovery_base(&self) -> Result<(), ImportError> {
        let saved = self.session.fs.read_limited(&self.path, MAX_BUNDLE_BYTES);
        if !saved.is_ok_and(|bytes| digest(&bytes) == self.base_sha256) {
            return Err(ImportError::InvalidRecovery);
        }
        Ok(())
    }

    fn recovery_bytes(&self) -> Result<Option<Vec<u8>>, ImportError> {
        read_recovery_bytes(&self.path, self.session.fs.as_ref())
    }

    fn read_recovery(&self) -> Result<(Project, String), ImportError> {
        let bytes = self.recovery_bytes()?.ok_or(ImportError::InvalidRecovery)?;
        self.decode_recovery(&bytes)
    }

    fn decode_recovery(&self, bytes: &[u8]) -> Result<(Project, String), ImportError> {
        // A valid envelope for the same base is not necessarily the snapshot
        // this session published or recovered. Preserve external replacements.
        let recovery_sha256 = digest(bytes);
        if self
            .recovery_sha256
            .as_ref()
            .is_some_and(|owned| owned != &recovery_sha256)
        {
            return Err(ImportError::InvalidRecovery);
        }
        let envelope: RecoveryEnvelope =
            serde_json::from_slice(bytes).map_err(|_| ImportError::InvalidRecovery)?;
        let body = serde_json::to_vec(&envelope.body).map_err(|_| ImportError::InvalidRecovery)?;
        if envelope.format != RECOVERY_FORMAT
            || envelope.version != 1
            || envelope.sha256 != digest(&body)
            || envelope.body.base_sha256 != self.base_sha256
        {
            return Err(ImportError::InvalidRecovery);
        }
        let project = load_project(&envelope.body.project)
            .map_err(|_| ImportError::InvalidRecovery)?
            .project;
        let serialized = hmi_project_model::serialization::to_json_string(&project)
            .map_err(|_| ImportError::InvalidRecovery)?;
        if serialized != envelope.body.project {
            return Err(ImportError::InvalidRecovery);
        }
        let original = decode_project(&self.payload.original_project)?;
        verify_review_edit(&original, &project, &self.payload.resources)?;
        Ok((project, recovery_sha256))
    }

    pub fn autosave(&mut self) -> Result<AutosaveOutcome, ImportError> {
        let fs = self.session.fs.as_ref();
        let path = review_recovery_path(&self.path);
        let dir = sidecar_dir(&self.path);
        if !self.is_dirty() {
            if self.recovery_sha256.is_some() {
                self.discard_recovery()?;
            }
            return Ok(AutosaveOutcome::Clean);
        }
        // A recovery is bound to the exact saved envelope bytes, not just
        // a project ID or report revision. A changed base cannot be recovered.
        self.verify_recovery_base().inspect_err(|_| {
            self.session.storage.autosaved_state = None;
        })?;
        // Never replace an unresolved or foreign recovery slot.
        if let Some(bytes) = self.recovery_bytes().inspect_err(|_| {
            self.session.storage.autosaved_state = None;
        })? {
            if self.recovery_sha256.is_none() {
                self.session.storage.autosaved_state = None;
                return Err(ImportError::InvalidRecovery);
            }
            let (previous, _) = self.decode_recovery(&bytes).inspect_err(|_| {
                self.session.storage.autosaved_state = None;
            })?;
            if self.session.storage.autosaved_state == Some(self.session.current_state_id())
                && &previous == self.session.project()
            {
                return Ok(AutosaveOutcome::UpToDate);
            }
        }
        self.session.storage.autosaved_state = None;
        let original = decode_project(&self.payload.original_project)?;
        verify_review_edit(&original, self.session.project(), &self.payload.resources)?;
        let project = hmi_project_model::serialization::to_json_string(self.session.project())
            .map_err(|_| ImportError::InvalidRecovery)?;
        let body = RecoveryBody {
            base_sha256: self.base_sha256.clone(),
            project,
        };
        let serialized = serde_json::to_vec(&body).map_err(|_| ImportError::InvalidRecovery)?;
        let envelope = RecoveryEnvelope {
            format: RECOVERY_FORMAT.into(),
            version: 1,
            sha256: digest(&serialized),
            body,
        };
        let bytes = serde_json::to_vec(&envelope).map_err(|_| ImportError::InvalidRecovery)?;
        if bytes.len() > MAX_BUNDLE_BYTES {
            return Err(ImportError::SizeLimit);
        }
        fs.create_dir_all(&dir)
            .map_err(|_| ImportError::Io("recovery directory"))?;
        let tmp = stage_temp(fs, &path, &bytes, |p, b| fs.write_review_synced(p, b))
            .map_err(|_| ImportError::Io("recovery stage"))?;
        // The native sidecar lock serializes cooperating writers. Use one atomic
        // replacement, preserving the previous snapshot on failure.
        if fs.rename(&tmp, &path).is_err() {
            let _ = fs.remove_file(&tmp);
            return Err(ImportError::Io("recovery publish"));
        }
        self.recovery_sha256 = Some(digest(&bytes));
        fs.sync_dir(&dir)
            .map_err(|_| ImportError::Io("recovery sync"))?;
        self.session.storage.autosaved_state = Some(self.session.current_state_id());
        Ok(AutosaveOutcome::Written {
            path: path.display().to_string(),
            revision: self.session.revision(),
        })
    }

    /// Retire this session's recovery only after explicit discard or a committed save.
    pub fn discard_recovery(&mut self) -> Result<(), ImportError> {
        let fs = self.session.fs.as_ref();
        let path = review_recovery_path(&self.path);
        if self.recovery_sha256.is_some() {
            // Invalidate the advertised snapshot even when retirement fails.
            self.session.storage.autosaved_state = None;
            if let Some(bytes) = self.recovery_bytes()? {
                self.verify_recovery_base()?;
                self.decode_recovery(&bytes)?;
                match fs.remove_file(&path) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(_) => return Err(ImportError::Io("recovery remove")),
                }
            }
            fs.sync_dir(&sidecar_dir(&self.path))
                .map_err(|_| ImportError::Io("recovery sync"))?;
            self.recovery_sha256 = None;
        }
        Ok(())
    }

    pub fn snapshot(&self) -> ProjectSnapshot {
        let mut snapshot = self.session.snapshot();
        self.project_review_status(&mut snapshot.status);
        snapshot
    }

    fn project_review_status(&self, status: &mut crate::session::SessionStatus) {
        status.path = Some(self.path.display().to_string());
        status.autosave_current = self.recovery_current();
    }

    pub fn is_dirty(&self) -> bool {
        self.session.is_dirty()
    }

    /// A clean review is current only after its obsolete owned recovery has
    /// been retired. Generic project status treats every clean state as
    /// current, even if review cleanup failed and an old slot remains.
    pub fn recovery_current(&self) -> bool {
        if self.is_dirty() {
            self.recovery_sha256.is_some()
                && self.session.storage.autosaved_state == Some(self.session.current_state_id())
        } else {
            self.recovery_sha256.is_none()
        }
    }

    /// The review session exposes editing, validation and history only.
    /// Persistence/reload/close requests cannot reach the inner session.
    pub fn request(&mut self, request: crate::Request) -> crate::Response {
        if !matches!(
            &request,
            crate::Request::Snapshot {}
                | crate::Request::Status {}
                | crate::Request::Validate {}
                | crate::Request::Execute { .. }
                | crate::Request::Undo { .. }
                | crate::Request::Redo { .. }
        ) {
            return crate::Response::Error {
                error: crate::protocol::ErrorBody {
                    code: "importReviewRequired",
                    message: "Use review bundle save-copy; ordinary persistence is disabled".into(),
                    details: Value::Null,
                },
            };
        }
        let (mut response, _) = crate::dispatch(
            &mut self.session,
            request,
            &hmi_project_model::MigrationRegistry::builtin(),
        );
        // All review response paths must describe the same persistence state
        // as the review snapshot, including undo/redo and status queries.
        match &mut response {
            crate::Response::Snapshot { snapshot } => {
                self.project_review_status(&mut snapshot.status);
            }
            crate::Response::Status { status } | crate::Response::Nothing { status } => {
                self.project_review_status(status);
            }
            crate::Response::Changed { outcome } => {
                self.project_review_status(&mut outcome.status);
            }
            _ => {}
        }
        response
    }

    pub fn report(&self) -> &Value {
        &self.payload.report
    }

    /// Whether this review contains private revision-container evidence.
    pub fn has_revision_source(&self) -> bool {
        self.payload.revision_source.is_some()
    }

    /// Whether this review retains an exact legacy runtime archive.
    pub fn has_package_source(&self) -> bool {
        self.payload.package_source.is_some()
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Non-fatal durability or cleanup warnings from the last publication.
    pub fn warnings(&self) -> &[&'static str] {
        &self.warnings
    }

    pub fn execute(
        &mut self,
        transaction: Transaction,
    ) -> Result<TransactionOutcome, TransactionError> {
        let mut outcome = self.session.execute(transaction)?;
        self.project_review_status(&mut outcome.status);
        Ok(outcome)
    }

    /// Persist edits in a new review bundle; never overwrite an existing
    /// bundle or source. On failure the current in-memory state is unchanged.
    pub fn save_copy(&mut self, target: &Path) -> Result<(), ImportError> {
        let mut payload = self.payload.clone();
        payload.project = canonical(self.session.project())?;
        verify_payload(&payload)?;
        let (warnings, base_sha256) = write_new(&payload, target, self.session.fs.as_ref())?;
        let mut warnings = warnings;
        // A successful publication with a durability/cleanup warning must not
        // discard the last recovery snapshot. The user can explicitly retire
        // it after verifying the newly published copy.
        if self.recovery_sha256.is_some()
            && (!warnings.is_empty() || self.discard_recovery().is_err())
        {
            warnings.push("saved review copy; previous recovery retained");
        }
        self.session.mark_saved();
        self.session.storage.autosaved_state = None;
        self.recovery_sha256 = None;
        self.payload = payload;
        self.base_sha256 = base_sha256;
        self.path = target.to_path_buf();
        self.warnings = warnings;
        Ok(())
    }
}

fn payload_version(payload: &Payload) -> u32 {
    if payload.package_source.is_some() {
        4
    } else if payload.revision_source.is_some() {
        3
    } else {
        2
    }
}

fn write_new(
    payload: &Payload,
    target: &Path,
    fs: &dyn FileSystem,
) -> Result<(Vec<&'static str>, String), ImportError> {
    let body = serde_json::to_vec(payload).map_err(|_| ImportError::InvalidBundle)?;
    let envelope = Envelope {
        format: FORMAT.into(),
        version: payload_version(payload),
        sha256: digest(&body),
        payload: payload.clone(),
    };
    let bytes = serde_json::to_vec(&envelope).map_err(|_| ImportError::InvalidBundle)?;
    if bytes.len() > MAX_BUNDLE_BYTES {
        return Err(ImportError::SizeLimit);
    }
    let tmp = stage_temp(fs, target, &bytes, |path, data| {
        fs.write_review_synced(path, data)
    })
    .map_err(|_| ImportError::Io("stage"))?;
    // Unlike rename, an exclusive same-volume hard link cannot replace an
    // existing destination even if it appears after the staging operation.
    let result = fs.link_new(&tmp, target);
    let cleanup = fs.unlink_staged(&tmp);
    result.map_err(|_| ImportError::Io("publish new bundle"))?;
    let mut warnings = Vec::new();
    if cleanup.is_err() {
        warnings.push("published bundle; temporary file cleanup failed");
    }
    let parent = target
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    // Publication already committed; do not report rollback after this point.
    if fs.sync_dir(parent).is_err() {
        warnings.push("published bundle; directory sync failed");
    }
    Ok((warnings, digest(&bytes)))
}
