#![allow(dead_code, clippy::unwrap_used)]

use std::path::PathBuf;

use hmi_project_model::serialization::parse_value_with;
use hmi_project_model::{validate, DiagnosticCode, MigrationRegistry, Project, ValidationReport};
use serde_json::{json, Value};

pub fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub fn fixture(rel: &str) -> String {
    std::fs::read_to_string(crate_dir().join("fixtures").join(rel)).unwrap()
}

pub fn fixture_files(dir: &str) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(crate_dir().join("fixtures").join(dir))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    files.sort();
    files
}

/// A small but representative valid project, as JSON, for tests to mutate.
pub fn base() -> Value {
    json!({
        "format": "hmi-studio.project",
        "schemaVersion": 1,
        "id": "prj-test",
        "metadata": { "name": "Test" },
        "navigation": { "startScreen": "scr-main" },
        "breakpoints": [ { "id": "bp-compact", "name": "Compact", "minWidth": 0, "maxWidth": 600 } ],
        "themes": [ { "id": "theme-a", "name": "A", "colors": { "ok": "#00FF00", "bad": "#FF0000" } } ],
        "resources": [ { "id": "res-img", "kind": "image", "path": "assets/a.png" } ],
        "localization": {
            "defaultLocale": "en",
            "locales": ["en", "it"],
            "strings": { "hello": { "en": "Hello", "it": "Ciao" } }
        },
        "dataTypes": [ {
            "id": "dt-motor",
            "name": "Motor",
            "members": [
                { "name": "running", "dataType": "bool" },
                { "name": "speed", "dataType": "float32" },
                { "name": "codes", "dataType": { "array": { "element": "uint16", "length": 2 } } }
            ]
        } ],
        "connections": [ { "id": "conn-sim", "name": "Sim", "driver": "simulator" } ],
        "tags": [
            { "id": "tag-bool-ro", "name": "BoolRO", "dataType": "bool", "connection": "conn-sim", "address": "a" },
            { "id": "tag-bool-rw", "name": "BoolRW", "dataType": "bool", "connection": "conn-sim", "address": "b", "access": "readWrite" },
            { "id": "tag-real-rw", "name": "RealRW", "dataType": "float32", "connection": "conn-sim", "address": "c", "access": "readWrite" },
            { "id": "tag-word", "name": "Word", "dataType": "uint16", "connection": "conn-sim", "address": "d" },
            { "id": "tag-text", "name": "Text", "dataType": "string" },
            { "id": "tag-motor", "name": "Motor", "dataType": { "struct": "dt-motor" }, "connection": "conn-sim", "address": "e", "access": "readWrite" }
        ],
        "alarms": [ { "id": "alm-1", "name": "A1", "tag": "tag-bool-ro", "condition": { "type": "isTrue" }, "message": "Alarm", "group": "g1" } ],
        "recipes": [ {
            "id": "rcp-1",
            "name": "R1",
            "fields": [ { "id": "fld-1", "name": "F1", "tag": "tag-real-rw" } ],
            "sets": [ { "id": "set-1", "name": "S1", "values": { "fld-1": 1.5 } } ]
        } ],
        "components": [ {
            "id": "cmp-lamp",
            "name": "Lamp",
            "parameters": [
                { "name": "source", "type": { "kind": "tag", "dataType": "bool" } },
                { "name": "caption", "type": { "kind": "text" }, "default": "Lamp" }
            ],
            "root": {
                "id": "n-lamp-root",
                "element": { "type": "row" },
                "children": [
                    { "id": "n-lamp-ind", "element": { "type": "component", "kind": "indicator" },
                      "bindings": [ { "property": "active", "source": { "tagParameter": "source" } } ] },
                    { "id": "n-lamp-text", "element": { "type": "component", "kind": "text" },
                      "bindings": [ { "property": "text", "source": { "parameter": "caption" } } ] }
                ]
            }
        } ],
        "screens": [
            {
                "id": "scr-main",
                "name": "Main",
                "root": {
                    "id": "n-root",
                    "element": { "type": "column", "spacing": 8 },
                    "children": [
                        { "id": "n-text", "element": { "type": "component", "kind": "text" },
                          "properties": { "text": { "textKey": "hello" } } },
                        { "id": "n-lamp", "element": { "type": "instance", "definition": "cmp-lamp",
                          "parameters": { "source": "tag-bool-ro" } } },
                        { "id": "n-switch", "element": { "type": "component", "kind": "switch" },
                          "bindings": [ { "property": "checked", "source": { "tag": "tag-bool-rw" }, "mode": "readWrite" } ] },
                        { "id": "n-button", "element": { "type": "component", "kind": "button" },
                          "events": [ { "event": "click", "actions": [ { "type": "openPopup", "screen": "pop-info" } ] } ] }
                    ]
                }
            },
            {
                "id": "pop-info",
                "name": "Info",
                "kind": "popup",
                "root": { "id": "n-pop-root", "element": { "type": "box" } }
            }
        ]
    })
}

/// Deserializes a JSON value (panics on structural errors).
pub fn project(value: Value) -> Project {
    parse_value_with(value, &MigrationRegistry::builtin())
        .unwrap_or_else(|e| panic!("structure error: {e}"))
        .project
}

pub fn report(value: Value) -> ValidationReport {
    validate(&project(value))
}

/// Mutates the base project and validates it.
pub fn report_with(f: impl FnOnce(&mut Value)) -> ValidationReport {
    let mut v = base();
    f(&mut v);
    report(v)
}

/// Asserts that `report` has an error with `code` at `path`.
#[track_caller]
pub fn assert_error(report: &ValidationReport, code: DiagnosticCode, path: &str) {
    assert!(
        report.errors().any(|d| d.code == code && d.path == path),
        "expected {code:?} at {path}, got:\n{report}"
    );
}

/// Asserts the report contains exactly the given (code, path) errors.
#[track_caller]
pub fn assert_only_errors(report: &ValidationReport, expected: &[(DiagnosticCode, &str)]) {
    let got: Vec<(DiagnosticCode, &str)> =
        report.errors().map(|d| (d.code, d.path.as_str())).collect();
    assert_eq!(got, expected, "full report:\n{report}");
}

/// JSON pointer access helper for mutation.
pub fn at<'a>(v: &'a mut Value, pointer: &str) -> &'a mut Value {
    v.pointer_mut(pointer)
        .unwrap_or_else(|| panic!("no value at {pointer}"))
}

/// Builds a column-nested chain of `depth` nodes.
pub fn nested_chain(depth: usize) -> Value {
    let mut node = json!({ "id": format!("n-{depth}"), "element": { "type": "component", "kind": "text" }, "properties": { "text": "leaf" } });
    for d in (1..depth).rev() {
        let element = match d % 3 {
            0 => json!({ "type": "row" }),
            1 => json!({ "type": "column" }),
            _ => json!({ "type": "box" }),
        };
        node = json!({ "id": format!("n-{d}"), "element": element, "children": [node] });
    }
    node
}
