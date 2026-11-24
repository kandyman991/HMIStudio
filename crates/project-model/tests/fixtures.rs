//! Example project fixtures.
//!
//! * `fixtures/valid/*.hmi.json` must load, validate with zero diagnostics,
//!   and be in canonical form.
//! * `fixtures/invalid/<diagnostic-code>.json` must parse and produce at
//!   least one error with that code.
//! * `fixtures/invalid/load-error.<kind>.json` must fail to load.
#![allow(clippy::unwrap_used)]

mod common;

use common::*;
use hmi_project_model::model::*;
use hmi_project_model::serialization::{load_project, parse_project, LoadError};
use hmi_project_model::validate;

#[test]
fn valid_fixtures_load_and_validate_cleanly() {
    let files = fixture_files("valid");
    assert!(files.len() >= 2);
    for path in files {
        let text = std::fs::read_to_string(&path).unwrap();
        let loaded = load_project(&text).unwrap_or_else(|e| {
            panic!(
                "{}: {e}\n{}",
                path.display(),
                match &e {
                    LoadError::Invalid(r) => r.to_string(),
                    _ => String::new(),
                }
            )
        });
        assert!(
            loaded.report.diagnostics.is_empty(),
            "{}:\n{}",
            path.display(),
            loaded.report
        );
        assert!(loaded.migrations.is_empty());
    }
}

#[test]
fn invalid_fixtures_fail_with_the_expected_code() {
    let files = fixture_files("invalid");
    assert!(files.len() >= 8);
    for path in files {
        let stem = path.file_stem().unwrap().to_str().unwrap().to_owned();
        let text = std::fs::read_to_string(&path).unwrap();
        if let Some(kind) = stem.strip_prefix("load-error.") {
            let err = parse_project(&text).unwrap_err();
            let ok = match kind {
                "unsupported-version" => matches!(err, LoadError::UnsupportedVersion { .. }),
                "unknown-field" => matches!(err, LoadError::Structure { .. }),
                other => panic!("unknown load-error fixture kind {other}"),
            };
            assert!(ok, "{stem}: {err}");
            continue;
        }
        let report = validate(&parse_project(&text).unwrap().project);
        let codes: Vec<String> = report
            .errors()
            .map(|d| {
                serde_json::to_value(d.code)
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_owned()
            })
            .collect();
        assert!(codes.contains(&stem), "{stem}: got {codes:?}\n{report}");
    }
}

/// The industrial example must actually exercise the model breadth that
/// RUST-03A requires, so it stays a meaningful integration fixture.
#[test]
fn industrial_example_covers_required_features() {
    let p = parse_project(&fixture("valid/bottling-line.hmi.json"))
        .unwrap()
        .project;
    assert!(
        p.screens
            .iter()
            .filter(|s| s.kind == ScreenKind::Page)
            .count()
            >= 4
    );
    assert!(p.screens.iter().any(|s| s.kind == ScreenKind::Popup));
    assert!(p.components.len() >= 3);
    assert!(p.tags.len() >= 10);
    assert!(p.alarms.len() >= 4);
    assert!(!p.recipes.is_empty() && p.recipes[0].sets.len() >= 2);
    assert!(p.connections.iter().any(|c| c.driver == DriverKind::OpcUa));
    assert!(p
        .connections
        .iter()
        .any(|c| c.driver == DriverKind::Simulator));
    assert!(p.device_profiles.len() >= 3);
    assert!(p.localization.locales.len() >= 2);

    let mut kinds = std::collections::BTreeSet::new();
    let mut elements = std::collections::BTreeSet::new();
    let mut instances = 0;
    let mut bindings = 0;
    let mut responsive = 0;
    let mut max_depth = 0;
    fn visit(n: &Node, depth: usize, max_depth: &mut usize, f: &mut dyn FnMut(&Node)) {
        *max_depth = (*max_depth).max(depth);
        f(n);
        for c in &n.children {
            visit(c, depth + 1, max_depth, f);
        }
    }
    let roots = p
        .screens
        .iter()
        .map(|s| &s.root)
        .chain(p.components.iter().map(|c| &c.root));
    for root in roots {
        visit(root, 1, &mut max_depth, &mut |n| {
            elements.insert(n.element.type_name());
            if let Element::Component { kind } = n.element {
                kinds.insert(kind);
            }
            if matches!(n.element, Element::Instance { .. }) {
                instances += 1;
            }
            bindings += n.bindings.len();
            responsive += n.responsive.len();
        });
    }
    for e in [
        "row",
        "column",
        "box",
        "grid",
        "scroll",
        "absolute",
        "component",
        "instance",
    ] {
        assert!(elements.contains(e), "missing element {e}");
    }
    for k in [
        ComponentKind::Button,
        ComponentKind::Text,
        ComponentKind::Image,
        ComponentKind::Rectangle,
        ComponentKind::Ellipse,
        ComponentKind::NumericDisplay,
        ComponentKind::NumericInput,
        ComponentKind::Indicator,
        ComponentKind::Gauge,
        ComponentKind::Slider,
        ComponentKind::Switch,
        ComponentKind::ProgressBar,
        ComponentKind::Container,
    ] {
        assert!(kinds.contains(&k), "missing component kind {k:?}");
    }
    assert!(instances >= 6);
    assert!(bindings >= 20);
    assert!(responsive >= 4);
    assert!(max_depth >= 6, "max depth {max_depth}");
}
