//! Compatibility invariants for REUSE-03A. Keep these tests when v2 is activated.
#![allow(clippy::unwrap_used)]

mod common;
use common::*;
use hmi_project_model::serialization::{load_project, parse_project, to_json_string, LoadError};
use hmi_project_model::CURRENT_SCHEMA_VERSION;
use serde_json::{json, Value};

#[test]
fn v1_preserves_exact_values_references_and_extensions() {
    let mut source = base();
    source["extensions"] =
        json!({"synthetic.audit": {"counter": u64::MAX, "nested": [null, true, "opaque"]}});
    source["tags"].as_array_mut().unwrap().push(json!({
        "id": "tag-exact", "name": "Exact", "dataType": "uint64",
        "access": "readWrite", "initialValue": u64::MAX,
        "extensions": {"synthetic.note": "preserve"}
    }));
    let input = source.to_string();
    let loaded = load_project(&input).unwrap();
    assert_eq!(loaded.source_version, 1);
    assert_eq!(loaded.project.schema_version, CURRENT_SCHEMA_VERSION);
    let text = to_json_string(&loaded.project).unwrap();
    let output: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(output["extensions"], source["extensions"]);
    assert_eq!(output["tags"][6], source["tags"][6]);
    assert_eq!(output["connections"][0], source["connections"][0]);
    assert_eq!(
        output["screens"][0]["root"]["children"][2]["bindings"],
        source["screens"][0]["root"]["children"][2]["bindings"]
    );
    assert_eq!(
        output["screens"][0]["root"]["children"][3]["events"],
        source["screens"][0]["root"]["children"][3]["events"]
    );
    assert_eq!(
        text,
        to_json_string(&load_project(&input).unwrap().project).unwrap()
    );
    assert_eq!(
        text,
        to_json_string(&load_project(&text).unwrap().project).unwrap()
    );
}

#[test]
fn v1_duplicate_keys_and_unknown_fields_are_not_silently_discarded() {
    let text = base().to_string();
    let duplicate = text.replacen(
        r#""schemaVersion":1"#,
        r#""schemaVersion":1,"schemaVersion":1"#,
        1,
    );
    assert_ne!(duplicate, text);
    assert!(matches!(
        parse_project(&duplicate),
        Err(LoadError::Syntax { .. })
    ));
    let mut source = base();
    source["unrecognizedSafetyPolicy"] = json!({"allow": true});
    assert!(matches!(
        parse_project(&source.to_string()),
        Err(LoadError::Structure { .. }) | Err(LoadError::Migration(_))
    ));
}
