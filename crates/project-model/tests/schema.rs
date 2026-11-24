//! JSON Schema tests.
//!
//! Includes a deliberately small JSON Schema evaluator covering the keywords
//! the generated schema uses, so fixtures can be checked against the
//! committed schema without adding a heavy dev-dependency. It is a test aid,
//! not a general-purpose validator (`pattern` is not evaluated).
#![allow(clippy::unwrap_used)]

mod common;

use common::*;
use hmi_project_model::schema::{project_json_schema_string, SCHEMA_FILE, SCHEMA_ID};
use serde_json::{json, Value};

fn committed_schema() -> Value {
    let text = std::fs::read_to_string(crate_dir().join(SCHEMA_FILE)).unwrap();
    serde_json::from_str(&text).unwrap()
}

#[test]
fn committed_schema_is_up_to_date() {
    let committed = std::fs::read_to_string(crate_dir().join(SCHEMA_FILE)).unwrap();
    assert!(
        committed == project_json_schema_string(),
        "{SCHEMA_FILE} is stale; run `cargo run --example export_schema`"
    );
}

#[test]
fn schema_identity_and_root_requirements() {
    let s = committed_schema();
    assert_eq!(s["$schema"], "https://json-schema.org/draft/2020-12/schema");
    assert_eq!(s["$id"], SCHEMA_ID);
    assert_eq!(s["additionalProperties"], false);
    let required: Vec<&str> = s["required"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    for r in [
        "format",
        "schemaVersion",
        "id",
        "metadata",
        "navigation",
        "localization",
        "screens",
    ] {
        assert!(required.contains(&r), "{r} not required");
    }
    assert_eq!(s["properties"]["schemaVersion"]["const"], 1);
}

#[test]
fn every_ref_resolves() {
    let s = committed_schema();
    let defs = s["$defs"].as_object().unwrap();
    fn walk(v: &Value, defs: &serde_json::Map<String, Value>, count: &mut usize) {
        match v {
            Value::Object(m) => {
                if let Some(Value::String(r)) = m.get("$ref") {
                    let name = r.strip_prefix("#/$defs/").unwrap();
                    assert!(defs.contains_key(name), "dangling {r}");
                    *count += 1;
                }
                m.values().for_each(|x| walk(x, defs, count));
            }
            Value::Array(a) => a.iter().for_each(|x| walk(x, defs, count)),
            _ => {}
        }
    }
    let mut count = 0;
    walk(&s, defs, &mut count);
    assert!(count > 50);
}

#[test]
fn fixtures_conform_to_the_committed_schema() {
    let schema = committed_schema();
    for path in fixture_files("valid") {
        let doc: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let mut errors = Vec::new();
        check(&schema, &doc, &schema, "", &mut errors);
        assert!(errors.is_empty(), "{}: {errors:#?}", path.display());
    }
    let mut errors = Vec::new();
    check(&schema, &base(), &schema, "", &mut errors);
    assert!(errors.is_empty(), "base: {errors:#?}");
}

#[test]
fn schema_rejects_structural_mistakes() {
    let schema = committed_schema();
    let mut bad = base();
    bad["screens"][0]["root"]["element"] = json!({ "type": "column", "gap": 4 });
    bad["tags"][0]["dataType"] = json!("float128");
    bad["metadata"] = json!({});
    let mut errors = Vec::new();
    check(&schema, &bad, &schema, "", &mut errors);
    assert!(
        errors
            .iter()
            .any(|e| e.starts_with("/screens/0/root/element")),
        "{errors:#?}"
    );
    assert!(
        errors.iter().any(|e| e.starts_with("/tags/0/dataType")),
        "{errors:#?}"
    );
    assert!(
        errors.iter().any(|e| e.starts_with("/metadata")),
        "{errors:#?}"
    );
}

// ------------------------------------------------------- mini evaluator

fn resolve<'a>(root: &'a Value, r: &str) -> &'a Value {
    let name = r.strip_prefix("#/$defs/").unwrap();
    &root["$defs"][name]
}

fn type_ok(t: &str, v: &Value) -> bool {
    match t {
        "null" => v.is_null(),
        "boolean" => v.is_boolean(),
        "object" => v.is_object(),
        "array" => v.is_array(),
        "string" => v.is_string(),
        "number" => v.is_number(),
        "integer" => v.is_i64() || v.is_u64() || v.as_f64().is_some_and(|f| f.fract() == 0.0),
        other => panic!("unsupported type {other}"),
    }
}

fn check(schema: &Value, v: &Value, root: &Value, path: &str, errors: &mut Vec<String>) {
    let Some(s) = schema.as_object() else {
        if schema == &Value::Bool(false) {
            errors.push(format!("{path}: schema false"));
        }
        return;
    };
    if let Some(Value::String(r)) = s.get("$ref") {
        check(resolve(root, r), v, root, path, errors);
    }
    match s.get("type") {
        Some(Value::String(t)) if !type_ok(t, v) => {
            errors.push(format!("{path}: expected {t}"));
            return;
        }
        Some(Value::Array(ts)) if !ts.iter().any(|t| type_ok(t.as_str().unwrap(), v)) => {
            errors.push(format!("{path}: expected one of {ts:?}"));
            return;
        }
        _ => {}
    }
    if let Some(c) = s.get("const") {
        if c != v {
            errors.push(format!("{path}: expected const {c}"));
        }
    }
    if let Some(Value::Array(e)) = s.get("enum") {
        if !e.contains(v) {
            errors.push(format!("{path}: {v} not in enum"));
        }
    }
    if let (Some(min), Some(n)) = (s.get("minimum").and_then(Value::as_f64), v.as_f64()) {
        if n < min {
            errors.push(format!("{path}: below minimum"));
        }
    }
    if let (Some(max), Some(n)) = (s.get("maximum").and_then(Value::as_f64), v.as_f64()) {
        if n > max {
            errors.push(format!("{path}: above maximum"));
        }
    }
    if let (Some(n), Some(t)) = (s.get("minLength").and_then(Value::as_u64), v.as_str()) {
        if (t.chars().count() as u64) < n {
            errors.push(format!("{path}: too short"));
        }
    }
    if let Some(Value::Array(all)) = s.get("allOf") {
        for sub in all {
            check(sub, v, root, path, errors);
        }
    }
    if let Some(Value::Array(any)) = s.get("anyOf") {
        if !any.iter().any(|sub| passes(sub, v, root)) {
            errors.push(format!("{path}: matches no anyOf branch"));
        }
    }
    if let Some(Value::Array(one)) = s.get("oneOf") {
        let n = one.iter().filter(|sub| passes(sub, v, root)).count();
        if n != 1 {
            errors.push(format!("{path}: matches {n} oneOf branches"));
        }
    }
    if let (Some(items), Some(arr)) = (s.get("items"), v.as_array()) {
        for (i, x) in arr.iter().enumerate() {
            check(items, x, root, &format!("{path}/{i}"), errors);
        }
    }
    if let Some(obj) = v.as_object() {
        let props = s.get("properties").and_then(Value::as_object);
        if let Some(Value::Array(req)) = s.get("required") {
            for r in req {
                if !obj.contains_key(r.as_str().unwrap()) {
                    errors.push(format!("{path}: missing required {r}"));
                }
            }
        }
        for (k, x) in obj {
            let p = format!("{path}/{k}");
            if let Some(sub) = props.and_then(|m| m.get(k)) {
                check(sub, x, root, &p, errors);
            } else if let Some(Value::Object(pp)) = s.get("patternProperties") {
                // Patterns are not evaluated; apply every pattern schema.
                for sub in pp.values() {
                    check(sub, x, root, &p, errors);
                }
            } else if let Some(ap) = s.get("additionalProperties") {
                check(ap, x, root, &p, errors);
            }
        }
    }
}

fn passes(schema: &Value, v: &Value, root: &Value) -> bool {
    let mut e = Vec::new();
    check(schema, v, root, "", &mut e);
    e.is_empty()
}
