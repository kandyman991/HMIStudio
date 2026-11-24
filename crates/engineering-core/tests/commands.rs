//! Command families, identifier stability and duplication remapping.
#![allow(clippy::unwrap_used)]

mod common;

use std::collections::BTreeSet;

use common::*;
use hmi_engineering_core::ids::collect_ids;
use hmi_engineering_core::locate;
use hmi_engineering_core::{
    ChangeKind, Command, CommandError, EntityKind, Transaction, TransactionError,
};
use hmi_project_model::model::*;
use hmi_project_model::validate;
use serde_json::json;

fn node<'a>(p: &'a Project, id: &str) -> &'a Node {
    locate::get(p, &locate::find(p, &nid(id)).unwrap()).unwrap()
}

fn assert_unique_ids(p: &Project) {
    let ids = collect_ids(p);
    let set: BTreeSet<_> = ids.iter().collect();
    assert_eq!(set.len(), ids.len(), "duplicate ids");
}

#[test]
fn screen_lifecycle() {
    let mut s = session(minimal());
    let o = run(
        &mut s,
        vec![Command::CreateScreen {
            id: None,
            name: "Second".into(),
            kind: ScreenKind::Page,
            root: None,
            index: None,
        }],
    );
    let new_id = o.allocated_ids["0:screen"].clone();
    assert_eq!(new_id, "scr-1");
    assert!(o
        .changes
        .contains(ChangeKind::Added, EntityKind::Screen, &new_id));
    let root_id = o.allocated_ids["0:root"].clone();
    assert_eq!(s.project().screens[1].root.id.as_str(), root_id);
    run(
        &mut s,
        vec![
            Command::RenameScreen {
                screen: sid(&new_id),
                name: "Renamed".into(),
            },
            Command::SetScreenTitle {
                screen: sid(&new_id),
                title: Some(LocalizedText::literal("T")),
            },
        ],
    );
    assert_eq!(s.project().screens[1].name, "Renamed");
    assert_eq!(
        s.project().screens[1].id.as_str(),
        new_id,
        "rename keeps id"
    );
    run(
        &mut s,
        vec![Command::DeleteScreen {
            screen: sid(&new_id),
        }],
    );
    assert_eq!(s.project().screens.len(), 1);
    // Explicit duplicate ids are refused.
    let err = s
        .execute(Transaction::single(Command::CreateScreen {
            id: Some(sid("n-hello")),
            name: "x".into(),
            kind: ScreenKind::Page,
            root: None,
            index: None,
        }))
        .unwrap_err();
    assert!(matches!(
        err,
        TransactionError::Command {
            error: CommandError::AlreadyExists { .. },
            ..
        }
    ));
}

#[test]
fn node_insert_remove_move_reorder() {
    let mut s = session(bottling());
    run(
        &mut s,
        vec![
            Command::InsertNode {
                parent: nid("n-al-actions"),
                index: Some(0),
                node: text_node("n-new", "New"),
                assign_new_ids: false,
            },
            Command::MoveNode {
                node: nid("n-al-back"),
                parent: nid("n-al-root"),
                index: Some(0),
            },
            Command::ReorderNode {
                node: nid("n-al-ack-all"),
                index: 0,
            },
        ],
    );
    let p = s.project();
    let actions: Vec<&str> = node(p, "n-al-actions")
        .children
        .iter()
        .map(|n| n.id.as_str())
        .collect();
    assert_eq!(actions, ["n-al-ack-all", "n-new", "n-al-ack-drives"]);
    assert_eq!(node(p, "n-al-root").children[0].id.as_str(), "n-al-back");
    // Reparent across screens.
    run(
        &mut s,
        vec![Command::MoveNode {
            node: nid("n-new"),
            parent: nid("n-rc-root"),
            index: None,
        }],
    );
    assert_eq!(
        node(s.project(), "n-rc-root")
            .children
            .last()
            .unwrap()
            .id
            .as_str(),
        "n-new"
    );
    run(&mut s, vec![Command::RemoveNode { node: nid("n-new") }]);
    assert!(locate::find(s.project(), &nid("n-new")).is_none());
    // Out-of-range index and root removal.
    let e = s
        .execute(Transaction::single(Command::ReorderNode {
            node: nid("n-al-back"),
            index: 99,
        }))
        .unwrap_err();
    assert!(matches!(
        e,
        TransactionError::Command {
            error: CommandError::IndexOutOfRange { .. },
            ..
        }
    ));
    let e = s
        .execute(Transaction::single(Command::RemoveNode {
            node: nid("n-al-root"),
        }))
        .unwrap_err();
    assert!(matches!(
        e,
        TransactionError::Command {
            error: CommandError::RootNode { .. },
            ..
        }
    ));
    assert!(validate(s.project()).is_valid());
}

#[test]
fn properties_styles_layout_bindings_events() {
    let mut s = session(bottling());
    run(&mut s, vec![
        Command::SetProperty { node: nid("n-ov-title"), name: "fontSize".into(), value: Some(json!(30)) },
        Command::SetNodeStyle { node: nid("n-ov-settings"), style: Some(StyleId::new("style-command").unwrap()) },
        Command::SetLayout { node: nid("n-ov-settings"), layout: serde_json::from_value(json!({ "width": { "mode": "fixed", "value": 120 } })).unwrap() },
        Command::RenameNode { node: nid("n-ov-settings"), name: Some("Settings button".into()) },
        Command::SetBinding { node: nid("n-ov-settings"), binding: serde_json::from_value(json!({ "property": "enabled", "source": { "tag": "tag-auto-mode" } })).unwrap() },
        Command::SetEventHandler { node: nid("n-ov-settings"), handler: serde_json::from_value(json!({ "event": "longPress", "actions": [ { "type": "navigate", "screen": "scr-alarms" } ] })).unwrap() },
        Command::SetResponsive { node: nid("n-ov-settings"), rules: serde_json::from_value(json!([ { "when": { "breakpoint": "bp-compact" }, "set": { "visibility": "collapsed" } } ])).unwrap() },
    ]);
    let n = node(s.project(), "n-ov-settings");
    assert_eq!(n.style.as_ref().unwrap().as_str(), "style-command");
    assert_eq!(n.layout.width, Size::fixed(120.0));
    assert_eq!(n.bindings.len(), 1);
    assert_eq!(n.events.len(), 2);
    assert_eq!(n.responsive.len(), 1);
    // Replace by key, then remove.
    run(
        &mut s,
        vec![
            Command::SetBinding {
                node: nid("n-ov-settings"),
                binding: serde_json::from_value(
                    json!({ "property": "enabled", "source": { "tag": "tag-hmi-dark-mode" } }),
                )
                .unwrap(),
            },
            Command::RemoveEventHandler {
                node: nid("n-ov-settings"),
                event: EventKind::LongPress,
            },
        ],
    );
    let n = node(s.project(), "n-ov-settings");
    assert_eq!(n.bindings.len(), 1);
    assert_eq!(n.events.len(), 1);
    run(
        &mut s,
        vec![
            Command::RemoveBinding {
                node: nid("n-ov-settings"),
                property: "enabled".into(),
            },
            Command::SetProperty {
                node: nid("n-ov-title"),
                name: "fontSize".into(),
                value: None,
            },
        ],
    );
    // Invalid property type is refused by validation.
    let e = s
        .execute(Transaction::single(Command::SetProperty {
            node: nid("n-ov-title"),
            name: "maxLines".into(),
            value: Some(json!("two")),
        }))
        .unwrap_err();
    assert!(matches!(e, TransactionError::Validation { .. }));
    // Removing an absent binding is a command error.
    let e = s
        .execute(Transaction::single(Command::RemoveBinding {
            node: nid("n-ov-title"),
            property: "x".into(),
        }))
        .unwrap_err();
    assert!(matches!(
        e,
        TransactionError::Command {
            error: CommandError::NotFound { .. },
            ..
        }
    ));
}

#[test]
fn tag_lifecycle() {
    let mut s = session(bottling());
    let tag: Tag = serde_json::from_value(
        json!({ "id": "tag-new", "name": "New", "dataType": "float32", "access": "readWrite" }),
    )
    .unwrap();
    run(
        &mut s,
        vec![Command::CreateTag {
            tag: tag.clone(),
            index: Some(0),
        }],
    );
    assert_eq!(s.project().tags[0].id.as_str(), "tag-new");
    let mut changed = tag.clone();
    changed.name = "Renamed".into();
    run(&mut s, vec![
        Command::UpdateTag { tag: changed },
        Command::InsertNode { parent: nid("n-rc-root"), index: None, node: serde_json::from_value(json!({ "id": "n-use", "element": { "type": "component", "kind": "numericInput" }, "bindings": [ { "property": "value", "source": { "tag": "tag-new" }, "mode": "readWrite" } ] })).unwrap(), assign_new_ids: false },
    ]);
    assert_eq!(s.project().tags[0].name, "Renamed");
    // Referenced: refused. Unreferenced after removing the user: allowed.
    assert!(s
        .execute(Transaction::single(Command::DeleteTag {
            tag: TagId::new("tag-new").unwrap()
        }))
        .is_err());
    run(
        &mut s,
        vec![
            Command::RemoveNode { node: nid("n-use") },
            Command::DeleteTag {
                tag: TagId::new("tag-new").unwrap(),
            },
        ],
    );
    assert!(s.project().tags.iter().all(|t| t.id.as_str() != "tag-new"));
    // Duplicate tag id refused.
    let e = s
        .execute(Transaction::single(Command::CreateTag {
            tag: serde_json::from_value(
                json!({ "id": "tag-auto-mode", "name": "x", "dataType": "bool" }),
            )
            .unwrap(),
            index: None,
        }))
        .unwrap_err();
    assert!(matches!(
        e,
        TransactionError::Command {
            error: CommandError::AlreadyExists { .. },
            ..
        }
    ));
}

#[test]
fn duplicate_node_gets_new_ids_and_keeps_original() {
    let mut s = session(bottling());
    let original = node(s.project(), "n-ov-speed-card").clone();
    // Into a column the grid placement becomes invalid: the gate refuses...
    let e = s
        .execute(Transaction::single(Command::DuplicateNode {
            node: nid("n-ov-speed-card"),
            parent: Some(nid("n-rc-root")),
            index: None,
        }))
        .unwrap_err();
    assert!(matches!(e, TransactionError::Validation { .. }));
    // ...so duplicate and fix the layout atomically (ids are deterministic).
    let o = run(
        &mut s,
        vec![
            Command::DuplicateNode {
                node: nid("n-ov-speed-card"),
                parent: Some(nid("n-rc-root")),
                index: None,
            },
            Command::SetLayout {
                node: nid("n-ov-speed-card-1"),
                layout: LayoutParams::default(),
            },
        ],
    );
    let p = s.project();
    assert_eq!(node(p, "n-ov-speed-card"), &original, "original untouched");
    let copy = node(p, "n-rc-root").children.last().unwrap();
    let mut orig_ids = Vec::new();
    let mut copy_ids = Vec::new();
    hmi_engineering_core::ids::collect_node_ids(&original, &mut orig_ids);
    hmi_engineering_core::ids::collect_node_ids(copy, &mut copy_ids);
    assert_eq!(orig_ids.len(), copy_ids.len());
    for (a, b) in orig_ids.iter().zip(&copy_ids) {
        assert_ne!(a, b);
        assert_eq!(&o.allocated_ids[&format!("0:node:{a}")], b);
    }
    // Structure, bindings and events are preserved except for ids.
    let mut stripped = copy.clone();
    let mut map = orig_ids.iter().zip(&copy_ids);
    fn restore(n: &mut Node, map: &mut dyn Iterator<Item = (&String, &String)>) {
        let (old, _) = map.next().unwrap();
        n.id = NodeId::new_unchecked(old.clone());
        for c in &mut n.children {
            restore(c, map);
        }
    }
    restore(&mut stripped, &mut map);
    // The copy's layout was reset deliberately; compare everything else.
    stripped.layout = original.layout.clone();
    assert_eq!(stripped, original);
    assert_unique_ids(p);
}

#[test]
fn duplicate_in_place_lands_after_original() {
    let mut s = session(minimal());
    let o = run(
        &mut s,
        vec![Command::DuplicateNode {
            node: nid("n-hello"),
            parent: None,
            index: None,
        }],
    );
    let children = &s.project().screens[0].root.children;
    assert_eq!(children.len(), 2);
    assert_eq!(children[0].id.as_str(), "n-hello");
    assert_eq!(children[1].id.as_str(), o.allocated_ids["0:node:n-hello"]);
    assert_eq!(children[1].id.as_str(), "n-hello-1");
}

#[test]
fn duplicate_screen_remaps_self_references() {
    let mut s = session(bottling());
    // Give the alarms screen a self-navigation (refresh) button.
    run(&mut s, vec![Command::SetEventHandler {
        node: nid("n-al-back"),
        handler: serde_json::from_value(json!({ "event": "longPress", "actions": [ { "type": "navigate", "screen": "scr-alarms" } ] })).unwrap_or_else(|_| unreachable!()),
    }]);
    // Buttons raise longPress; n-al-back is a button.
    let o = run(
        &mut s,
        vec![Command::DuplicateScreen {
            screen: sid("scr-alarms"),
            name: None,
        }],
    );
    let new_screen = o.allocated_ids["0:screen"].clone();
    let p = s.project();
    let i = p
        .screens
        .iter()
        .position(|x| x.id.as_str() == new_screen)
        .unwrap();
    assert_eq!(p.screens[i - 1].id.as_str(), "scr-alarms");
    assert_eq!(p.screens[i].name, "Alarms copy");
    let back_copy = o.allocated_ids["0:node:n-al-back"].clone();
    let n = node(p, &back_copy);
    let lp = n
        .events
        .iter()
        .find(|h| h.event == EventKind::LongPress)
        .unwrap();
    assert_eq!(
        lp.actions[0],
        Action::Navigate {
            screen: sid(&new_screen)
        },
        "self reference remapped"
    );
    let click = n
        .events
        .iter()
        .find(|h| h.event == EventKind::Click)
        .unwrap();
    assert_eq!(
        click.actions[0],
        Action::Navigate {
            screen: sid("scr-overview")
        },
        "external reference kept"
    );
    // Original still targets itself.
    let orig = node(p, "n-al-back");
    assert!(orig
        .events
        .iter()
        .any(|h| h.actions.contains(&Action::Navigate {
            screen: sid("scr-alarms")
        })));
    assert_unique_ids(p);
    assert!(validate(p).is_valid());
}

#[test]
fn component_definitions_lifecycle_and_duplication() {
    let mut s = session(bottling());
    let o = run(
        &mut s,
        vec![Command::DuplicateComponentDefinition {
            definition: ComponentDefinitionId::new("cmp-motor-faceplate").unwrap(),
            name: Some("Pump faceplate".into()),
        }],
    );
    let new_def = o.allocated_ids["0:definition"].clone();
    assert_eq!(new_def, "cmp-motor-faceplate-1");
    let p = s.project();
    let d = p
        .components
        .iter()
        .find(|d| d.id.as_str() == new_def)
        .unwrap();
    assert_eq!(d.name, "Pump faceplate");
    assert_ne!(d.root.id.as_str(), "n-mf-root");
    assert_unique_ids(p);
    run(&mut s, vec![
        Command::UpdateComponentDefinitionInfo { definition: ComponentDefinitionId::new(&new_def).unwrap(), name: "Pump".into(), description: Some("d".into()), category: Some("Pumps".into()) },
        Command::InsertNode { parent: nid("n-rc-root"), index: None, node: serde_json::from_value(json!({ "id": "n-pump", "element": { "type": "instance", "definition": new_def, "parameters": { "motor": "tag-filler-motor" } } })).unwrap(), assign_new_ids: false },
    ]);
    // In use: deletion refused; after removing the instance: allowed.
    let did = ComponentDefinitionId::new(&new_def).unwrap();
    assert!(s
        .execute(Transaction::single(Command::DeleteComponentDefinition {
            definition: did.clone()
        }))
        .is_err());
    run(
        &mut s,
        vec![
            Command::RemoveNode {
                node: nid("n-pump"),
            },
            Command::DeleteComponentDefinition { definition: did },
        ],
    );
    // New definition + parameters.
    let def: ComponentDefinition = serde_json::from_value(json!({ "id": "cmp-new", "name": "New", "root": { "id": "n-new-root", "element": { "type": "row" } } })).unwrap();
    run(
        &mut s,
        vec![
            Command::CreateComponentDefinition { definition: def },
            Command::SetComponentParameters {
                definition: ComponentDefinitionId::new("cmp-new").unwrap(),
                parameters: serde_json::from_value(
                    json!([ { "name": "caption", "type": { "kind": "text" }, "default": "x" } ]),
                )
                .unwrap(),
            },
        ],
    );
    assert_eq!(s.project().components.last().unwrap().parameters.len(), 1);
}

#[test]
fn project_settings_and_extensions() {
    let mut s = session(bottling());
    let mut meta = s.project().metadata.clone();
    meta.revision = Some("2.0.0".into());
    let mut settings = s.project().settings.clone();
    settings.default_theme = Some(ThemeId::new("theme-night").unwrap());
    run(
        &mut s,
        vec![
            Command::SetMetadata { metadata: meta },
            Command::SetSettings { settings },
            Command::SetExtension {
                key: "studio.layout".into(),
                value: Some(json!({ "zoom": 1.5 })),
            },
        ],
    );
    assert_eq!(s.project().metadata.revision.as_deref(), Some("2.0.0"));
    assert_eq!(
        s.project()
            .settings
            .default_theme
            .as_ref()
            .unwrap()
            .as_str(),
        "theme-night"
    );
    let mut bad = s.project().settings.clone();
    bad.default_theme = Some(ThemeId::new("theme-x").unwrap());
    assert!(s
        .execute(Transaction::single(Command::SetSettings { settings: bad }))
        .is_err());
    run(
        &mut s,
        vec![Command::SetExtension {
            key: "studio.layout".into(),
            value: None,
        }],
    );
    assert!(!s.project().extensions.contains_key("studio.layout"));
}

#[test]
fn insert_with_assign_new_ids_never_collides() {
    let mut s = session(minimal());
    for _ in 0..10 {
        run(
            &mut s,
            vec![Command::InsertNode {
                parent: nid("n-root"),
                index: None,
                node: text_node("n-hello", "dup"),
                assign_new_ids: true,
            }],
        );
    }
    assert_eq!(s.project().screens[0].root.children.len(), 11);
    assert_unique_ids(s.project());
    let e = s
        .execute(Transaction::single(Command::InsertNode {
            parent: nid("n-root"),
            index: None,
            node: text_node("n-hello", "dup"),
            assign_new_ids: false,
        }))
        .unwrap_err();
    assert!(matches!(
        e,
        TransactionError::Command {
            error: CommandError::AlreadyExists { .. },
            ..
        }
    ));
}
