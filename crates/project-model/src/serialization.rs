//! Canonical JSON serialization, version detection and loading.
//!
//! ## Load pipeline
//!
//! 1. Parse the text as generic JSON (syntax errors carry line/column).
//! 2. [`detect_version`]: check the `format` marker and `schemaVersion`.
//!    Newer-than-supported and too-old documents are rejected explicitly
//!    with [`LoadError::UnsupportedVersion`]; nothing is guessed.
//! 3. Apply registered migrations on the raw JSON, one version at a time.
//! 4. Deserialize into typed structs. Unknown fields are errors, so data is
//!    never silently dropped; failures report the JSON path.
//! 5. ([`load_project`] only) validate; any error-severity diagnostic fails
//!    the load with the full [`ValidationReport`].
//!
//! ## Canonical form
//!
//! [`to_json_string`] writes UTF-8, two-space indented JSON with a trailing
//! newline. Struct fields appear in declaration order, maps are sorted by
//! key, empty optional collections and default values are omitted. The same
//! model therefore always produces byte-identical output, and
//! `serialize(parse(serialize(p))) == serialize(p)`.
//! Every save re-parses its own output and compares it with the input model,
//! so values JSON cannot represent (NaN, infinities) are reported instead of
//! being silently written as `null`.

use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::Value;

use crate::migration::{MigrationError, MigrationRegistry, MigrationStep};
use crate::model::Project;
use crate::validation::{validate, ValidationReport};
use crate::{CURRENT_SCHEMA_VERSION, FORMAT_ID};

/// Why a document could not be loaded.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    /// Not valid JSON (or nested deeper than the parser limit).
    #[error("invalid JSON at line {line}, column {column}: {message}")]
    Syntax {
        /// 1-based line.
        line: usize,
        /// 1-based column.
        column: usize,
        /// Parser message.
        message: String,
    },
    /// Valid JSON but not an HMI Studio project document.
    #[error("not an HMI Studio project document: {0}")]
    NotAProject(String),
    /// `schemaVersion` is missing or not a non-negative integer.
    #[error("missing or malformed schemaVersion")]
    MissingVersion,
    /// The schema version cannot be handled by this build.
    #[error(
        "unsupported schema version {found}: this build supports versions \
         {oldest_supported} to {newest_supported}{hint}",
        hint = if *found > u64::from(*newest_supported) {
            " (the project was saved by a newer HMI Studio)"
        } else {
            ""
        }
    )]
    UnsupportedVersion {
        /// Version found in the document.
        found: u64,
        /// Oldest version that can be migrated.
        oldest_supported: u32,
        /// Version written by this build.
        newest_supported: u32,
    },
    /// A migration step failed.
    #[error(transparent)]
    Migration(#[from] MigrationError),
    /// The document does not match the model structure.
    #[error("invalid project structure at {path}: {message}")]
    Structure {
        /// JSON Pointer to the offending value (`""` = root).
        path: String,
        /// Deserializer message.
        message: String,
    },
    /// The project deserialized but failed validation.
    #[error("project failed validation with {} error(s)", .0.error_count())]
    Invalid(ValidationReport),
}

impl LoadError {
    fn from_json(e: &serde_json::Error) -> Self {
        let mut message = e.to_string();
        // serde_json appends " at line X column Y"; we report them separately.
        if let Some(pos) = message.rfind(" at line ") {
            message.truncate(pos);
        }
        LoadError::Syntax {
            line: e.line(),
            column: e.column(),
            message,
        }
    }
}

/// Why a project could not be saved.
#[derive(Debug, thiserror::Error)]
pub enum SaveError {
    /// Serialization failed.
    #[error("serialization failed: {0}")]
    Serialize(String),
    /// The serialized JSON would not load back to the same model (for
    /// example a NaN or infinite number somewhere in the model).
    #[error("project cannot be represented losslessly as JSON: {0}")]
    NotLossless(String),
    /// [`save_project`] refused to write a project with validation errors.
    #[error("project has {} validation error(s)", .0.error_count())]
    Invalid(ValidationReport),
}

/// A parsed (but not necessarily valid) project.
#[derive(Clone, Debug, PartialEq)]
pub struct ParsedProject {
    /// The project, upgraded to [`CURRENT_SCHEMA_VERSION`].
    pub project: Project,
    /// Version found in the source document.
    pub source_version: u32,
    /// Migrations applied, in order.
    pub migrations: Vec<MigrationStep>,
}

/// A parsed and validated project.
#[derive(Clone, Debug, PartialEq)]
pub struct LoadedProject {
    /// The project, upgraded to [`CURRENT_SCHEMA_VERSION`].
    pub project: Project,
    /// Version found in the source document.
    pub source_version: u32,
    /// Migrations applied, in order.
    pub migrations: Vec<MigrationStep>,
    /// Validation report (contains warnings only).
    pub report: ValidationReport,
}

// Decode every object without serde_json::Value's last-key-wins behavior.
struct UniqueValue(Value);

impl<'de> Deserialize<'de> for UniqueValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct UniqueVisitor;
        impl<'de> Visitor<'de> for UniqueVisitor {
            type Value = UniqueValue;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a JSON value with unique object keys")
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::Bool(v)))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::from(v)))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::from(v)))
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> Result<Self::Value, E> {
                serde_json::Number::from_f64(v)
                    .map(|n| UniqueValue(Value::Number(n)))
                    .ok_or_else(|| E::custom("non-finite JSON number"))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::String(v.to_owned())))
            }
            fn visit_string<E: de::Error>(self, v: String) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::String(v)))
            }
            fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::Null))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut items = Vec::new();
                while let Some(UniqueValue(v)) = seq.next_element()? {
                    items.push(v);
                }
                Ok(UniqueValue(Value::Array(items)))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut items = serde_json::Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    if items.contains_key(&key) {
                        return Err(de::Error::custom(format!(
                            "duplicate JSON object key {key:?}"
                        )));
                    }
                    let UniqueValue(value) = map.next_value()?;
                    items.insert(key, value);
                }
                Ok(UniqueValue(Value::Object(items)))
            }
        }
        deserializer.deserialize_any(UniqueVisitor)
    }
}

fn parse_value(input: &str) -> Result<Value, LoadError> {
    serde_json::from_str::<UniqueValue>(input)
        .map(|v| v.0)
        .map_err(|e| LoadError::from_json(&e))
}

/// Reads `format` and `schemaVersion` from a JSON document without
/// deserializing the rest. Does not consult migrations.
pub fn detect_version(input: &str) -> Result<u32, LoadError> {
    let value = parse_value(input)?;
    detect_version_value(&value)
}

/// [`detect_version`] for an already parsed document.
pub fn detect_version_value(value: &Value) -> Result<u32, LoadError> {
    let object = value
        .as_object()
        .ok_or_else(|| LoadError::NotAProject("root must be a JSON object".into()))?;
    match object.get("format") {
        None => return Err(LoadError::NotAProject("missing \"format\" field".into())),
        Some(Value::String(s)) if s == FORMAT_ID => {}
        Some(other) => {
            return Err(LoadError::NotAProject(format!(
                "\"format\" is {other}, expected \"{FORMAT_ID}\""
            )))
        }
    }
    let version = object
        .get("schemaVersion")
        .and_then(Value::as_u64)
        .ok_or(LoadError::MissingVersion)?;
    u32::try_from(version).map_err(|_| LoadError::UnsupportedVersion {
        found: version,
        oldest_supported: MigrationRegistry::builtin().oldest_supported(CURRENT_SCHEMA_VERSION),
        newest_supported: CURRENT_SCHEMA_VERSION,
    })
}

/// Parses a project using the built-in migrations, without validation.
pub fn parse_project(input: &str) -> Result<ParsedProject, LoadError> {
    parse_project_with(input, &MigrationRegistry::builtin())
}

/// Parses a project using the given migrations, without validation.
pub fn parse_project_with(
    input: &str,
    registry: &MigrationRegistry,
) -> Result<ParsedProject, LoadError> {
    let value = parse_value(input)?;
    parse_value_with(value, registry)
}

/// Parses an already decoded JSON document. Duplicate keys must have been
/// rejected by the caller before decoding; they cannot be recovered from Value.
pub fn parse_value_with(
    value: Value,
    registry: &MigrationRegistry,
) -> Result<ParsedProject, LoadError> {
    let source_version = detect_version_value(&value)?;
    let oldest = registry.oldest_supported(CURRENT_SCHEMA_VERSION);
    if source_version > CURRENT_SCHEMA_VERSION || source_version < oldest {
        return Err(LoadError::UnsupportedVersion {
            found: u64::from(source_version),
            oldest_supported: oldest,
            newest_supported: CURRENT_SCHEMA_VERSION,
        });
    }
    let (value, migrations) = registry.migrate(value, source_version, CURRENT_SCHEMA_VERSION)?;
    let project: Project = serde_path_to_error::deserialize(value).map_err(|e| {
        let path = json_pointer(e.path());
        LoadError::Structure {
            path,
            message: e.into_inner().to_string(),
        }
    })?;
    Ok(ParsedProject {
        project,
        source_version,
        migrations,
    })
}

/// Parses and validates a project using the built-in migrations.
pub fn load_project(input: &str) -> Result<LoadedProject, LoadError> {
    load_project_with(input, &MigrationRegistry::builtin())
}

/// Parses and validates a project using the given migrations.
pub fn load_project_with(
    input: &str,
    registry: &MigrationRegistry,
) -> Result<LoadedProject, LoadError> {
    let parsed = parse_project_with(input, registry)?;
    let report = validate(&parsed.project);
    if report.has_errors() {
        return Err(LoadError::Invalid(report));
    }
    Ok(LoadedProject {
        project: parsed.project,
        source_version: parsed.source_version,
        migrations: parsed.migrations,
        report,
    })
}

/// Serializes a project to canonical JSON (no validation, lossless check).
pub fn to_json_string(project: &Project) -> Result<String, SaveError> {
    let mut text =
        serde_json::to_string_pretty(project).map_err(|e| SaveError::Serialize(e.to_string()))?;
    text.push('\n');
    let reparsed: Project = serde_path_to_error::deserialize(
        &mut serde_json::Deserializer::from_str(&text),
    )
    .map_err(|e| {
        SaveError::NotLossless(format!(
            "value at {} does not survive JSON: {}",
            json_pointer(e.path()),
            e.inner()
        ))
    })?;
    if reparsed != *project {
        return Err(SaveError::NotLossless(
            "serialized JSON loads back to a different model (non-finite number?)".into(),
        ));
    }
    Ok(text)
}

/// Validates, then serializes to canonical JSON. Refuses invalid projects.
pub fn save_project(project: &Project) -> Result<String, SaveError> {
    let report = validate(project);
    if report.has_errors() {
        return Err(SaveError::Invalid(report));
    }
    to_json_string(project)
}

/// Converts a `serde_path_to_error` path to an RFC 6901 JSON Pointer.
fn json_pointer(path: &serde_path_to_error::Path) -> String {
    use serde_path_to_error::Segment;
    let mut out = String::new();
    for segment in path.iter() {
        match segment {
            Segment::Seq { index } => {
                out.push('/');
                out.push_str(&index.to_string());
            }
            Segment::Map { key } => {
                out.push('/');
                out.push_str(&key.replace('~', "~0").replace('/', "~1"));
            }
            Segment::Enum { .. } | Segment::Unknown => {}
        }
    }
    out
}
