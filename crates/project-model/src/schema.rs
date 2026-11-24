//! Machine-readable JSON Schema (draft 2020-12) of the project format.
//!
//! The schema is generated from the Rust types, so it cannot drift from the
//! serializer. A committed copy lives at
//! `crates/project-model/schema/hmi-project.v1.schema.json` for consumers
//! that do not build Rust (the Kotlin runtime, external tooling); a test
//! fails if the committed copy is stale. Regenerate with
//! `cargo run --example export_schema`.
//!
//! The JSON Schema describes **structure** only. Cross-references, property
//! types per component kind, hierarchy rules and the other semantic checks
//! are performed by [`crate::validation`].

use serde_json::Value;

use crate::model::Project;
use crate::CURRENT_SCHEMA_VERSION;

/// `$id` of the current schema.
pub const SCHEMA_ID: &str = "urn:hmi-studio:project:schema:v1";

/// Relative path of the committed schema inside the crate.
pub const SCHEMA_FILE: &str = "schema/hmi-project.v1.schema.json";

/// Builds the JSON Schema for [`Project`].
pub fn project_json_schema() -> Value {
    let schema = schemars::schema_for!(Project);
    let mut value = schema.to_value();
    if let Some(obj) = value.as_object_mut() {
        obj.insert("$id".into(), Value::String(SCHEMA_ID.into()));
        obj.insert(
            "description".into(),
            Value::String(format!(
                "HMI Studio project document, schemaVersion {CURRENT_SCHEMA_VERSION}. \
                 Structural schema only; semantic rules are enforced by the \
                 hmi-project-model validator (see docs/PROJECT_MODEL_VALIDATION.md)."
            )),
        );
        if let Some(Value::Object(props)) = obj.get_mut("properties") {
            props.insert(
                "schemaVersion".into(),
                serde_json::json!({
                    "description": "Schema version of this document.",
                    "const": CURRENT_SCHEMA_VERSION
                }),
            );
            props.insert(
                "format".into(),
                serde_json::json!({
                    "description": "Format marker.",
                    "const": crate::FORMAT_ID
                }),
            );
        }
    }
    value
}

/// The schema as canonical, pretty-printed text with a trailing newline.
pub fn project_json_schema_string() -> String {
    // Serializing a `serde_json::Value` cannot fail.
    let mut text = serde_json::to_string_pretty(&project_json_schema()).unwrap_or_default();
    text.push('\n');
    text
}
