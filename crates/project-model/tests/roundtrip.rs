//! JSON round-trip and lossless-load tests.
#![allow(clippy::unwrap_used)]

mod common;

use common::*;
use hmi_project_model::model::*;
use hmi_project_model::serialization::{parse_project, to_json_string, LoadError};
use hmi_project_model::validate;
use serde_json::json;

#[test]
fn base_project_is_valid() {
    let r = report(base());
    assert!(r.diagnostics.is_empty(), "{r}");
}

#[test]
fn fixtures_round_trip_byte_for_byte() {
    for path in fixture_files("valid") {
        let text = std::fs::read_to_string(&path).unwrap();
        let parsed = parse_project(&text).unwrap().project;
        let out = to_json_string(&parsed).unwrap();
        assert_eq!(out, text, "{} is not in canonical form", path.display());
        let again = parse_project(&out).unwrap().project;
        assert_eq!(again, parsed);
    }
}

#[test]
fn model_round_trip_through_json_is_identity() {
    let p = project(base());
    let text = to_json_string(&p).unwrap();
    let back = parse_project(&text).unwrap().project;
    assert_eq!(back, p);
    assert_eq!(to_json_string(&back).unwrap(), text);
}

#[test]
fn programmatic_project_round_trips() {
    let mut root = Node::new(
        NodeId::new("n-root").unwrap(),
        Element::Column {
            arrangement: Arrangement::SpaceBetween,
            alignment: CrossAlignment::Center,
            spacing: 4.5,
            wrap: false,
        },
    );
    let mut text = Node::new(
        NodeId::new("n-text").unwrap(),
        Element::Component {
            kind: ComponentKind::Text,
        },
    );
    text.properties.insert("text".into(), json!("Hi"));
    text.layout.width = Size::fill();
    text.layout.max_width = Some(320.0);
    root.children.push(text);
    let screen = Screen {
        id: ScreenId::new("scr-a").unwrap(),
        name: "A".into(),
        kind: ScreenKind::Page,
        title: Some(LocalizedText::literal("A")),
        design_profile: None,
        events: vec![],
        root,
        extensions: Extensions::new(),
    };
    let p = Project::new(ProjectId::new("prj-a").unwrap(), "A", screen);
    assert!(validate(&p).diagnostics.is_empty(), "{}", validate(&p));
    let text = to_json_string(&p).unwrap();
    assert_eq!(parse_project(&text).unwrap().project, p);
}

#[test]
fn extensions_and_free_form_values_are_preserved_exactly() {
    let mut v = base();
    v["extensions"] = json!({
        "vendor.tool": { "nested": [1, 2.5, "x", null, { "z": true, "a": false }] },
        "big": 18446744073709551615u64,
        "neg": -9223372036854775808i64,
        "precise": 1.2345678901234567
    });
    at(&mut v, "/screens/0/root")["extensions"] = json!({ "editor.locked": true });
    let p = project(v.clone());
    let text = to_json_string(&p).unwrap();
    let reparsed: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(reparsed["extensions"], v["extensions"]);
    assert_eq!(
        reparsed["screens"][0]["root"]["extensions"]["editor.locked"],
        true
    );
}

#[test]
fn unknown_fields_are_rejected_not_dropped() {
    for (pointer, field) in [
        ("", "unexpected"),
        ("/metadata", "colour"),
        ("/screens/0/root", "widht"),
        ("/screens/0/root/element", "gap"),
        ("/tags/0", "plcAddress"),
        ("/screens/1/root/element", "extra"),
    ] {
        let mut v = base();
        at(&mut v, if pointer.is_empty() { "" } else { pointer })
            .as_object_mut()
            .unwrap()
            .insert(field.into(), json!(1));
        let err = parse_project(&v.to_string()).unwrap_err();
        match err {
            LoadError::Structure { path, message } => {
                assert!(message.contains(field), "{message}");
                assert!(path.starts_with(pointer), "{path} vs {pointer}");
            }
            other => panic!("expected structure error, got {other}"),
        }
    }
}

#[test]
fn unknown_fields_on_empty_variants_are_rejected() {
    // serde silently ignores extra keys on *unit* variants of internally
    // tagged enums; the model uses empty struct variants to prevent that.
    let mut v = base();
    at(&mut v, "/screens/0/root/children/3/events/0/actions/0")
        .clone_from(&json!({ "type": "back", "screen": "x" }));
    assert!(matches!(
        parse_project(&v.to_string()),
        Err(LoadError::Structure { .. })
    ));

    let mut v = base();
    at(&mut v, "/screens/0/root/children/0")
        .as_object_mut()
        .unwrap()
        .insert(
            "layout".into(),
            json!({ "width": { "mode": "auto", "value": 3 } }),
        );
    assert!(matches!(
        parse_project(&v.to_string()),
        Err(LoadError::Structure { .. })
    ));
}

#[test]
fn structure_errors_report_json_pointer() {
    let mut v = base();
    at(&mut v, "/screens/0/root/children/1/element")["type"] = json!("carousel");
    match parse_project(&v.to_string()).unwrap_err() {
        LoadError::Structure { path, .. } => {
            assert_eq!(path, "/screens/0/root/children/1/element/type")
        }
        other => panic!("{other}"),
    }
}

#[test]
fn syntax_errors_report_position() {
    match parse_project("{\n  \"format\": \"hmi-studio.project\",\n  oops\n}").unwrap_err() {
        LoadError::Syntax { line, column, .. } => {
            assert_eq!(line, 3);
            assert!(column > 0);
        }
        other => panic!("{other}"),
    }
}

#[test]
fn floats_round_trip_exactly() {
    let mut v = base();
    at(&mut v, "/screens/0/root/element")["spacing"] = json!(0.1 + 0.2);
    at(&mut v, "/recipes/0/sets/0/values")["fld-1"] = json!(1.0e-300);
    let p = project(v);
    let back = parse_project(&to_json_string(&p).unwrap()).unwrap().project;
    let Element::Column { spacing, .. } = back.screens[0].root.element else {
        panic!()
    };
    assert_eq!(spacing.to_bits(), (0.1f64 + 0.2).to_bits());
    assert_eq!(back, p);
}
