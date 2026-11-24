//! Invalid references, invalid hierarchies and circular references.
#![allow(clippy::unwrap_used)]

mod common;

use common::*;
use hmi_project_model::DiagnosticCode as C;
use serde_json::json;

#[test]
fn unknown_references_are_reported() {
    let r = report_with(|v| {
        at(v, "/navigation")["startScreen"] = json!("scr-missing");
        at(v, "/tags/0")["connection"] = json!("conn-missing");
        at(v, "/alarms/0")["tag"] = json!("tag-missing");
        at(v, "/recipes/0/fields/0")["tag"] = json!("tag-missing");
        at(v, "/screens/0/root/children/0")["style"] = json!("style-missing");
        at(v, "/screens/0/root/children/0/properties")["text"] =
            json!({ "textKey": "missing.key" });
        at(v, "/screens/0/root/children/1/element")["definition"] = json!("cmp-missing");
        at(v, "/screens/0/root/children/2/bindings/0/source")["tag"] = json!("tag-missing");
        at(v, "/screens/0/root/children/3/events/0/actions/0")["screen"] = json!("pop-missing");
    });
    assert_only_errors(
        &r,
        &[
            (C::UnknownReference, "/tags/0/connection"),
            (C::UnknownReference, "/alarms/0/tag"),
            (C::UnknownReference, "/recipes/0/fields/0/tag"),
            (C::UnknownReference, "/navigation/startScreen"),
            (
                C::UnknownReference,
                "/screens/0/root/children/0/properties/text",
            ),
            (C::UnknownReference, "/screens/0/root/children/0/style"),
            (
                C::UnknownReference,
                "/screens/0/root/children/1/element/definition",
            ),
            (
                C::UnknownReference,
                "/screens/0/root/children/2/bindings/0/source",
            ),
            (
                C::UnknownReference,
                "/screens/0/root/children/3/events/0/actions/0/screen",
            ),
        ],
    );
}

#[test]
fn references_to_the_wrong_kind_of_object() {
    let r = report_with(|v| {
        at(v, "/navigation")["startScreen"] = json!("pop-info");
        at(v, "/screens/0/root/children/3/events/0/actions/0")
            .clone_from(&json!({ "type": "navigate", "screen": "pop-info" }));
        at(v, "/screens/0/root/children/0").clone_from(&json!({
            "id": "n-text", "element": { "type": "component", "kind": "image" },
            "properties": { "source": "res-img", "tint": { "token": "nope" } }
        }));
        at(v, "/resources/0")["kind"] = json!("font");
    });
    assert_only_errors(
        &r,
        &[
            (C::InvalidReference, "/navigation/startScreen"),
            (
                C::InvalidReference,
                "/screens/0/root/children/0/properties/source",
            ),
            (
                C::UnknownReference,
                "/screens/0/root/children/0/properties/tint",
            ),
            (
                C::InvalidAction,
                "/screens/0/root/children/3/events/0/actions/0/screen",
            ),
        ],
    );
}

#[test]
fn invalid_component_hierarchies() {
    let r = report_with(|v| {
        let leaf =
            json!({ "id": "n-child", "element": { "type": "component", "kind": "rectangle" } });
        at(v, "/screens/0/root/children/0")["children"] = json!([leaf]);
        at(v, "/screens/0/root/children/1")["children"] =
            json!([{ "id": "n-child2", "element": { "type": "box" } }]);
        at(v, "/screens/1/root")["children"] = json!([]);
        at(v, "/screens/1/root/element").clone_from(&json!({ "type": "scroll" }));
    });
    assert_only_errors(
        &r,
        &[
            (C::InvalidHierarchy, "/screens/0/root/children/0/children"),
            (C::InvalidHierarchy, "/screens/0/root/children/1/children"),
            (C::InvalidHierarchy, "/screens/1/root/children"),
        ],
    );
}

#[test]
fn container_accepts_one_child_and_scroll_exactly_one() {
    let mk = |n: usize, kind: serde_json::Value| {
        let children: Vec<_> = (0..n)
            .map(|i| json!({ "id": format!("n-c{i}"), "element": { "type": "box" } }))
            .collect();
        report_with(|v| {
            at(v, "/screens/1")["root"] =
                json!({ "id": "n-pop-root", "element": kind, "children": children });
        })
    };
    let container = json!({ "type": "component", "kind": "container" });
    let scroll = json!({ "type": "scroll" });
    assert!(mk(0, container.clone()).is_valid());
    assert!(mk(1, container.clone()).is_valid());
    assert!(mk(2, container).has_code(C::InvalidHierarchy));
    assert!(mk(0, scroll.clone()).has_code(C::InvalidHierarchy));
    assert!(mk(1, scroll.clone()).is_valid());
    assert!(mk(2, scroll).has_code(C::InvalidHierarchy));
}

fn definition(id: &str, instances: &[&str]) -> serde_json::Value {
    let children: Vec<_> = instances
        .iter()
        .enumerate()
        .map(|(i, d)| json!({ "id": format!("n-{id}-{i}"), "element": { "type": "instance", "definition": d } }))
        .collect();
    json!({ "id": id, "name": id, "root": { "id": format!("n-{id}-root"), "element": { "type": "row" }, "children": children } })
}

#[test]
fn self_recursive_component_is_detected() {
    let r = report_with(|v| {
        v["components"]
            .as_array_mut()
            .unwrap()
            .push(definition("cmp-self", &["cmp-self"]));
    });
    let d = r.with_code(C::CircularComponentReference);
    assert_eq!(d.len(), 1, "{r}");
    assert_eq!(d[0].path, "/components/1");
    assert!(d[0].message.contains("cmp-self -> cmp-self"));
}

#[test]
fn indirect_component_cycles_are_reported_once_each() {
    let r = report_with(|v| {
        let c = v["components"].as_array_mut().unwrap();
        c.push(definition("cmp-a", &["cmp-b"]));
        c.push(definition("cmp-b", &["cmp-c", "cmp-lamp"]));
        c.push(definition("cmp-c", &["cmp-a"]));
        c.push(definition("cmp-x", &["cmp-y"]));
        c.push(definition("cmp-y", &["cmp-x"]));
        c.push(definition("cmp-ok", &["cmp-lamp", "cmp-lamp"]));
    });
    let d = r.with_code(C::CircularComponentReference);
    let messages: Vec<&str> = d.iter().map(|d| d.message.as_str()).collect();
    assert_eq!(d.len(), 2, "{r}");
    assert!(
        messages[0].ends_with("cmp-a -> cmp-b -> cmp-c -> cmp-a"),
        "{messages:?}"
    );
    assert!(
        messages[1].ends_with("cmp-x -> cmp-y -> cmp-x"),
        "{messages:?}"
    );
    assert_eq!(d[0].path, "/components/1");
    assert_eq!(d[1].path, "/components/4");
}

#[test]
fn circular_data_types_are_detected() {
    let r = report_with(|v| {
        let t = v["dataTypes"].as_array_mut().unwrap();
        t.push(json!({ "id": "dt-a", "name": "A", "members": [ { "name": "b", "dataType": { "struct": "dt-b" } } ] }));
        t.push(json!({ "id": "dt-b", "name": "B", "members": [ { "name": "a", "dataType": { "array": { "element": { "struct": "dt-a" }, "length": 2 } } } ] }));
        t.push(json!({ "id": "dt-s", "name": "S", "members": [ { "name": "me", "dataType": { "struct": "dt-s" } } ] }));
    });
    let d = r.with_code(C::CircularDataType);
    assert_eq!(d.len(), 2, "{r}");
    assert!(d[0].message.ends_with("dt-a -> dt-b -> dt-a"));
    assert!(d[1].message.ends_with("dt-s -> dt-s"));
}

#[test]
fn properties_are_typed_per_component_kind() {
    let r = report_with(|v| {
        at(v, "/screens/0/root/children/0")["properties"] = json!({
            "text": 42,
            "maxLines": 0,
            "overflow": "wrap",
            "textColor": "red",
            "opacity": 2,
            "visible": "yes",
            "checked": true
        });
        at(v, "/screens/0/root")["properties"] = json!({ "text": "rows have no text" });
    });
    let p = "/screens/0/root/children/0/properties";
    assert_only_errors(
        &r,
        &[
            (C::UnknownProperty, "/screens/0/root/properties/text"),
            (C::UnknownProperty, &format!("{p}/checked")),
            (C::InvalidPropertyValue, &format!("{p}/maxLines")),
            (C::InvalidPropertyValue, &format!("{p}/opacity")),
            (C::InvalidPropertyValue, &format!("{p}/overflow")),
            (C::InvalidPropertyType, &format!("{p}/text")),
            (C::InvalidPropertyValue, &format!("{p}/textColor")),
            (C::InvalidPropertyType, &format!("{p}/visible")),
        ],
    );
}

#[test]
fn instance_parameters_are_checked() {
    let r = report_with(|v| {
        at(v, "/screens/0/root/children/1/element")["parameters"] =
            json!({ "caption": 5, "nope": 1 });
        at(v, "/screens/0/root/children/1")["properties"] = json!({ "caption": "x" });
    });
    let p = "/screens/0/root/children/1";
    assert_only_errors(
        &r,
        &[
            (
                C::InvalidPropertyType,
                &format!("{p}/element/parameters/caption"),
            ),
            (C::InvalidParameter, &format!("{p}/element/parameters/nope")),
            (C::InvalidParameter, &format!("{p}/element/parameters")),
            (C::UnknownProperty, &format!("{p}/properties/caption")),
        ],
    );
}

#[test]
fn tag_parameter_type_and_access_are_enforced_per_instance() {
    let r = report_with(|v| {
        at(v, "/screens/0/root/children/1/element")["parameters"] =
            json!({ "source": "tag-real-rw" });
    });
    assert_only_errors(
        &r,
        &[(
            C::InvalidParameter,
            "/screens/0/root/children/1/element/parameters/source",
        )],
    );
}

#[test]
fn styles_must_match_the_node_kind() {
    let r = report_with(|v| {
        v["styles"] = json!([
            { "id": "style-btn", "name": "Btn", "target": "button", "properties": { "behavior": "toggle" } },
            { "id": "style-bad", "name": "Bad", "target": "text", "properties": { "behavior": "toggle" } }
        ]);
        at(v, "/screens/0/root/children/0")["style"] = json!("style-btn");
    });
    assert_only_errors(
        &r,
        &[
            (C::UnknownProperty, "/styles/1/properties/behavior"),
            (C::InvalidStyle, "/screens/0/root/children/0/style"),
        ],
    );
}
