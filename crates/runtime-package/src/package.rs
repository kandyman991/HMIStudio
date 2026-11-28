//! Deterministic format-1 (unsigned) and format-2 (signed) `.hmipkg` serializer.
//!
//! Produces single-disk ordinary ZIP archives with fixed DOS timestamp
//! `1980-01-01 00:00:00`, canonical entry order (`manifest.json`, `project.json`,
//! `runtime.json`, and `signature.json` for format 2), 4-space pretty-printed
//! `manifest.json`, and exact `packageId` digests matching HMI Forge format 1/2.

use std::io::{Cursor, Read, Write};

use hmi_legacy_import::package::{decode_runtime_package, MAX_PACKAGE_BYTES};
use hmi_legacy_import::MAX_INPUT_BYTES;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipArchive, ZipWriter};

use crate::signing::{verify_manifest_signature, ManifestSigner, PackageSignature, SigningError};

const MANIFEST_ENTRY: &str = "manifest.json";
const PROJECT_ENTRY: &str = "project.json";
const RUNTIME_ENTRY: &str = "runtime.json";
const SIGNATURE_ENTRY: &str = "signature.json";
const MAX_METADATA_BYTES: usize = 64 * 1024;

/// Package wire format version.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PackageFormat {
    /// Format 1: unsigned package (`manifest.json`, `project.json`, `runtime.json`).
    Format1Unsigned,
    /// Format 2: signed package (`manifest.json`, `project.json`, `runtime.json`, `signature.json`).
    Format2Signed,
}

/// ZIP compression method for package entries.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Compression {
    /// Standard DEFLATE compression (default, matching `ZipOutputStream`).
    #[default]
    Deflated,
    /// Uncompressed STORED entries.
    Stored,
}

/// Legacy runtime data-source mode serialized into `runtime.json`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RuntimeValues {
    /// Isolated simulated runtime values (requires no production approval per D-004).
    Simulated,
    /// Live external PLC/OPC UA values (requires declared external source + mapping).
    Live,
}

/// Legacy `runtime.json` payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeSettings {
    pub values: RuntimeValues,
    pub full_screen: bool,
    pub status_bar: bool,
    pub keep_screen_on: bool,
}

impl Default for RuntimeSettings {
    fn default() -> Self {
        Self {
            values: RuntimeValues::Simulated,
            full_screen: true,
            status_bar: true,
            keep_screen_on: true,
        }
    }
}

/// One integrity-checked file entry inside `manifest.json`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManifestFile {
    pub path: String,
    pub sha256: String,
    pub size_bytes: u32,
}

/// Canonical `manifest.json` wire model (field order matches HMI Forge format 1/2).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PackageManifest {
    pub format: u32,
    pub package_id: String,
    pub project_name: String,
    pub version: String,
    pub schema_version: u32,
    pub created_at_ms: i64,
    pub created_by: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub startup_screen: Option<String>,
    pub files: Vec<ManifestFile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sequence: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publisher_key_id: Option<String>,
}

/// Metadata required to build an unsigned format-1 runtime package.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnsignedPackageMeta {
    pub version: String,
    pub created_at_ms: i64,
    pub created_by: String,
}

/// Metadata required to build a signed format-2 runtime package.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedPackageMeta {
    pub version: String,
    pub created_at_ms: i64,
    pub created_by: String,
    /// Strictly positive monotonic deployment sequence number (`> 0`).
    pub sequence: i64,
}

/// Deterministic built package artifact and its exact wire components.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuiltPackage {
    format: PackageFormat,
    package_id: String,
    archive_sha256: String,
    manifest_json: Vec<u8>,
    runtime_json: Vec<u8>,
    signature_json: Option<Vec<u8>>,
    archive_bytes: Vec<u8>,
}

impl BuiltPackage {
    pub fn format(&self) -> PackageFormat {
        self.format
    }

    pub fn package_id(&self) -> &str {
        &self.package_id
    }

    pub fn archive_sha256(&self) -> &str {
        &self.archive_sha256
    }

    pub fn manifest_json(&self) -> &[u8] {
        &self.manifest_json
    }

    pub fn runtime_json(&self) -> &[u8] {
        &self.runtime_json
    }

    pub fn signature_json(&self) -> Option<&[u8]> {
        self.signature_json.as_deref()
    }

    pub fn archive_bytes(&self) -> &[u8] {
        &self.archive_bytes
    }
}

/// Verified format-2 archive summary returned by [`verify_signed_package_archive`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedPackageSummary {
    pub package_id: String,
    pub archive_sha256: String,
    pub schema_version: u32,
    pub sequence: i64,
    pub publisher_key_id: String,
}

/// Fail-closed package build and verification errors.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PackageBuildError {
    #[error("project payload exceeds maximum package input size")]
    SizeLimit,
    #[error("project payload is not valid legacy HmiProject JSON")]
    InvalidProjectJson,
    #[error("invalid package metadata: {0}")]
    InvalidMetadata(&'static str),
    #[error("LIVE runtime settings require at least one valid external source mapping")]
    LiveMappingRequired,
    #[error("manifest or runtime metadata serialization failed")]
    SerializationFailed,
    #[error("signing failed: {0}")]
    Signing(#[from] SigningError),
    #[error("ZIP archive assembly failed")]
    ArchiveWriteFailed,
    #[error("assembled archive failed self-verification: {0}")]
    SelfVerificationFailed(String),
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn format_manifest_4space(manifest: &PackageManifest) -> Result<Vec<u8>, PackageBuildError> {
    let mut out = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(b"    ");
    let mut ser = serde_json::Serializer::with_formatter(&mut out, formatter);
    manifest
        .serialize(&mut ser)
        .map_err(|_| PackageBuildError::SerializationFailed)?;
    if out.len() > MAX_METADATA_BYTES {
        return Err(PackageBuildError::SizeLimit);
    }
    Ok(out)
}

struct ExtractedProjectInfo {
    project_name: String,
    schema_version: u32,
    startup_screen: Option<String>,
    has_live_mapping: bool,
}

fn inspect_project_json(project_json: &[u8]) -> Result<ExtractedProjectInfo, PackageBuildError> {
    if project_json.is_empty() || project_json.len() > MAX_INPUT_BYTES {
        return Err(PackageBuildError::SizeLimit);
    }
    let _ = std::str::from_utf8(project_json).map_err(|_| PackageBuildError::InvalidProjectJson)?;
    let value: Value =
        serde_json::from_slice(project_json).map_err(|_| PackageBuildError::InvalidProjectJson)?;
    let schema_version = value
        .get("schemaVersion")
        .and_then(Value::as_u64)
        .and_then(|v| u32::try_from(v).ok())
        .filter(|v| (1..=10).contains(v))
        .ok_or(PackageBuildError::InvalidProjectJson)?;
    let project_name = value
        .get("metadata")
        .and_then(|m| m.get("name"))
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or(PackageBuildError::InvalidProjectJson)?
        .to_owned();
    let screens = value
        .get("screens")
        .and_then(Value::as_object)
        .ok_or(PackageBuildError::InvalidProjectJson)?;
    let default_screen = value
        .get("metadata")
        .and_then(|m| m.get("defaultScreenId"))
        .and_then(Value::as_str)
        .filter(|id| screens.contains_key(*id));
    let order_first = value
        .get("screenOrder")
        .and_then(Value::as_array)
        .and_then(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .find(|id| screens.contains_key(*id))
        });
    let fallback_first = screens
        .keys()
        .find(|id| !id.starts_with("definition:"))
        .map(String::as_str);
    let startup_screen = default_screen
        .or(order_first)
        .or(fallback_first)
        .map(str::to_owned);

    let has_live_mapping = match (
        value.get("externalSources").and_then(Value::as_object),
        value.get("variables").and_then(Value::as_object),
        value.get("externalMappings").and_then(Value::as_object),
    ) {
        (Some(sources), Some(variables), Some(mappings)) => mappings.values().any(|m| {
            m.get("sourceId")
                .and_then(Value::as_str)
                .is_some_and(|id| sources.contains_key(id))
                && m.get("internalVariableId")
                    .and_then(Value::as_str)
                    .is_some_and(|id| variables.contains_key(id))
        }),
        _ => false,
    };

    Ok(ExtractedProjectInfo {
        project_name,
        schema_version,
        startup_screen,
        has_live_mapping,
    })
}

fn validate_common_meta(
    version: &str,
    created_at_ms: i64,
    created_by: &str,
) -> Result<(), PackageBuildError> {
    if version.trim().is_empty() {
        return Err(PackageBuildError::InvalidMetadata(
            "version must be non-empty",
        ));
    }
    if created_at_ms < 0 {
        return Err(PackageBuildError::InvalidMetadata(
            "createdAtMs must be non-negative",
        ));
    }
    if created_by.trim().is_empty() {
        return Err(PackageBuildError::InvalidMetadata(
            "createdBy must be non-empty",
        ));
    }
    Ok(())
}

fn write_deterministic_zip(
    entries: &[(&str, &[u8])],
    compression: Compression,
) -> Result<Vec<u8>, PackageBuildError> {
    let method = match compression {
        Compression::Deflated => CompressionMethod::Deflated,
        Compression::Stored => CompressionMethod::Stored,
    };
    let fixed_epoch = DateTime::from_date_and_time(1980, 1, 1, 0, 0, 0)
        .map_err(|_| PackageBuildError::ArchiveWriteFailed)?;
    let options = SimpleFileOptions::default()
        .compression_method(method)
        .last_modified_time(fixed_epoch)
        .large_file(false);

    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer = ZipWriter::new(&mut cursor);
        for &(name, bytes) in entries {
            writer
                .start_file(name, options)
                .map_err(|_| PackageBuildError::ArchiveWriteFailed)?;
            writer
                .write_all(bytes)
                .map_err(|_| PackageBuildError::ArchiveWriteFailed)?;
        }
        writer
            .finish()
            .map_err(|_| PackageBuildError::ArchiveWriteFailed)?;
    }
    let archive = cursor.into_inner();
    if archive.len() > MAX_PACKAGE_BYTES {
        return Err(PackageBuildError::SizeLimit);
    }
    Ok(archive)
}

/// Build a deterministic format-1 (unsigned) `.hmipkg` runtime package.
pub fn build_unsigned_package(
    project_json: &[u8],
    settings: RuntimeSettings,
    meta: UnsignedPackageMeta,
    compression: Compression,
) -> Result<BuiltPackage, PackageBuildError> {
    validate_common_meta(&meta.version, meta.created_at_ms, &meta.created_by)?;
    let info = inspect_project_json(project_json)?;
    if settings.values == RuntimeValues::Live && !info.has_live_mapping {
        return Err(PackageBuildError::LiveMappingRequired);
    }

    let runtime_json =
        serde_json::to_vec(&settings).map_err(|_| PackageBuildError::SerializationFailed)?;
    let project_sha = sha256_hex(project_json);
    let runtime_sha = sha256_hex(&runtime_json);
    let project_size =
        u32::try_from(project_json.len()).map_err(|_| PackageBuildError::SizeLimit)?;
    let runtime_size =
        u32::try_from(runtime_json.len()).map_err(|_| PackageBuildError::SizeLimit)?;

    let package_id = sha256_hex(
        format!(
            "{PROJECT_ENTRY}:{project_sha}:{project_size}\n{RUNTIME_ENTRY}:{runtime_sha}:{runtime_size}"
        )
        .as_bytes(),
    );
    let manifest = PackageManifest {
        format: 1,
        package_id: package_id.clone(),
        project_name: info.project_name,
        version: meta.version,
        schema_version: info.schema_version,
        created_at_ms: meta.created_at_ms,
        created_by: meta.created_by,
        startup_screen: info.startup_screen,
        files: vec![
            ManifestFile {
                path: PROJECT_ENTRY.to_owned(),
                sha256: project_sha,
                size_bytes: project_size,
            },
            ManifestFile {
                path: RUNTIME_ENTRY.to_owned(),
                sha256: runtime_sha,
                size_bytes: runtime_size,
            },
        ],
        sequence: None,
        publisher_key_id: None,
    };
    let manifest_json = format_manifest_4space(&manifest)?;
    let archive_bytes = write_deterministic_zip(
        &[
            (MANIFEST_ENTRY, &manifest_json),
            (PROJECT_ENTRY, project_json),
            (RUNTIME_ENTRY, &runtime_json),
        ],
        compression,
    )?;

    // Self-verify against the strict independent transport decoder.
    let decoded = decode_runtime_package(&archive_bytes)
        .map_err(|e| PackageBuildError::SelfVerificationFailed(format!("{e:?}")))?;
    if decoded.package_id() != package_id || decoded.project_bytes() != project_json {
        return Err(PackageBuildError::SelfVerificationFailed(
            "decoded identity or project bytes diverged".into(),
        ));
    }

    Ok(BuiltPackage {
        format: PackageFormat::Format1Unsigned,
        package_id,
        archive_sha256: sha256_hex(&archive_bytes),
        manifest_json,
        runtime_json,
        signature_json: None,
        archive_bytes,
    })
}

/// Build a deterministic format-2 (signed) `.hmipkg` runtime package.
///
/// The `packageId` is the SHA-256 of the canonical 4-space `manifest.json` with
/// `packageId: ""`, and `signer` signs the finalized `manifest.json` bytes.
pub fn build_signed_package(
    project_json: &[u8],
    settings: RuntimeSettings,
    meta: SignedPackageMeta,
    signer: &dyn ManifestSigner,
    compression: Compression,
) -> Result<BuiltPackage, PackageBuildError> {
    validate_common_meta(&meta.version, meta.created_at_ms, &meta.created_by)?;
    if meta.sequence <= 0 {
        return Err(PackageBuildError::InvalidMetadata(
            "format-2 sequence must be > 0",
        ));
    }
    let key_id = signer.key_id();
    if key_id.trim().is_empty() {
        return Err(PackageBuildError::Signing(SigningError::InvalidKeyId));
    }
    let info = inspect_project_json(project_json)?;
    if settings.values == RuntimeValues::Live && !info.has_live_mapping {
        return Err(PackageBuildError::LiveMappingRequired);
    }

    let runtime_json =
        serde_json::to_vec(&settings).map_err(|_| PackageBuildError::SerializationFailed)?;
    let project_sha = sha256_hex(project_json);
    let runtime_sha = sha256_hex(&runtime_json);
    let project_size =
        u32::try_from(project_json.len()).map_err(|_| PackageBuildError::SizeLimit)?;
    let runtime_size =
        u32::try_from(runtime_json.len()).map_err(|_| PackageBuildError::SizeLimit)?;

    let mut manifest = PackageManifest {
        format: 2,
        package_id: String::new(),
        project_name: info.project_name,
        version: meta.version,
        schema_version: info.schema_version,
        created_at_ms: meta.created_at_ms,
        created_by: meta.created_by,
        startup_screen: info.startup_screen,
        files: vec![
            ManifestFile {
                path: PROJECT_ENTRY.to_owned(),
                sha256: project_sha,
                size_bytes: project_size,
            },
            ManifestFile {
                path: RUNTIME_ENTRY.to_owned(),
                sha256: runtime_sha,
                size_bytes: runtime_size,
            },
        ],
        sequence: Some(meta.sequence),
        publisher_key_id: Some(key_id.to_owned()),
    };

    // Format 2 packageId = SHA-256 of canonical 4-space manifest with blank packageId.
    let canonical_blank = format_manifest_4space(&manifest)?;
    let package_id = sha256_hex(&canonical_blank);
    manifest.package_id = package_id.clone();
    let manifest_json = format_manifest_4space(&manifest)?;

    let signature = signer.sign_manifest(&manifest_json)?;
    if signature.key_id != key_id {
        return Err(PackageBuildError::Signing(SigningError::KeyIdMismatch));
    }
    let signature_json =
        serde_json::to_vec(&signature).map_err(|_| PackageBuildError::SerializationFailed)?;
    if signature_json.len() > MAX_METADATA_BYTES {
        return Err(PackageBuildError::SizeLimit);
    }

    let archive_bytes = write_deterministic_zip(
        &[
            (MANIFEST_ENTRY, &manifest_json),
            (PROJECT_ENTRY, project_json),
            (RUNTIME_ENTRY, &runtime_json),
            (SIGNATURE_ENTRY, &signature_json),
        ],
        compression,
    )?;

    // Self-verify against the strict independent transport decoder.
    let decoded = decode_runtime_package(&archive_bytes)
        .map_err(|e| PackageBuildError::SelfVerificationFailed(format!("{e:?}")))?;
    if decoded.package_id() != package_id || decoded.project_bytes() != project_json {
        return Err(PackageBuildError::SelfVerificationFailed(
            "decoded identity or project bytes diverged".into(),
        ));
    }

    Ok(BuiltPackage {
        format: PackageFormat::Format2Signed,
        package_id,
        archive_sha256: sha256_hex(&archive_bytes),
        manifest_json,
        runtime_json,
        signature_json: Some(signature_json),
        archive_bytes,
    })
}

/// Strictly decode a format-2 archive AND cryptographically verify its Ed25519
/// `signature.json` over the exact `manifest.json` bytes inside the ZIP.
pub fn verify_signed_package_archive(
    archive_bytes: &[u8],
    public_key_bytes: &[u8; 32],
    expected_key_id: &str,
) -> Result<VerifiedPackageSummary, PackageBuildError> {
    let decoded = decode_runtime_package(archive_bytes)
        .map_err(|e| PackageBuildError::SelfVerificationFailed(format!("{e:?}")))?;
    let mut zip = ZipArchive::new(Cursor::new(archive_bytes))
        .map_err(|_| PackageBuildError::SelfVerificationFailed("zip open failed".into()))?;
    let read_entry = |zip: &mut ZipArchive<Cursor<&[u8]>>, name: &str| {
        let f = zip
            .by_name(name)
            .map_err(|_| PackageBuildError::SelfVerificationFailed("missing entry".into()))?;
        let mut buf = Vec::new();
        f.take((MAX_METADATA_BYTES + 1) as u64)
            .read_to_end(&mut buf)
            .map_err(|_| PackageBuildError::SelfVerificationFailed("read failed".into()))?;
        if buf.len() > MAX_METADATA_BYTES {
            return Err(PackageBuildError::SizeLimit);
        }
        Ok(buf)
    };
    let manifest_bytes = read_entry(&mut zip, MANIFEST_ENTRY)?;
    let signature_bytes = read_entry(&mut zip, SIGNATURE_ENTRY)?;
    let manifest: PackageManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|_| PackageBuildError::SelfVerificationFailed("invalid manifest".into()))?;
    if manifest.format != 2 {
        return Err(PackageBuildError::SelfVerificationFailed(
            "archive is not format 2".into(),
        ));
    }
    let sequence = manifest
        .sequence
        .filter(|&s| s > 0)
        .ok_or_else(|| PackageBuildError::SelfVerificationFailed("missing sequence".into()))?;
    let publisher_key_id = manifest
        .publisher_key_id
        .ok_or_else(|| PackageBuildError::SelfVerificationFailed("missing keyId".into()))?;
    if publisher_key_id != expected_key_id {
        return Err(PackageBuildError::Signing(SigningError::KeyIdMismatch));
    }
    let signature: PackageSignature = serde_json::from_slice(&signature_bytes)
        .map_err(|_| PackageBuildError::SelfVerificationFailed("invalid signature json".into()))?;
    verify_manifest_signature(
        public_key_bytes,
        expected_key_id,
        &manifest_bytes,
        &signature,
    )?;

    Ok(VerifiedPackageSummary {
        package_id: decoded.package_id().to_owned(),
        archive_sha256: decoded.archive_sha256().to_owned(),
        schema_version: decoded.manifest_schema_version(),
        sequence,
        publisher_key_id,
    })
}
