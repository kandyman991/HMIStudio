//! Stable identifier tests.
#![allow(clippy::unwrap_used)]

mod common;

use common::*;
use hmi_project_model::id::{validate_id, IdAllocator, IdError};
use hmi_project_model::model::*;
use hmi_project_model::serialization::{parse_project, to_json_string};
use hmi_project_model::DiagnosticCode as C;
use serde_json::json;

#[test]
fn identifier_syntax() {
    for ok in [
        "a",
        "A1",
        "scr-main",
        "tag_motor_1",
        "0e4b1c2a-9f1d-4c4e-8a1b-1f2e3d4c5b6a",
    ] {
        assert_eq!(validate_id(ok), Ok(()), "{ok}");
    }
    assert_eq!(validate_id(""), Err(IdError::Empty));
    assert_eq!(validate_id("-x"), Err(IdError::InvalidStart));
    assert_eq!(validate_id("_x"), Err(IdError::InvalidStart));
    assert_eq!(validate_id("a b"), Err(IdError::InvalidCharacter(' ')));
    assert_eq!(validate_id("a.b"), Err(IdError::InvalidCharacter('.')));
    assert_eq!(validate_id("é"), Err(IdError::InvalidStart));
    assert_eq!(validate_id(&"a".repeat(64)), Ok(()));
    assert_eq!(validate_id(&"a".repeat(65)), Err(IdError::TooLong));
    assert!(ScreenId::new("bad id").is_err());
}

#[test]
fn allocator_is_deterministic_and_avoids_existing() {
    let mut a = IdAllocator::new(["btn-1", "btn-3"]);
    assert_eq!(a.allocate("btn").unwrap(), "btn-2");
    assert_eq!(a.allocate("btn").unwrap(), "btn-4");
    assert_eq!(a.allocate("txt").unwrap(), "txt-1");
    let mut b = IdAllocator::new(["btn-1", "btn-3"]);
    assert_eq!(b.allocate("btn").unwrap(), "btn-2");
    assert!(a.allocate("bad prefix").is_err());
    assert!(a.allocate(&"p".repeat(41)).is_err());
    for _ in 0..50 {
        let id = a.allocate("n").unwrap();
        assert_eq!(validate_id(&id), Ok(()));
    }
}

#[test]
fn ids_are_unaffected_by_renames_and_edits() {
    let mut p = project(base());
    let before: Vec<String> = all_node_ids(&p);
    p.screens[0].name = "Renamed".into();
    p.screens[0].root.name = Some("Layer renamed".into());
    p.screens[0].root.children.reverse();
    p.screens[0].root.children[0].layout.width = Size::fixed(10.0);
    let text = to_json_string(&p).unwrap();
    let back = parse_project(&text).unwrap().project;
    let mut after = all_node_ids(&back);
    let mut before_sorted = before.clone();
    before_sorted.sort();
    after.sort();
    assert_eq!(before_sorted, after);
    assert_eq!(back.screens[0].id.as_str(), "scr-main");
    assert!(text.contains("\"id\": \"n-text\""));
}

fn all_node_ids(p: &Project) -> Vec<String> {
    let mut out = Vec::new();
    for s in &p.screens {
        s.root.walk(&mut |n| out.push(n.id.to_string()));
    }
    out
}

#[test]
fn invalid_identifiers_are_reported_with_path() {
    let r = report_with(|v| {
        at(v, "/screens/0/root/children/0")["id"] = json!("has space");
        at(v, "/tags/0")["id"] = json!("");
    });
    assert_error(&r, C::InvalidIdentifier, "/tags/0/id");
    assert_error(&r, C::InvalidIdentifier, "/screens/0/root/children/0/id");
}

#[test]
fn duplicates_are_detected_across_kinds() {
    let r = report_with(|v| {
        // Same id for a tag and a node: one global namespace.
        at(v, "/screens/0/root/children/0")["id"] = json!("tag-word");
        // Duplicate node id inside a component definition vs a screen.
        at(v, "/screens/0/root/children/2")["id"] = json!("n-lamp-ind");
    });
    assert_only_errors(
        &r,
        &[
            (C::DuplicateIdentifier, "/screens/0/root/children/0/id"),
            (C::DuplicateIdentifier, "/screens/0/root/children/2/id"),
        ],
    );
}

#[test]
fn duplicate_report_points_at_later_occurrence() {
    let r = report_with(|v| {
        at(v, "/tags/1")["id"] = json!("tag-bool-ro");
    });
    let d = r.with_code(C::DuplicateIdentifier);
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].path, "/tags/1/id");
    assert!(d[0].message.contains("/tags/0/id"));
}
