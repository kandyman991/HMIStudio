//! Bounded transport-only decoding of published legacy runtime packages.
//! No filesystem extraction, installation, signature trust, or runtime admission.
use crate::{revision, MAX_INPUT_BYTES};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{de, Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read};
use zip::{CompressionMethod, ZipArchive};

/// A stricter transport bound than the legacy runtime; portable JSON has
/// its existing 4 MiB limit. Only ordinary single-disk ZIPs are supported.
pub const MAX_PACKAGE_BYTES: usize = 8 * 1024 * 1024;
const MAX_METADATA_BYTES: usize = 64 * 1024;
const MAX_CENTRAL_BYTES: usize = 64 * 1024;
const MANIFEST: &str = "manifest.json";
const PROJECT: &str = "project.json";
const RUNTIME: &str = "runtime.json";
const SIGNATURE: &str = "signature.json";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PackageError {
    SizeLimit,
    InvalidArchive,
    UnexpectedEntry,
    InvalidManifest,
    UnsupportedFormat,
    InvalidContents,
    InvalidSignature,
    InvalidProject,
    InvalidSettings,
}

impl std::fmt::Display for PackageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::SizeLimit => "Legacy runtime package exceeds import size limits",
            Self::InvalidArchive => "Invalid or unsupported legacy ZIP archive",
            Self::UnexpectedEntry => "Unexpected or duplicate legacy ZIP entry",
            Self::InvalidManifest => "Invalid legacy runtime package manifest",
            Self::UnsupportedFormat => "Unsupported legacy runtime package format",
            Self::InvalidContents => "Legacy runtime package content integrity check failed",
            Self::InvalidSignature => "Invalid legacy runtime package signature metadata",
            Self::InvalidProject => "Legacy runtime package project metadata mismatch",
            Self::InvalidSettings => "Invalid legacy runtime settings",
        })
    }
}
impl std::error::Error for PackageError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignatureStatus {
    Unsigned,
    /// Structurally checked only; no cryptographic or publisher trust claim.
    PresentUnverified,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LegacyValues {
    Simulated,
    Live,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LegacyRuntimeSettings {
    pub values: LegacyValues,
    pub full_screen: bool,
    pub status_bar: bool,
    pub keep_screen_on: bool,
}

/// Integrity-checked, inert source transport, not a canonical project or
/// trusted deployment. No Debug implementation for private source data.
pub struct DecodedRuntimePackage {
    project: Vec<u8>,
    package_id: String,
    manifest_schema_version: u32,
    archive_sha256: String,
    signature_status: SignatureStatus,
    settings: LegacyRuntimeSettings,
}

impl DecodedRuntimePackage {
    pub fn project_bytes(&self) -> &[u8] {
        &self.project
    }
    pub fn package_id(&self) -> &str {
        &self.package_id
    }
    /// The manifest's claimed minimum schema, not a fully validated feature floor.
    pub fn manifest_schema_version(&self) -> u32 {
        self.manifest_schema_version
    }
    pub fn archive_sha256(&self) -> &str {
        &self.archive_sha256
    }
    pub fn signature_status(&self) -> SignatureStatus {
        self.signature_status
    }
    /// Inert source settings, never host driver or deployment settings.
    pub fn source_settings(&self) -> LegacyRuntimeSettings {
        self.settings
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Manifest {
    format: u32,
    package_id: String,
    project_name: String,
    version: String,
    schema_version: u32,
    created_at_ms: i64,
    created_by: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    startup_screen: Option<String>,
    files: Vec<ManifestFile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sequence: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    publisher_key_id: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManifestFile {
    path: String,
    sha256: String,
    size_bytes: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Signature {
    algorithm: String,
    key_id: String,
    value: String,
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn entry_limit(name: &[u8]) -> Result<usize, PackageError> {
    match name {
        b"project.json" => Ok(MAX_INPUT_BYTES),
        b"manifest.json" | b"runtime.json" | b"signature.json" => Ok(MAX_METADATA_BYTES),
        _ => Err(PackageError::UnexpectedEntry),
    }
}

fn u16_at(bytes: &[u8], at: usize) -> Result<u16, PackageError> {
    let end = at.checked_add(2).ok_or(PackageError::InvalidArchive)?;
    let v = bytes.get(at..end).ok_or(PackageError::InvalidArchive)?;
    Ok(u16::from_le_bytes([v[0], v[1]]))
}

fn u32_at(bytes: &[u8], at: usize) -> Result<u32, PackageError> {
    let end = at.checked_add(4).ok_or(PackageError::InvalidArchive)?;
    let v = bytes.get(at..end).ok_or(PackageError::InvalidArchive)?;
    Ok(u32::from_le_bytes([v[0], v[1], v[2], v[3]]))
}

// The pinned writer may emit an extended timestamp when the fixed DOS
// timestamp falls outside the local time zone's representable interval.
// Do not allow ZIP64, Unicode path aliases, encryption, or unknown extras.
fn check_extra(extra: &[u8]) -> Result<(), PackageError> {
    if extra.is_empty() {
        return Ok(());
    }
    if extra.len() > 17
        || u16_at(extra, 0)? != 0x5455
        || usize::from(u16_at(extra, 2)?) + 4 != extra.len()
        || !matches!(extra.len(), 5 | 9 | 13 | 17)
        || extra[4] & !7 != 0
    {
        return Err(PackageError::InvalidArchive);
    }
    Ok(())
}

/// Bound the directory before giving untrusted data to the ZIP library.
/// Only the ordinary, single-disk end record with no archive comment is admitted.
fn directory_bounds(bytes: &[u8]) -> Result<(usize, usize), PackageError> {
    if bytes.len() > MAX_PACKAGE_BYTES {
        return Err(PackageError::SizeLimit);
    }
    let end = bytes
        .len()
        .checked_sub(22)
        .ok_or(PackageError::InvalidArchive)?;
    if u32_at(bytes, end)? != 0x0605_4b50
        || u16_at(bytes, end + 4)? != 0
        || u16_at(bytes, end + 6)? != 0
        || u16_at(bytes, end + 20)? != 0
    {
        return Err(PackageError::InvalidArchive);
    }
    let count = usize::from(u16_at(bytes, end + 10)?);
    let central_size = u32_at(bytes, end + 12)? as usize;
    let central = u32_at(bytes, end + 16)? as usize;
    if !(3..=4).contains(&count)
        || usize::from(u16_at(bytes, end + 8)?) != count
        || central_size > MAX_CENTRAL_BYTES
        || central.checked_add(central_size) != Some(end)
    {
        return Err(PackageError::InvalidArchive);
    }
    // Bound actual central headers, not only the declared entry count.
    // Reject encryption, unsupported methods/flags, split disks and ZIP64 sizes.
    let mut at = central;
    let mut names = BTreeSet::new();
    for _ in 0..count {
        if at.checked_add(46).is_none_or(|n| n > end) || u32_at(bytes, at)? != 0x0201_4b50 {
            return Err(PackageError::InvalidArchive);
        }
        let flags = u16_at(bytes, at + 8)?;
        let method = u16_at(bytes, at + 10)?;
        let compressed = u32_at(bytes, at + 20)? as usize;
        let size = u32_at(bytes, at + 24)? as usize;
        let name_len = usize::from(u16_at(bytes, at + 28)?);
        let extra_len = usize::from(u16_at(bytes, at + 30)?);
        let local = u32_at(bytes, at + 42)? as usize;
        let next = at + 46 + name_len + extra_len;
        if flags & !0x080e != 0
            || !matches!(method, 0 | 8)
            || (method == 0 && flags & 6 != 0)
            || u16_at(bytes, at + 32)? != 0
            || u16_at(bytes, at + 34)? != 0
            || next > end
            || local >= central
            || compressed > central
            || (method == 0 && compressed != size)
        {
            return Err(PackageError::InvalidArchive);
        }
        let name = &bytes[at + 46..at + 46 + name_len];
        if size > entry_limit(name)? {
            return Err(PackageError::SizeLimit);
        }
        if !names.insert(name) {
            return Err(PackageError::UnexpectedEntry);
        }
        check_extra(&bytes[at + 46 + name_len..next])?;
        // Bound and inspect the raw local header and its extra fields before
        // the general ZIP parser touches them. No alias or ZIP64 expansion.
        if local.checked_add(30).is_none_or(|n| n > central)
            || u32_at(bytes, local)? != 0x0403_4b50
            || u16_at(bytes, local + 6)? != flags
            || u16_at(bytes, local + 8)? != method
            || usize::from(u16_at(bytes, local + 26)?) != name_len
        {
            return Err(PackageError::InvalidArchive);
        }
        let extra_start = local + 30 + name_len;
        let data = extra_start + usize::from(u16_at(bytes, local + 28)?);
        if data > central
            || bytes.get(local + 30..extra_start) != Some(name)
            || data.checked_add(compressed).is_none_or(|n| n > central)
        {
            return Err(PackageError::InvalidArchive);
        }
        check_extra(&bytes[extra_start..data])?;
        // A streaming local header may have zero CRC and size fields, but
        // it must not contradict its central header.
        for (local_field, central_field) in [(14, 16), (18, 20), (22, 24)] {
            let local_value = u32_at(bytes, local + local_field)?;
            let central_value = u32_at(bytes, at + central_field)?;
            if local_value != central_value && (flags & 8 == 0 || local_value != 0) {
                return Err(PackageError::InvalidArchive);
            }
        }
        at = next;
    }
    if at != end {
        return Err(PackageError::InvalidArchive);
    }
    Ok((central, count))
}

fn unpack(bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, PackageError> {
    let (central, count) = directory_bounds(bytes)?;
    let mut zip = ZipArchive::new(Cursor::new(bytes)).map_err(|_| PackageError::InvalidArchive)?;
    if zip.len() != count
        || zip.offset() != 0
        || zip.central_directory_start() != central as u64
        || zip.has_duplicate_names()
    {
        return Err(PackageError::InvalidArchive);
    }
    zip.validate_header_consistency()
        .map_err(|_| PackageError::InvalidArchive)?;
    let mut entries = BTreeMap::new();
    let mut ranges = Vec::with_capacity(count);
    for index in 0..count {
        let mut file = zip
            .by_index(index)
            .map_err(|_| PackageError::InvalidArchive)?;
        let name = file
            .name()
            .map_err(|_| PackageError::UnexpectedEntry)?
            .into_owned();
        let limit = entry_limit(file.name_raw())?;
        if name.as_bytes() != file.name_raw()
            || !file.is_file()
            || file.encrypted()
            || file
                .unix_mode()
                .is_some_and(|mode| !matches!(mode & 0o170000, 0 | 0o100000))
            || !matches!(
                file.compression(),
                CompressionMethod::Stored | CompressionMethod::Deflated
            )
        {
            return Err(PackageError::UnexpectedEntry);
        }
        if file.size() > limit as u64 {
            return Err(PackageError::SizeLimit);
        }
        let local =
            usize::try_from(file.header_start()).map_err(|_| PackageError::InvalidArchive)?;
        let data = usize::try_from(file.data_start().ok_or(PackageError::InvalidArchive)?)
            .map_err(|_| PackageError::InvalidArchive)?;
        let compressed =
            usize::try_from(file.compressed_size()).map_err(|_| PackageError::InvalidArchive)?;
        // Compare original local and central filename bytes too. Unicode path
        // extra fields must not turn a different raw name into an allowed one.
        if local >= central || data > central || local + 30 > data {
            return Err(PackageError::InvalidArchive);
        }
        let central_header = usize::try_from(file.central_header_start())
            .map_err(|_| PackageError::InvalidArchive)?;
        if central_header < central
            || central_header
                .checked_add(46)
                .is_none_or(|n| n > bytes.len())
        {
            return Err(PackageError::InvalidArchive);
        }
        let name_len = usize::from(u16_at(bytes, local + 26)?);
        if name_len != name.len()
            || bytes.get(local + 30..local + 30 + name_len) != Some(name.as_bytes())
            || usize::from(u16_at(bytes, central_header + 28)?) != name.len()
            || bytes.get(central_header + 46..central_header + 46 + name.len())
                != Some(name.as_bytes())
            || u16_at(bytes, local + 6)? != file.flags().as_u16()
        {
            return Err(PackageError::InvalidArchive);
        }
        let mut end = data
            .checked_add(compressed)
            .filter(|n| *n <= central)
            .ok_or(PackageError::InvalidArchive)?;
        // Java's streaming writer uses a signed 32-bit data descriptor.
        if file.flags().is_using_data_descriptor() {
            if end + 16 > central
                || u32_at(bytes, end)? != 0x0807_4b50
                || u32_at(bytes, end + 4)? != file.crc32()
                || u64::from(u32_at(bytes, end + 8)?) != file.compressed_size()
                || u64::from(u32_at(bytes, end + 12)?) != file.size()
            {
                return Err(PackageError::InvalidArchive);
            }
            end += 16;
        }
        ranges.push((local, end));
        let expected = file.size() as usize;
        let mut decoded = Vec::with_capacity(expected);
        // Read no more than the claimed bounded output plus one byte; on
        // success read to EOF so the library performs its CRC check.
        (&mut file)
            .take(expected as u64 + 1)
            .read_to_end(&mut decoded)
            .map_err(|_| PackageError::InvalidContents)?;
        if decoded.len() != expected {
            return Err(PackageError::InvalidContents);
        }
        if entries.insert(name, decoded).is_some() {
            return Err(PackageError::UnexpectedEntry);
        }
    }
    ranges.sort_unstable();
    let mut end = 0;
    for (start, next) in ranges {
        if start != end {
            return Err(PackageError::InvalidArchive);
        }
        end = next;
    }
    if end != central {
        return Err(PackageError::InvalidArchive);
    }
    Ok(entries)
}

fn parse<T: de::DeserializeOwned>(bytes: &[u8], error: PackageError) -> Result<T, PackageError> {
    revision::unique(bytes).map_err(|_| error)?;
    serde_json::from_slice(bytes).map_err(|_| error)
}

fn package_identity(manifest: &mut Manifest) -> Result<String, PackageError> {
    if manifest.format == 1 {
        let lines: Vec<_> = manifest
            .files
            .iter()
            .map(|file| format!("{}:{}:{}", file.path, file.sha256, file.size_bytes))
            .collect();
        return Ok(digest(lines.join("\n").as_bytes()));
    }
    // Signed-format identity uses the canonical four-space manifest with ID
    // blank. The raw manifest is NOT thereby cryptographically authenticated.
    let id = std::mem::take(&mut manifest.package_id);
    let mut canonical = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(b"    ");
    let result = manifest.serialize(&mut serde_json::Serializer::with_formatter(
        &mut canonical,
        formatter,
    ));
    manifest.package_id = id;
    result.map_err(|_| PackageError::InvalidManifest)?;
    Ok(digest(&canonical))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectHeader {
    schema_version: u32,
    metadata: ProjectMetadata,
    screens: OrderedScreenKeys,
    #[serde(default)]
    screen_order: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectMetadata {
    name: String,
    #[serde(default)]
    default_screen_id: Option<String>,
}

// JSON object member order matters for legacy startup fallback.
struct OrderedScreenKeys(Vec<String>);

impl<'de> Deserialize<'de> for OrderedScreenKeys {
    fn deserialize<D: de::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> de::Visitor<'de> for Visitor {
            type Value = OrderedScreenKeys;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("legacy screen object")
            }
            fn visit_map<A: de::MapAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
                let mut keys = Vec::new();
                while let Some((key, _)) = a.next_entry::<String, de::IgnoredAny>()? {
                    keys.push(key);
                }
                Ok(OrderedScreenKeys(keys))
            }
        }
        d.deserialize_map(Visitor)
    }
}

fn verify_project(bytes: &[u8], manifest: &Manifest) -> Result<(), PackageError> {
    let header: ProjectHeader = parse(bytes, PackageError::InvalidProject)?;
    let screens = &header.screens.0;
    let startup = header
        .metadata
        .default_screen_id
        .as_ref()
        .filter(|id| screens.contains(id))
        .or_else(|| header.screen_order.iter().find(|id| screens.contains(id)))
        .or_else(|| screens.iter().find(|id| !id.starts_with("definition:")));
    // The writer puts the minimum required schema in the manifest but retains
    // higher declared project versions. Exact minimum-feature semantics and
    // full project validation belong to subsequent portable migration.
    if header.metadata.name != manifest.project_name
        || !matches!(header.schema_version, 1..=10)
        || header.schema_version < manifest.schema_version
        || startup != manifest.startup_screen.as_ref()
    {
        return Err(PackageError::InvalidProject);
    }
    Ok(())
}

/// Decode the transport and retain the exact portable project bytes. This
/// accepts neither executable runtime configuration nor publisher authority.
pub fn decode_runtime_package(bytes: &[u8]) -> Result<DecodedRuntimePackage, PackageError> {
    let mut entries = unpack(bytes)?;
    let manifest_bytes = entries
        .remove(MANIFEST)
        .ok_or(PackageError::InvalidManifest)?;
    let mut manifest: Manifest = parse(&manifest_bytes, PackageError::InvalidManifest)?;
    if !matches!(manifest.format, 1 | 2) {
        return Err(PackageError::UnsupportedFormat);
    }
    if !matches!(manifest.schema_version, 1..=10)
        || manifest.version.trim().is_empty()
        || manifest.version.encode_utf16().count() > 40
        || manifest.files.len() != 2
    {
        return Err(PackageError::InvalidManifest);
    }
    let signature_status = if manifest.format == 1 {
        if entries.contains_key(SIGNATURE)
            || manifest.sequence.is_some()
            || manifest.publisher_key_id.is_some()
        {
            return Err(PackageError::InvalidSignature);
        }
        SignatureStatus::Unsigned
    } else {
        let raw = entries
            .remove(SIGNATURE)
            .ok_or(PackageError::InvalidSignature)?;
        let signature: Signature = parse(&raw, PackageError::InvalidSignature)?;
        if signature.algorithm.trim().is_empty()
            || signature.key_id.trim().is_empty()
            || manifest.publisher_key_id.as_ref() != Some(&signature.key_id)
            || manifest.sequence.is_none_or(|n| n <= 0)
            || !matches!(STANDARD.decode(&signature.value), Ok(v) if !v.is_empty())
        {
            return Err(PackageError::InvalidSignature);
        }
        SignatureStatus::PresentUnverified
    };
    let mut seen = BTreeSet::new();
    for file in &manifest.files {
        if !matches!(file.path.as_str(), PROJECT | RUNTIME) || !seen.insert(file.path.as_str()) {
            return Err(PackageError::InvalidManifest);
        }
        let content = entries
            .get(&file.path)
            .ok_or(PackageError::InvalidContents)?;
        if content.len() != file.size_bytes as usize || digest(content) != file.sha256 {
            return Err(PackageError::InvalidContents);
        }
    }
    if package_identity(&mut manifest)? != manifest.package_id {
        return Err(PackageError::InvalidContents);
    }
    let project = entries
        .remove(PROJECT)
        .ok_or(PackageError::InvalidContents)?;
    let settings = entries
        .remove(RUNTIME)
        .ok_or(PackageError::InvalidContents)?;
    if !entries.is_empty() {
        return Err(PackageError::UnexpectedEntry);
    }
    verify_project(&project, &manifest)?;
    let settings = parse(&settings, PackageError::InvalidSettings)?;
    Ok(DecodedRuntimePackage {
        project,
        package_id: manifest.package_id,
        manifest_schema_version: manifest.schema_version,
        archive_sha256: digest(bytes),
        signature_status,
        settings,
    })
}
