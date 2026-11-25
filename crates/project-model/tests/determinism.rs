//! Deterministic serialization and validation.
#![allow(clippy::unwrap_used)]

mod common;

use std::collections::BTreeMap;

use common::*;
use hmi_project_model::model::*;
use hmi_project_model::serialization::{parse_project, save_project, to_json_string, SaveError};
use hmi_project_model::validate;
use serde_json::json;

#[test]
fn serialization_is_byte_identical_across_runs() {
    let p = parse_project(&fixture("valid/bottling-line.hmi.json"))
        .unwrap()
        .project;
    let first = to_json_string(&p).unwrap();
    for _ in 0..20 {
        assert_eq!(to_json_string(&p.clone()).unwrap(), first);
    }
}

#[test]
fn map_insertion_order_does_not_affect_output() {
    let mut a = project(base());
    let mut b = a.clone();
    let mut fwd = BTreeMap::new();
    let mut rev = BTreeMap::new();
    let keys = ["zeta", "alpha", "mu", "beta"];
    for k in keys {
        fwd.insert(k.to_owned(), json!(k));
    }
    for k in keys.iter().rev() {
        rev.insert((*k).to_owned(), json!(k));
    }
    a.extensions = fwd;
    b.extensions = rev;
    assert_eq!(to_json_string(&a).unwrap(), to_json_string(&b).unwrap());

    // Same for free-form JSON objects inside values: parse with keys in two
    // different textual orders.
    let x: serde_json::Value = serde_json::from_str(r#"{"b":1,"a":{"d":2,"c":3}}"#).unwrap();
    let y: serde_json::Value = serde_json::from_str(r#"{"a":{"c":3,"d":2},"b":1}"#).unwrap();
    a.extensions = BTreeMap::from([("k".to_owned(), x)]);
    b.extensions = BTreeMap::from([("k".to_owned(), y)]);
    assert_eq!(to_json_string(&a).unwrap(), to_json_string(&b).unwrap());
}

#[test]
fn json_key_order_in_input_does_not_affect_output() {
    let text = to_json_string(&project(base())).unwrap();
    // Re-emit the same document with object keys reversed everywhere.
    fn reverse(v: &serde_json::Value) -> String {
        match v {
            serde_json::Value::Object(m) => {
                let parts: Vec<String> = m
                    .iter()
                    .rev()
                    .map(|(k, v)| format!("{}:{}", serde_json::to_string(k).unwrap(), reverse(v)))
                    .collect();
                format!("{{{}}}", parts.join(","))
            }
            serde_json::Value::Array(a) => {
                format!("[{}]", a.iter().map(reverse).collect::<Vec<_>>().join(","))
            }
            other => other.to_string(),
        }
    }
    let shuffled = reverse(&serde_json::from_str(&text).unwrap());
    assert_ne!(shuffled, text);
    assert_eq!(
        to_json_string(&parse_project(&shuffled).unwrap().project).unwrap(),
        text
    );
}

#[test]
fn canonical_output_omits_defaults_and_ends_with_newline() {
    let text = to_json_string(&project(base())).unwrap();
    assert!(text.ends_with("}\n"));
    assert!(!text.contains("\"visibility\": \"visible\""));
    assert!(!text.contains("\"mode\": \"read\""));
    assert!(!text.contains("\"zIndex\": 0"));
    assert!(!text.contains("null"));
    assert!(text.starts_with("{\n  \"format\": \"hmi-studio.project\",\n  \"schemaVersion\": 2,"));
}

#[test]
fn validation_is_deterministic() {
    let broken = || {
        report_with(|v| {
            at(v, "/screens/0/root/children/0")["id"] = json!("tag-word");
            at(v, "/screens/0/root/children/2/bindings/0/source")["tag"] = json!("x");
            v["components"].as_array_mut().unwrap().push(json!({
                "id": "cmp-r", "name": "R",
                "root": { "id": "n-r", "element": { "type": "instance", "definition": "cmp-r" } }
            }));
            at(v, "/localization/strings")["k"] = json!({ "en": "x" });
        })
    };
    let first = broken();
    assert!(first.diagnostics.len() >= 4, "{first}");
    for _ in 0..20 {
        assert_eq!(broken(), first);
    }
    // And serializable for IPC / CLI output.
    let as_json = serde_json::to_value(&first).unwrap();
    assert_eq!(as_json["diagnostics"][0]["severity"], "error");
    assert!(as_json["diagnostics"][0]["code"]
        .as_str()
        .unwrap()
        .contains('-'));
}

#[test]
fn non_finite_numbers_are_refused_not_written_as_null() {
    let mut p = project(base());
    p.screens[0].root.layout.max_width = Some(f64::NAN);
    assert!(matches!(to_json_string(&p), Err(SaveError::NotLossless(_))));
    let mut p = project(base());
    if let Element::Column { spacing, .. } = &mut p.screens[0].root.element {
        *spacing = f64::INFINITY;
    }
    assert!(matches!(to_json_string(&p), Err(SaveError::NotLossless(_))));
    assert!(validate(&p).has_errors());
}

#[test]
fn save_refuses_invalid_projects() {
    let mut p = project(base());
    p.navigation.start_screen = ScreenId::new("nope").unwrap();
    match save_project(&p) {
        Err(SaveError::Invalid(r)) => assert_eq!(r.error_count(), 1),
        other => panic!("{other:?}"),
    }
    assert!(save_project(&project(base())).is_ok());
}
