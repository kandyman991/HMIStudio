//! Binding, event and action validation; industrial contracts.
#![allow(clippy::unwrap_used)]

mod common;

use common::*;
use hmi_project_model::DiagnosticCode as C;
use serde_json::{json, Value};

const SW: &str = "/screens/0/root/children/2";

fn with_switch_bindings(bindings: Value) -> hmi_project_model::ValidationReport {
    report_with(|v| at(v, SW)["bindings"] = bindings)
}

fn with_node(node: Value) -> hmi_project_model::ValidationReport {
    report_with(|v| at(v, SW).clone_from(&node))
}

#[test]
fn valid_bindings_pass() {
    let r = with_node(json!({
        "id": "n-x", "element": { "type": "component", "kind": "indicator" },
        "bindings": [
            { "property": "active", "source": { "tag": "tag-word" }, "transform": { "type": "bit", "index": 15 } },
            { "property": "blink", "source": { "tag": "tag-motor" }, "member": "running" },
            { "property": "label", "source": { "tag": "tag-text" } },
            { "property": "activeColor", "source": { "tag": "tag-word" },
              "transform": { "type": "map", "entries": [ { "when": 1, "value": "#00FF00" }, { "when": 2, "value": { "token": "bad" } } ], "default": { "token": "ok" } } },
            { "property": "visible", "source": { "tag": "tag-bool-rw" }, "transform": { "type": "invert" } }
        ]
    }));
    assert!(r.diagnostics.is_empty(), "{r}");
}

#[test]
fn write_bindings_need_writable_tags_and_input_properties() {
    let r = with_node(json!({
        "id": "n-x", "element": { "type": "component", "kind": "numericInput" },
        "bindings": [
            { "property": "value", "source": { "tag": "tag-word" }, "mode": "readWrite" },
            { "property": "enabled", "source": { "tag": "tag-bool-rw" }, "mode": "write" }
        ]
    }));
    assert_only_errors(
        &r,
        &[
            (C::InvalidBinding, &format!("{SW}/bindings/0/mode")),
            (C::InvalidBinding, &format!("{SW}/bindings/1/mode")),
        ],
    );
}

#[test]
fn type_mismatches_need_transforms() {
    let r = with_node(json!({
        "id": "n-x", "element": { "type": "component", "kind": "numericDisplay" },
        "bindings": [
            { "property": "value", "source": { "tag": "tag-text" } },
            { "property": "visible", "source": { "tag": "tag-word" } },
            { "property": "textColor", "source": { "tag": "tag-bool-ro" } },
            { "property": "decimals", "source": { "tag": "tag-word" } }
        ]
    }));
    assert_only_errors(
        &r,
        &[
            (C::InvalidBinding, &format!("{SW}/bindings/0/source")),
            (C::InvalidBinding, &format!("{SW}/bindings/1/source")),
            (C::InvalidBinding, &format!("{SW}/bindings/2/source")),
            (C::InvalidBinding, &format!("{SW}/bindings/3/property")),
        ],
    );
}

#[test]
fn transforms_are_checked() {
    let r = with_node(json!({
        "id": "n-x", "element": { "type": "component", "kind": "slider" },
        "bindings": [
            { "property": "value", "source": { "tag": "tag-real-rw" }, "mode": "readWrite", "transform": { "type": "linear", "factor": 0 } },
            { "property": "visible", "source": { "tag": "tag-word" }, "transform": { "type": "bit", "index": 16 } },
            { "property": "enabled", "source": { "tag": "tag-real-rw" }, "transform": { "type": "bit", "index": 0 } },
            { "property": "background", "source": { "tag": "tag-bool-ro" }, "transform": { "type": "map", "entries": [ { "when": 3, "value": "blue" } ] } },
            { "property": "opacity", "source": { "tag": "tag-bool-ro" }, "transform": { "type": "invert" } }
        ]
    }));
    let b = format!("{SW}/bindings");
    assert_only_errors(
        &r,
        &[
            (C::InvalidBinding, &format!("{b}/0/transform")),
            (C::InvalidBinding, &format!("{b}/1/transform/index")),
            (C::InvalidBinding, &format!("{b}/2/transform")),
            (
                C::InvalidBinding,
                &format!("{b}/3/transform/entries/0/when"),
            ),
            (
                C::InvalidPropertyValue,
                &format!("{b}/3/transform/entries/0/value"),
            ),
            (C::InvalidBinding, &format!("{b}/4/transform")),
        ],
    );
}

#[test]
fn member_paths_are_resolved() {
    let r = with_switch_bindings(json!([
        { "property": "checked", "source": { "tag": "tag-motor" }, "member": "running", "mode": "readWrite" },
        { "property": "label", "source": { "tag": "tag-motor" }, "member": "nope" },
        { "property": "enabled", "source": { "tag": "tag-motor" }, "member": "codes[2]" },
        { "property": "visible", "source": { "tag": "tag-bool-ro" }, "member": "x" },
        { "property": "opacity", "source": { "tag": "tag-motor" }, "member": "codes[1]" }
    ]));
    let b = format!("{SW}/bindings");
    assert_only_errors(
        &r,
        &[
            (C::InvalidDataType, &format!("{b}/1/member")),
            (C::InvalidDataType, &format!("{b}/2/member")),
            (C::InvalidDataType, &format!("{b}/3/member")),
        ],
    );
}

#[test]
fn duplicate_and_unknown_binding_targets() {
    let r = with_switch_bindings(json!([
        { "property": "checked", "source": { "tag": "tag-bool-ro" } },
        { "property": "checked", "source": { "tag": "tag-bool-rw" } },
        { "property": "speed", "source": { "tag": "tag-real-rw" } }
    ]));
    assert_only_errors(
        &r,
        &[
            (C::DuplicateBinding, &format!("{SW}/bindings/1/property")),
            (C::UnknownProperty, &format!("{SW}/bindings/2/property")),
        ],
    );
}

#[test]
fn parameter_sources_only_inside_definitions() {
    let r = with_switch_bindings(json!([
        { "property": "checked", "source": { "tagParameter": "source" } },
        { "property": "label", "source": { "parameter": "caption" } }
    ]));
    assert_only_errors(
        &r,
        &[
            (C::InvalidBinding, &format!("{SW}/bindings/0/source")),
            (C::InvalidBinding, &format!("{SW}/bindings/1/source")),
        ],
    );
}

#[test]
fn parameter_sources_inside_definitions_are_typed() {
    let r = report_with(|v| {
        at(v, "/components/0/root/children/1")["bindings"] = json!([
            { "property": "text", "source": { "parameter": "source" } }
        ]);
        at(v, "/components/0/root/children/0")["bindings"] = json!([
            { "property": "active", "source": { "parameter": "caption" } },
            { "property": "label", "source": { "tagParameter": "source" } },
            { "property": "blink", "source": { "tagParameter": "missing" } }
        ]);
    });
    let n = "/components/0/root/children";
    assert_only_errors(
        &r,
        &[
            (C::InvalidBinding, &format!("{n}/0/bindings/0/source")),
            (C::InvalidBinding, &format!("{n}/0/bindings/1/source")),
            (C::InvalidBinding, &format!("{n}/0/bindings/2/source")),
            (C::InvalidBinding, &format!("{n}/1/bindings/0/source")),
        ],
    );
}

#[test]
fn instance_value_parameters_can_be_bound_to_tags() {
    let r = report_with(|v| {
        at(v, "/localization/strings")["x"] = json!({ "en": "x", "it": "x" });
        at(v, "/screens/0/root/children/1")["bindings"] = json!([
            { "property": "caption", "source": { "tag": "tag-text" } },
            { "property": "visible", "source": { "tag": "tag-bool-ro" } }
        ]);
    });
    assert!(r.diagnostics.is_empty(), "{r}");
    let r = report_with(|v| {
        at(v, "/screens/0/root/children/1")["bindings"] = json!([
            { "property": "source", "source": { "tag": "tag-bool-ro" } }
        ]);
    });
    assert_only_errors(
        &r,
        &[(
            C::UnknownProperty,
            "/screens/0/root/children/1/bindings/0/property",
        )],
    );
}

#[test]
fn events_and_actions_are_checked() {
    let r = with_node(json!({
        "id": "n-x", "element": { "type": "component", "kind": "button" },
        "events": [
            { "event": "valueChanged", "actions": [ { "type": "back" } ] },
            { "event": "click", "actions": [
                { "type": "writeTag", "target": { "tag": "tag-bool-ro" }, "value": true },
                { "type": "writeTag", "target": { "tag": "tag-real-rw" }, "value": "fast" },
                { "type": "toggleTag", "target": { "tag": "tag-real-rw" } },
                { "type": "stepTag", "target": { "tag": "tag-bool-rw" }, "step": 0 },
                { "type": "pulseTag", "target": { "tag": "tag-bool-rw" }, "durationMs": 0 },
                { "type": "acknowledgeAlarms", "alarm": "alm-1", "group": "g1" },
                { "type": "acknowledgeAlarms", "group": "nope" },
                { "type": "loadRecipe", "recipe": "rcp-1", "set": "set-x" },
                { "type": "setLocale", "locale": "de" },
                { "type": "writeTag", "target": { "tagParameter": "source" }, "value": true }
            ] },
            { "event": "click", "actions": [] }
        ]
    }));
    let a = format!("{SW}/events/1/actions");
    assert_only_errors(
        &r,
        &[
            (C::InvalidEvent, &format!("{SW}/events/0/event")),
            (C::InvalidAction, &format!("{a}/0/target")),
            (C::InvalidAction, &format!("{a}/1/value")),
            (C::InvalidAction, &format!("{a}/2/target")),
            (C::InvalidAction, &format!("{a}/3/step")),
            (C::InvalidAction, &format!("{a}/3/target")),
            (C::InvalidAction, &format!("{a}/4/durationMs")),
            (C::InvalidAction, &format!("{a}/5")),
            (C::UnknownReference, &format!("{a}/6/group")),
            (C::UnknownReference, &format!("{a}/7/set")),
            (C::UnknownReference, &format!("{a}/8/locale")),
            (C::InvalidAction, &format!("{a}/9/target")),
            (C::InvalidEvent, &format!("{SW}/events/2/event")),
            (C::InvalidEvent, &format!("{SW}/events/2/actions")),
        ],
    );
}

#[test]
fn tags_alarms_and_recipes_are_consistent() {
    let r = report_with(|v| {
        v["units"] = json!([ { "id": "unit-c", "symbol": "°C" } ]);
        v["tags"].as_array_mut().unwrap().extend([
            json!({ "id": "tag-t1", "name": "T1", "dataType": "bool", "unit": "unit-c", "range": { "min": 0, "max": 1 } }),
            json!({ "id": "tag-t2", "name": "T2", "dataType": "int8", "connection": "conn-sim", "initialValue": 300 }),
            json!({ "id": "tag-t3", "name": "T3", "dataType": "float64", "address": "x", "range": { "min": 5, "max": 1 } })
        ]);
        v["alarms"] = json!([
            { "id": "alm-a", "name": "a", "tag": "tag-text", "condition": { "type": "high", "limit": 1 }, "message": "m" },
            { "id": "alm-b", "name": "b", "tag": "tag-word", "condition": { "type": "bitSet", "index": 16 }, "message": "m", "priority": 0 },
            { "id": "alm-c", "name": "c", "tag": "tag-bool-ro", "condition": { "type": "isTrue" }, "deadband": 1, "message": "m" },
            { "id": "alm-d", "name": "d", "tag": "tag-motor", "member": "speed", "condition": { "type": "equals", "value": "x" }, "message": "m" }
        ]);
        at(v, "/recipes/0/fields")
            .as_array_mut()
            .unwrap()
            .push(json!({ "id": "fld-2", "name": "F2", "tag": "tag-word" }));
        at(v, "/recipes/0/sets/0/values")["fld-1"] = json!(true);
        at(v, "/recipes/0/sets/0/values")["fld-x"] = json!(1);
    });
    assert_only_errors(
        &r,
        &[
            (C::InvalidTag, "/tags/6/unit"),
            (C::InvalidTag, "/tags/6/range"),
            (C::InvalidTag, "/tags/7/address"),
            (C::InvalidTag, "/tags/7/initialValue"),
            (C::InvalidTag, "/tags/8/address"),
            (C::InvalidTag, "/tags/8/range"),
            (C::InvalidAlarm, "/alarms/0/condition"),
            (C::InvalidAlarm, "/alarms/1/priority"),
            (C::InvalidAlarm, "/alarms/1/condition/index"),
            (C::InvalidAlarm, "/alarms/2/deadband"),
            (C::InvalidAlarm, "/alarms/3/condition/value"),
            (C::InvalidRecipe, "/recipes/0/fields/1/tag"),
            (C::InvalidRecipe, "/recipes/0/sets/0/values/fld-1"),
            (C::InvalidRecipe, "/recipes/0/sets/0/values/fld-x"),
        ],
    );
    // Missing recipe value for fld-2 is a warning, not an error.
    assert!(r
        .warnings()
        .any(|d| d.code == C::InvalidRecipe && d.message.contains("fld-2")));
}

#[test]
fn localization_rules() {
    let r = report_with(|v| {
        v["localization"] = json!({
            "defaultLocale": "fr",
            "locales": ["en", "it", "en", "not a locale"],
            "strings": {
                "hello": { "en": "Hello", "de": "Hallo" },
                "bad key!": { "en": "x", "it": "x" }
            }
        });
    });
    assert_only_errors(
        &r,
        &[
            (C::InvalidLocalization, "/localization/locales/2"),
            (C::InvalidLocalization, "/localization/locales/3"),
            (C::InvalidLocalization, "/localization/defaultLocale"),
            (C::InvalidLocalization, "/localization/strings/bad key!"),
            (C::InvalidLocalization, "/localization/strings/hello/de"),
        ],
    );
    assert!(r.warnings().any(|d| d.code == C::MissingTranslation));
}

#[test]
fn secrets_in_connections_are_flagged_and_paths_are_sandboxed() {
    let r = report_with(|v| {
        at(v, "/connections/0")["parameters"] = json!({ "Password": "hunter2", "rate": 100 });
        v["resources"] = json!([
            { "id": "res-a", "kind": "image", "path": "../etc/passwd" },
            { "id": "res-b", "kind": "image", "path": "C:\\img.png" },
            { "id": "res-c", "kind": "image", "path": "https://example.com/x.png" },
            { "id": "res-img", "kind": "image", "path": "ok/x.png", "sha256": "ABC" }
        ]);
    });
    assert!(r
        .warnings()
        .any(|d| d.code == C::PossibleSecret && d.path == "/connections/0/parameters/Password"));
    assert_only_errors(
        &r,
        &[
            (C::InvalidResource, "/resources/0/path"),
            (C::InvalidResource, "/resources/1/path"),
            (C::InvalidResource, "/resources/2/path"),
            (C::InvalidResource, "/resources/3/sha256"),
        ],
    );
}

#[test]
fn resource_paths_reject_nonportable_windows_segments() {
    for bad in [
        "assets/.. /x.png",
        "assets/NUL.png",
        "COM1.txt",
        "LPT².png",
        "assets/name.",
        "assets/file ",
        "assets/x?y",
        "assets/\u{0}bad",
    ] {
        let r = report_with(|v| v["resources"][0]["path"] = json!(bad));
        assert_error(&r, C::InvalidResource, "/resources/0/path");
    }
    assert!(!report_with(|v| v["resources"][0]["path"] = json!("assets/图像 1.png")).has_errors());
}

#[test]
fn read_modify_write_actions_and_recipe_upload_require_read_access() {
    let r = report_with(|v| {
        v["tags"][1]["access"] = json!("write");
        v["tags"][2]["access"] = json!("write");
        v["screens"][0]["root"]["children"][2]["bindings"][0]["mode"] = json!("write");
        v["screens"][0]["root"]["children"][3]["events"][0]["actions"] = json!([
            {"type":"toggleTag","target":{"tag":"tag-bool-rw"}},
            {"type":"stepTag","target":{"tag":"tag-real-rw"},"step":1},
            {"type":"writeTag","target":{"tag":"tag-bool-rw"},"value":true},
            {"type":"pulseTag","target":{"tag":"tag-bool-rw"},"durationMs":100},
            {"type":"loadRecipe","recipe":"rcp-1"},
            {"type":"saveRecipe","recipe":"rcp-1"}
        ]);
    });
    assert_only_errors(
        &r,
        &[
            (
                C::InvalidAction,
                "/screens/0/root/children/3/events/0/actions/0/target",
            ),
            (
                C::InvalidAction,
                "/screens/0/root/children/3/events/0/actions/1/target",
            ),
            (
                C::InvalidAction,
                "/screens/0/root/children/3/events/0/actions/5/recipe",
            ),
        ],
    );
}
