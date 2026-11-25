//! Canonical v2 typed transactions and persistence.
#![allow(clippy::unwrap_used)]
mod common;
use common::*;
use hmi_engineering_core::{Command, ProjectSession, SaveOptions, SessionConfig, Transaction};
use serde_json::json;

#[test]
fn v2_transaction_preserves_policy_and_atomic_undo() {
    let mut s = session(minimal());
    let before = canonical(s.project());
    let command:Command=serde_json::from_value(json!({"op":"setIndustrialSecurity","declarations":{"security":{"permissions":[{"id":"perm-view","name":"View"}]},"screens":[{"screen":"scr-main","requiredPermission":"perm-view"}]}})).unwrap();
    s.execute(Transaction::single(command)).unwrap();
    let after = canonical(s.project());
    assert_ne!(before, after);
    assert!(after.contains("perm-view"));
    let copied = s
        .execute(Transaction::single(Command::DuplicateScreen {
            screen: sid("scr-main"),
            name: None,
        }))
        .unwrap();
    assert_eq!(s.project().v2.industrial.screens.len(), 2);
    assert_eq!(
        s.project().v2.industrial.screens[1].screen.as_str(),
        copied.allocated_ids["0:screen"]
    );
    s.undo(None).unwrap().unwrap();
    let bad:Command=serde_json::from_value(json!({"op":"setIndustrialSecurity","declarations":{"screens":[{"screen":"scr-main","requiredPermission":"missing"}]}})).unwrap();
    assert!(s.execute(Transaction::single(bad)).is_err());
    assert_eq!(canonical(s.project()), after);
    s.undo(None).unwrap().unwrap();
    assert_eq!(canonical(s.project()), before);
    s.redo(None).unwrap().unwrap();
    assert_eq!(canonical(s.project()), after);
    let dir = TempDir::new("v2");
    let path = dir.join("v2.hmi.json");
    s.save_as(&path, SaveOptions::default()).unwrap();
    let reopened =
        ProjectSession::open(&path, SessionConfig::default(), std_fs(), &registry()).unwrap();
    assert_eq!(canonical(reopened.session.project()), after);
}

#[test]
fn placement_and_duplication_preserve_contract_permissions_and_actions() {
    let mut s = session(minimal());
    let cmds:Vec<Command>=serde_json::from_value(json!([
        {"op":"insertV2Component","parent":"n-root","id":"n-line","contract":{"kind":"line","start":{"x":0,"y":0},"end":{"x":12,"y":5},"strokeColor":"#000000","strokeWidth":1}},
        {"op":"setIndustrialSecurity","declarations":{"security":{"permissions":[{"id":"perm-view","name":"View"}]},"screens":[{"screen":"scr-main","requiredPermission":"perm-view"}],"nodes":[{"node":"n-line","requiredPermission":"perm-view"}]}},
        {"op":"setComponentActions","declarations":{"components":[{"id":"n-line","contract":{"kind":"line","start":{"x":0,"y":0},"end":{"x":12,"y":5},"strokeColor":"#000000","strokeWidth":1}}],"handlers":[{"origin":{"node":"n-line"},"event":"click","actions":[{"type":"navigate","screen":"scr-main"},{"type":"navigateStartup"}]}]}},
        {"op":"setExpressionEnumerations","declarations":{"expressions":[{"node":"n-line","property":"visible","contractVersion":1,"expression":"true"}]}}
    ])).unwrap();
    s.execute(Transaction::new(cmds)).unwrap();
    let before = canonical(s.project());
    let copy = s
        .execute(Transaction::single(Command::DuplicateScreen {
            screen: sid("scr-main"),
            name: None,
        }))
        .unwrap();
    let new_screen = &copy.allocated_ids["0:screen"];
    let new_node = &copy.allocated_ids["0:node:n-line"];
    let v = serde_json::to_value(&s.project().v2).unwrap();
    assert_eq!(v["industrial"]["screens"][1]["screen"], json!(new_screen));
    assert_eq!(v["industrial"]["nodes"][1]["node"], json!(new_node));
    assert_eq!(
        v["industrial"]["nodes"][1]["requiredPermission"],
        "perm-view"
    );
    assert_eq!(v["bindings"]["expressions"][1]["node"], json!(new_node));
    assert_eq!(v["bindings"]["expressions"][1]["expression"], "true");
    assert_eq!(v["components"]["components"][1]["id"], json!(new_node));
    assert_eq!(
        v["components"]["handlers"][1]["origin"]["node"],
        json!(new_node)
    );
    assert_eq!(
        v["components"]["handlers"][1]["actions"][0]["screen"],
        json!(new_screen)
    );
    assert_eq!(
        v["components"]["handlers"][1]["actions"][1]["type"],
        "navigateStartup"
    );
    let after = canonical(s.project());
    s.undo(None).unwrap().unwrap();
    assert_eq!(canonical(s.project()), before);
    s.redo(None).unwrap().unwrap();
    assert_eq!(canonical(s.project()), after);
    let result = s
        .execute(Transaction::single(Command::DuplicateNode {
            node: nid("n-line"),
            parent: None,
            index: None,
        }))
        .unwrap();
    assert!(s
        .project()
        .v2
        .components
        .components
        .iter()
        .any(|c| c.id.as_str() == result.allocated_ids["0:node:n-line"]));
    // Failed placement and deleting a still-referenced node are atomic.
    let stable = canonical(s.project());
    let existing = s.project().screens[0]
        .root
        .children
        .iter()
        .find(|n| n.id.as_str() == "n-line")
        .unwrap()
        .clone();
    assert!(s
        .execute(Transaction::single(Command::InsertNode {
            parent: nid("n-root"),
            index: None,
            node: existing,
            assign_new_ids: true
        }))
        .is_err());
    let bad:Command=serde_json::from_value(json!({"op":"insertV2Component","parent":"n-hello","contract":{"kind":"line","start":{"x":0,"y":0},"end":{"x":1,"y":1},"strokeColor":"#000000","strokeWidth":1}})).unwrap();
    assert!(s.execute(Transaction::single(bad)).is_err());
    assert!(s
        .execute(Transaction::single(Command::RemoveNode {
            node: nid("n-line")
        }))
        .is_err());
    assert_eq!(canonical(s.project()), stable);
}

#[test]
fn copying_and_moving_cannot_drop_inherited_permissions() {
    let mut s = session(minimal());
    let cmds:Vec<Command>=serde_json::from_value(json!([
        {"op":"createScreen","id":"scr-public","name":"Public","kind":"page","root":{"id":"n-public","element":{"type":"column"}}},
        {"op":"setIndustrialSecurity","declarations":{"security":{"permissions":[{"id":"perm-view","name":"View"}]},"screens":[{"screen":"scr-main","requiredPermission":"perm-view"}]}}
    ])).unwrap();
    s.execute(Transaction::new(cmds)).unwrap();
    let before = canonical(s.project());
    assert!(s
        .execute(Transaction::single(Command::DuplicateNode {
            node: nid("n-hello"),
            parent: Some(nid("n-public")),
            index: None
        }))
        .is_err());
    assert!(s
        .execute(Transaction::single(Command::MoveNode {
            node: nid("n-hello"),
            parent: nid("n-public"),
            index: None
        }))
        .is_err());
    assert_eq!(canonical(s.project()), before);
}

#[test]
fn definition_duplication_keeps_classification_and_remaps_faceplate_self_target() {
    let mut s = session(minimal());
    let cmds:Vec<Command>=serde_json::from_value(json!([
        {"op":"createComponentDefinition","definition":{"id":"cmp-test","name":"Faceplate","root":{"id":"n-fp","element":{"type":"component","kind":"button"}}}},
        {"op":"setComponentActions","declarations":{"definitions":[{"definition":"cmp-test","classification":{"kind":"faceplate"}}],"handlers":[{"origin":{"node":"n-fp"},"event":"click","actions":[{"type":"openFaceplate","definition":"cmp-test"}]}]}}
    ])).unwrap();
    s.execute(Transaction::new(cmds)).unwrap();
    let copy = s
        .execute(Transaction::single(Command::DuplicateComponentDefinition {
            definition: hmi_project_model::model::ComponentDefinitionId::new("cmp-test").unwrap(),
            name: None,
        }))
        .unwrap();
    let v = serde_json::to_value(&s.project().v2).unwrap();
    assert_eq!(
        v["components"]["definitions"][1]["definition"],
        json!(copy.allocated_ids["0:definition"])
    );
    assert_eq!(
        v["components"]["handlers"][1]["origin"]["node"],
        json!(copy.allocated_ids["0:node:n-fp"])
    );
    assert_eq!(
        v["components"]["handlers"][1]["actions"][0]["definition"],
        json!(copy.allocated_ids["0:definition"])
    );
}
