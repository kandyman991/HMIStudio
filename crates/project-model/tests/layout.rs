//! Deeply nested layouts, responsive representation and layout constraints.
#![allow(clippy::unwrap_used)]

mod common;

use common::*;
use hmi_project_model::model::*;
use hmi_project_model::serialization::{parse_project, to_json_string, LoadError};
use hmi_project_model::{DiagnosticCode as C, MAX_NODE_DEPTH};
use serde_json::json;

fn with_root(root: serde_json::Value) -> serde_json::Value {
    let mut v = base();
    at(&mut v, "/screens/0")["root"] = root;
    v
}

#[test]
fn nesting_up_to_the_limit_is_valid_and_round_trips() {
    let v = with_root(nested_chain(MAX_NODE_DEPTH));
    let p = project(v);
    let r = hmi_project_model::validate(&p);
    assert!(!r.has_errors(), "{r}");
    let mut depth = 0;
    let mut node = &p.screens[0].root;
    loop {
        depth += 1;
        match node.children.first() {
            Some(c) => node = c,
            None => break,
        }
    }
    assert_eq!(depth, MAX_NODE_DEPTH);
    let text = to_json_string(&p).unwrap();
    assert_eq!(parse_project(&text).unwrap().project, p);
}

#[test]
fn nesting_beyond_the_limit_is_reported_once() {
    let r = report(with_root(nested_chain(MAX_NODE_DEPTH + 3)));
    let d = r.with_code(C::NestingTooDeep);
    assert_eq!(d.len(), 1, "{r}");
    let expected_path = format!("/screens/0/root{}", "/children/0".repeat(MAX_NODE_DEPTH));
    assert_eq!(d[0].path, expected_path);
}

#[test]
fn pathological_depth_is_a_clean_error_not_a_crash() {
    let v = with_root(nested_chain(500));
    match parse_project(&v.to_string()) {
        Err(LoadError::Syntax { message, .. }) => {
            assert!(message.contains("recursion"), "{message}")
        }
        other => panic!("expected recursion limit error, got {other:?}"),
    }
}

#[test]
fn every_container_kind_nests_inside_every_other() {
    let containers = [
        json!({ "type": "row", "spacing": 4 }),
        json!({ "type": "column", "arrangement": "spaceEvenly" }),
        json!({ "type": "box", "contentAlignment": "center" }),
        json!({ "type": "grid", "columns": [ { "mode": "auto" } ], "rows": [ { "mode": "auto" } ] }),
        json!({ "type": "scroll", "direction": "horizontal" }),
        json!({ "type": "absolute" }),
    ];
    let mut n = 0;
    for outer in &containers {
        for inner in &containers {
            n += 1;
            let mut child_layout = json!({});
            match outer["type"].as_str().unwrap() {
                "grid" => child_layout["gridCell"] = json!({ "column": 0, "row": 0 }),
                "absolute" => child_layout["position"] = json!({ "x": 1, "y": 2 }),
                _ => {}
            }
            let mut leaf_layout = json!({});
            match inner["type"].as_str().unwrap() {
                "grid" => leaf_layout["gridCell"] = json!({ "column": 0, "row": 0 }),
                "absolute" => leaf_layout["position"] = json!({ "x": 0, "y": 0 }),
                _ => {}
            }
            let root = json!({
                "id": "n-outer", "element": outer,
                "layout": { "width": { "mode": "fill" } },
                "children": [ {
                    "id": "n-inner", "element": inner, "layout": child_layout,
                    "children": [ { "id": "n-leaf", "element": { "type": "component", "kind": "rectangle" }, "layout": leaf_layout } ]
                } ]
            });
            let r = report(with_root(root));
            assert!(
                !r.has_errors(),
                "{} in {}:\n{r}",
                inner["type"],
                outer["type"]
            );
        }
    }
    assert_eq!(n, 36);
}

#[test]
fn responsive_rules_are_represented_and_round_trip() {
    let root = json!({
        "id": "n-root",
        "element": { "type": "row", "spacing": 16 },
        "layout": { "width": { "mode": "fill" } },
        "responsive": [
            { "when": { "breakpoint": "bp-compact" }, "set": { "direction": "vertical", "spacing": 8 } },
            { "when": { "minWidth": 1200, "orientation": "landscape" }, "set": { "padding": { "start": 32, "end": 32 } } },
            { "when": { "platform": "android", "maxWidth": 400 }, "set": { "visibility": "collapsed" } }
        ],
        "children": [
            { "id": "n-a", "element": { "type": "component", "kind": "rectangle" }, "layout": { "weight": 2 },
              "responsive": [ { "when": { "breakpoint": "bp-compact" }, "set": { "weight": 1, "height": { "mode": "fixed", "value": 80 } } } ] },
            { "id": "n-b", "element": { "type": "component", "kind": "rectangle" }, "layout": { "weight": 1, "minWidth": 120, "maxWidth": 480 } }
        ]
    });
    let p = project(with_root(root));
    let r = hmi_project_model::validate(&p);
    assert!(r.diagnostics.is_empty(), "{r}");
    let rules = &p.screens[0].root.responsive;
    assert_eq!(rules.len(), 3);
    assert_eq!(rules[0].set.direction, Some(Axis::Vertical));
    assert_eq!(
        rules[0].when.breakpoint.as_ref().map(|b| b.as_str()),
        Some("bp-compact")
    );
    assert_eq!(rules[1].when.orientation, Some(Orientation::Landscape));
    assert_eq!(rules[2].set.visibility, Some(Visibility::Collapsed));
    assert_eq!(
        p.screens[0].root.children[0].responsive[0].set.weight,
        Some(1.0)
    );
    let text = to_json_string(&p).unwrap();
    assert_eq!(parse_project(&text).unwrap().project, p);
}

#[test]
fn invalid_responsive_rules() {
    let root = json!({
        "id": "n-root",
        "element": { "type": "box" },
        "responsive": [
            { "when": {}, "set": { "visibility": "hidden" } },
            { "when": { "breakpoint": "bp-nope" }, "set": {} },
            { "when": { "minWidth": 800, "maxWidth": 400 }, "set": { "direction": "vertical" } },
            { "when": { "maxWidth": 400 }, "set": { "columns": [ { "mode": "auto" } ], "weight": 1 } }
        ]
    });
    let r = report(with_root(root));
    assert_only_errors(
        &r,
        &[
            (
                C::InvalidResponsiveRule,
                "/screens/0/root/responsive/0/when",
            ),
            (
                C::UnknownReference,
                "/screens/0/root/responsive/1/when/breakpoint",
            ),
            (C::InvalidResponsiveRule, "/screens/0/root/responsive/1/set"),
            (
                C::InvalidResponsiveRule,
                "/screens/0/root/responsive/2/when",
            ),
            (
                C::InvalidResponsiveRule,
                "/screens/0/root/responsive/2/set/direction",
            ),
            (
                C::InvalidResponsiveRule,
                "/screens/0/root/responsive/3/set/columns",
            ),
            (
                C::InvalidResponsiveRule,
                "/screens/0/root/responsive/3/set/weight",
            ),
        ],
    );
}

#[test]
fn layout_constraints_are_checked_in_context() {
    let root = json!({
        "id": "n-root",
        "element": { "type": "column" },
        "layout": { "weight": 1 },
        "children": [
            { "id": "n-1", "element": { "type": "component", "kind": "rectangle" },
              "layout": { "width": { "mode": "fixed", "value": -5 }, "minWidth": 50, "maxWidth": 10 } },
            { "id": "n-2", "element": { "type": "component", "kind": "rectangle" },
              "layout": { "width": { "mode": "fill", "fraction": 1.5 }, "align": "center", "position": { "x": 1, "y": 1 } } },
            { "id": "n-3", "element": { "type": "component", "kind": "rectangle" },
              "layout": { "height": { "mode": "fixed", "value": 400 }, "maxHeight": 300, "weight": 0, "aspectRatio": 0 } },
            { "id": "n-4", "element": { "type": "grid", "columns": [], "rows": [ { "mode": "fraction", "weight": 0 } ] },
              "children": [
                { "id": "n-4a", "element": { "type": "component", "kind": "rectangle" } },
                { "id": "n-4b", "element": { "type": "component", "kind": "rectangle" }, "layout": { "gridCell": { "column": 0, "row": 0, "rowSpan": 3 } } }
              ] },
            { "id": "n-5", "element": { "type": "absolute" },
              "children": [ { "id": "n-5a", "element": { "type": "component", "kind": "rectangle" }, "layout": { "gridCell": { "column": 0, "row": 0 } } } ] },
            { "id": "n-6", "element": { "type": "scroll" },
              "children": [ { "id": "n-6a", "element": { "type": "column" }, "layout": { "height": { "mode": "fill" }, "width": { "mode": "fill" } } } ] },
            { "id": "n-7", "element": { "type": "row", "spacing": -1 }, "layout": { "padding": { "top": -2 } } }
        ]
    });
    let r = report(with_root(root));
    let p = "/screens/0/root";
    assert_only_errors(
        &r,
        &[
            (C::InvalidLayoutConstraint, &format!("{p}/layout/weight")),
            (
                C::InvalidLayoutConstraint,
                &format!("{p}/children/0/layout/width"),
            ),
            (
                C::InvalidLayoutConstraint,
                &format!("{p}/children/0/layout/minWidth"),
            ),
            (
                C::InvalidLayoutConstraint,
                &format!("{p}/children/0/layout/width"),
            ),
            (
                C::InvalidLayoutConstraint,
                &format!("{p}/children/1/layout/width"),
            ),
            (
                C::InvalidLayoutConstraint,
                &format!("{p}/children/1/layout/align"),
            ),
            (
                C::InvalidLayoutConstraint,
                &format!("{p}/children/1/layout/position"),
            ),
            (
                C::InvalidLayoutConstraint,
                &format!("{p}/children/2/layout/height"),
            ),
            (
                C::InvalidLayoutConstraint,
                &format!("{p}/children/2/layout/aspectRatio"),
            ),
            (
                C::InvalidLayoutConstraint,
                &format!("{p}/children/2/layout/weight"),
            ),
            (
                C::InvalidLayoutConstraint,
                &format!("{p}/children/3/element/columns"),
            ),
            (
                C::InvalidLayoutConstraint,
                &format!("{p}/children/3/element/rows/0"),
            ),
            (
                C::InvalidLayoutConstraint,
                &format!("{p}/children/3/children/0/layout"),
            ),
            (
                C::InvalidLayoutConstraint,
                &format!("{p}/children/3/children/1/layout/gridCell"),
            ),
            (
                C::InvalidLayoutConstraint,
                &format!("{p}/children/4/children/0/layout"),
            ),
            (
                C::InvalidLayoutConstraint,
                &format!("{p}/children/4/children/0/layout/gridCell"),
            ),
            (
                C::InvalidLayoutConstraint,
                &format!("{p}/children/5/children/0/layout/height"),
            ),
            (
                C::InvalidLayoutConstraint,
                &format!("{p}/children/6/element/spacing"),
            ),
            (
                C::InvalidLayoutConstraint,
                &format!("{p}/children/6/layout/padding"),
            ),
        ],
    );
}

#[test]
fn z_order_visibility_and_clipping_are_preserved() {
    let root = json!({
        "id": "n-root", "element": { "type": "box" },
        "children": [
            { "id": "n-top", "element": { "type": "component", "kind": "rectangle" }, "layout": { "zIndex": 10, "visibility": "hidden", "clip": true } },
            { "id": "n-bottom", "element": { "type": "component", "kind": "rectangle" }, "layout": { "zIndex": -1 } }
        ]
    });
    let p = project(with_root(root));
    let c = &p.screens[0].root.children;
    assert_eq!(c[0].layout.z_index, 10);
    assert_eq!(c[0].layout.visibility, Visibility::Hidden);
    assert!(c[0].layout.clip);
    assert_eq!(c[1].layout.z_index, -1);
    // Child order (paint order for equal zIndex) is preserved.
    let text = to_json_string(&p).unwrap();
    assert!(text.find("n-top").unwrap() < text.find("n-bottom").unwrap());
}
