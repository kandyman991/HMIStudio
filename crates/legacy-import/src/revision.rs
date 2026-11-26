//! Bounded, read-only decoding of a published legacy revision snapshot.
//! No filesystem access, repair, history replay, or deployment admission.

use crate::MAX_INPUT_BYTES;
use serde::{de, Deserialize};
use serde_json::value::RawValue;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

/// Maximum bytes in the bundle format marker.
pub const MAX_DESCRIPTOR_BYTES: usize = 4096;
/// Maximum bytes in a checksummed HEAD, including its checksum line.
pub const MAX_HEAD_BYTES: usize = 256 * 1024;
/// Maximum bytes in a checksummed revision, including inert history metadata.
pub const MAX_REVISION_BYTES: usize = 8 * 1024 * 1024;
const MAX_HISTORY_IDS: usize = 200;
const MAX_RECENT_TRANSACTION_IDS: usize = 512;
// Bound collection overhead as well as encoded bytes; historical command
// payloads are not needed for snapshot migration.
const MAX_RECORD_NODES: usize = 200_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RevisionError {
    SizeLimit,
    InvalidDescriptor,
    InvalidChecksum,
    InvalidRecord,
    UnsupportedStorageVersion,
    InvalidRevisionId,
    RevisionMismatch,
    InvalidSequence,
}

impl std::fmt::Display for RevisionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::SizeLimit => "Legacy revision storage size limit exceeded",
            Self::InvalidDescriptor => "Invalid legacy bundle descriptor",
            Self::InvalidChecksum => "Invalid legacy revision record checksum",
            Self::InvalidRecord => "Invalid legacy revision record",
            Self::UnsupportedStorageVersion => "Unsupported legacy storage version",
            Self::InvalidRevisionId => "Invalid legacy revision identifier",
            Self::RevisionMismatch => "Revision does not match the selected HEAD",
            Self::InvalidSequence => "Invalid legacy revision sequence",
        })
    }
}
impl std::error::Error for RevisionError {}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn bounded(bytes: &[u8], max: usize) -> Result<(), RevisionError> {
    if bytes.len() > max {
        return Err(RevisionError::SizeLimit);
    }
    Ok(())
}

// Streaming duplicate-key validation: do not build a second full JSON tree
// (especially for large arrays or ignored historical transaction payloads).
struct Check<'a>(&'a mut usize);

impl<'de> de::DeserializeSeed<'de> for Check<'_> {
    type Value = ();

    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
        if *self.0 == 0 {
            return Err(de::Error::custom("record node limit"));
        }
        *self.0 -= 1;
        d.deserialize_any(self)
    }
}

impl<'de> de::Visitor<'de> for Check<'_> {
    type Value = ();

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("bounded unique-key JSON")
    }
    fn visit_bool<E: de::Error>(self, _: bool) -> Result<(), E> {
        Ok(())
    }
    fn visit_i64<E: de::Error>(self, _: i64) -> Result<(), E> {
        Ok(())
    }
    fn visit_u64<E: de::Error>(self, _: u64) -> Result<(), E> {
        Ok(())
    }
    fn visit_f64<E: de::Error>(self, _: f64) -> Result<(), E> {
        Ok(())
    }
    fn visit_str<E: de::Error>(self, _: &str) -> Result<(), E> {
        Ok(())
    }
    fn visit_unit<E: de::Error>(self) -> Result<(), E> {
        Ok(())
    }

    fn visit_seq<A: de::SeqAccess<'de>>(self, mut a: A) -> Result<(), A::Error> {
        while a.next_element_seed(Check(&mut *self.0))?.is_some() {}
        Ok(())
    }

    fn visit_map<A: de::MapAccess<'de>>(self, mut a: A) -> Result<(), A::Error> {
        let mut keys = BTreeSet::new();
        while let Some(key) = a.next_key::<String>()? {
            if !keys.insert(key) {
                return Err(de::Error::custom("duplicate object key"));
            }
            a.next_value_seed(Check(&mut *self.0))?;
        }
        Ok(())
    }
}

pub(crate) fn unique(bytes: &[u8]) -> Result<(), RevisionError> {
    use serde::de::DeserializeSeed;
    let mut budget = MAX_RECORD_NODES;
    let mut decoder = serde_json::Deserializer::from_slice(bytes);
    Check(&mut budget).deserialize(&mut decoder).map_err(|_| {
        if budget == 0 {
            RevisionError::SizeLimit
        } else {
            RevisionError::InvalidRecord
        }
    })?;
    decoder.end().map_err(|_| RevisionError::InvalidRecord)
}

fn body(bytes: &[u8], max: usize) -> Result<&[u8], RevisionError> {
    bounded(bytes, max)?;
    if bytes.get(64) != Some(&b'\n') {
        return Err(RevisionError::InvalidChecksum);
    }
    let payload = &bytes[65..];
    if digest(payload).as_bytes() != &bytes[..64] {
        return Err(RevisionError::InvalidChecksum);
    }
    // Reject duplicate keys, invalid UTF-8/JSON and excessive nesting even
    // inside history fields that will not be transferred.
    unique(payload)?;
    Ok(payload)
}

fn revision_number(id: &str) -> Result<i64, RevisionError> {
    let b = id.as_bytes();
    if b.len() != 19
        || b[10] != b'-'
        || !b[..10].iter().all(u8::is_ascii_digit)
        || !b[11..]
            .iter()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(c))
    {
        return Err(RevisionError::InvalidRevisionId);
    }
    id[..10]
        .parse::<i64>()
        .map_err(|_| RevisionError::InvalidRevisionId)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Descriptor {
    format: String,
    format_version: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Head {
    format_version: u32,
    sequence: i64,
    current_revision_id: String,
    next_revision_number: i64,
    #[serde(default)]
    undo: Vec<String>,
    #[serde(default)]
    redo: Vec<String>,
    #[serde(default)]
    recent_transaction_ids: Vec<String>,
}

/// A validated selection from a checksummed HEAD. The only revision path
/// exposed is derived from the exact validated identifier, never user paths.
pub struct RevisionSelection {
    id: String,
    number: i64,
    head_sha256: String,
    descriptor_sha256: String,
}

impl RevisionSelection {
    pub fn revision_id(&self) -> &str {
        &self.id
    }

    pub fn revision_relative_path(&self) -> String {
        format!("revisions/{}.json", self.id)
    }

    /// Check that the originally selected marker and HEAD still match the
    /// bytes read by the caller. A future native reader must re-read both
    /// before admitting the snapshot, under its read-only storage policy.
    pub fn matches_source(&self, descriptor: &[u8], head: &[u8]) -> bool {
        descriptor.len() <= MAX_DESCRIPTOR_BYTES
            && head.len() <= MAX_HEAD_BYTES
            && digest(descriptor) == self.descriptor_sha256
            && digest(head) == self.head_sha256
    }
}

/// Select only the published current revision. Never fall back to HEAD.previous
/// or promote an orphaned revision.
pub fn select_current_revision(
    descriptor: &[u8],
    head: &[u8],
) -> Result<RevisionSelection, RevisionError> {
    bounded(descriptor, MAX_DESCRIPTOR_BYTES)?;
    unique(descriptor).map_err(|_| RevisionError::InvalidDescriptor)?;
    let marker: Descriptor =
        serde_json::from_slice(descriptor).map_err(|_| RevisionError::InvalidDescriptor)?;
    if marker.format_version != 1 {
        return Err(RevisionError::UnsupportedStorageVersion);
    }
    if marker.format != "hmistudio-project-bundle" {
        return Err(RevisionError::InvalidDescriptor);
    }

    let h: Head = serde_json::from_slice(body(head, MAX_HEAD_BYTES)?)
        .map_err(|_| RevisionError::InvalidRecord)?;
    if h.format_version != 1 {
        return Err(RevisionError::UnsupportedStorageVersion);
    }
    let number = revision_number(&h.current_revision_id)?;
    if h.sequence <= 0 || number <= 0 || h.next_revision_number <= number {
        return Err(RevisionError::InvalidSequence);
    }
    if h.undo.len() > MAX_HISTORY_IDS
        || h.redo.len() > MAX_HISTORY_IDS
        || h.recent_transaction_ids.len() > MAX_RECENT_TRANSACTION_IDS
    {
        return Err(RevisionError::SizeLimit);
    }
    // Stack entries are not opened or replayed. Validate their identifiers
    // nevertheless so malformed path references are never accepted.
    for id in h.undo.iter().chain(&h.redo) {
        let n = revision_number(id)?;
        if n <= 0 || n >= h.next_revision_number {
            return Err(RevisionError::InvalidSequence);
        }
    }

    Ok(RevisionSelection {
        id: h.current_revision_id,
        number,
        head_sha256: digest(head),
        descriptor_sha256: digest(descriptor),
    })
}

#[derive(Deserialize)]
enum RevisionKind {
    #[serde(rename = "CREATED")]
    Created,
    #[serde(rename = "IMPORTED")]
    Imported,
    #[serde(rename = "TRANSACTION")]
    Transaction,
    #[serde(rename = "RESTORE")]
    Restore,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Revision<'a> {
    format_version: u32,
    id: String,
    number: i64,
    #[serde(default)]
    parent_id: Option<String>,
    kind: RevisionKind,
    #[serde(default, rename = "label")]
    _label: String,
    #[serde(default, rename = "createdAtEpochMs")]
    _created_at_epoch_ms: i64,
    // These are never deserialized into commands or executed.
    #[serde(default, borrow)]
    transaction: Option<&'a RawValue>,
    #[serde(default)]
    restored_from_id: Option<String>,
    #[serde(borrow)]
    project: &'a RawValue,
}

/// Verified transport snapshot, not an admitted canonical project.
/// No Debug implementation: project bytes may contain sensitive local data.
pub struct VerifiedRevision<'a> {
    project: &'a [u8],
    revision_sha256: String,
}

impl<'a> VerifiedRevision<'a> {
    /// Exact embedded JSON, without reserialization or ordering changes.
    pub fn project_bytes(&self) -> &'a [u8] {
        self.project
    }

    /// Digest of the complete checksummed revision envelope.
    pub fn revision_sha256(&self) -> &str {
        &self.revision_sha256
    }
}

/// Decode the selected revision, verifying the envelope and binding the
/// embedded project to the published ID. Call import_portable separately;
/// this decoder does not grant any trust or approve project semantics.
pub fn decode_current_revision<'a>(
    selection: &RevisionSelection,
    bytes: &'a [u8],
) -> Result<VerifiedRevision<'a>, RevisionError> {
    let r: Revision<'_> = serde_json::from_slice(body(bytes, MAX_REVISION_BYTES)?)
        .map_err(|_| RevisionError::InvalidRecord)?;
    if r.format_version != 1 {
        return Err(RevisionError::UnsupportedStorageVersion);
    }
    if revision_number(&r.id)? != r.number || r.id != selection.id || r.number != selection.number {
        return Err(RevisionError::RevisionMismatch);
    }
    for id in [r.parent_id.as_deref(), r.restored_from_id.as_deref()]
        .into_iter()
        .flatten()
    {
        let n = revision_number(id)?;
        if n <= 0 || n >= r.number {
            return Err(RevisionError::InvalidSequence);
        }
    }
    // Match the pinned writer's revision shapes, without interpreting the
    // transaction vocabulary or replaying any historical operation.
    let valid_kind = match r.kind {
        RevisionKind::Created | RevisionKind::Imported => {
            r.number == 1
                && r.parent_id.is_none()
                && r.transaction.is_none()
                && r.restored_from_id.is_none()
        }
        RevisionKind::Transaction => {
            r.parent_id.is_some()
                && r.restored_from_id.is_none()
                && r.transaction.is_some_and(|t| t.get().starts_with('{'))
        }
        RevisionKind::Restore => {
            r.parent_id.is_some() && r.restored_from_id.is_some() && r.transaction.is_none()
        }
    };
    if !valid_kind {
        return Err(RevisionError::InvalidRecord);
    }
    let project = r.project.get().as_bytes();
    bounded(project, MAX_INPUT_BYTES)?;
    if project.first() != Some(&b'{') {
        return Err(RevisionError::InvalidRecord);
    }
    Ok(VerifiedRevision {
        project,
        revision_sha256: digest(bytes),
    })
}
