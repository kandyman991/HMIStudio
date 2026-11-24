//! Undo/redo consistency, grouping, limits and dirty state.
#![allow(clippy::unwrap_used)]

mod common;

use common::*;
use hmi_engineering_core::{
    ChangeKind, Command, EntityKind, ProjectSession, SaveOptions, SessionConfig, Transaction,
};
use hmi_project_model::model::*;
use serde_json::json;

fn edits() -> Vec<Transaction> {
    vec![
        Transaction::single(Command::InsertNode {
            parent: nid("n-ov-root"),
            index: Some(0),
            node: text_node("x", "a"),
            assign_new_ids: true,
        }),
        Transaction::single(Command::MoveNode {
            node: nid("n-ov-kpis"),
            parent: nid("n-rc-root"),
            index: Some(1),
        }),
        Transaction::single(Command::DuplicateNode {
            node: nid("n-ov-speed-card"),
            parent: None,
            index: None,
        }),
        Transaction::single(Command::RemoveNode {
            node: nid("n-al-list"),
        }),
        Transaction::single(Command::DuplicateScreen {
            screen: sid("scr-filler"),
            name: None,
        }),
        Transaction::single(Command::SetProperty {
            node: nid("n-ov-title"),
            name: "fontSize".into(),
            value: Some(json!(40)),
        }),
    ]
}

#[test]
fn undo_and_redo_restore_exact_states() {
    let mut s = session(bottling());
    let mut states = vec![canonical(s.project())];
    for t in edits() {
        s.execute(t).unwrap();
        states.push(canonical(s.project()));
    }
    for i in (0..states.len() - 1).rev() {
        let o = s.undo(None).unwrap().unwrap();
        assert!(o.changed);
        assert_eq!(canonical(s.project()), states[i], "after undo to state {i}");
    }
    assert!(s.undo(None).unwrap().is_none());
    for (i, state) in states.iter().enumerate().skip(1) {
        s.redo(None).unwrap().unwrap();
        assert_eq!(&canonical(s.project()), state, "after redo to state {i}");
    }
    assert!(s.redo(None).unwrap().is_none());
    // Revision counts every commit, undo and redo.
    let n = edits().len() as u64;
    assert_eq!(s.revision(), n * 3);
}

#[test]
fn undo_reports_inverted_changes() {
    let mut s = session(minimal());
    let o = run(
        &mut s,
        vec![Command::InsertNode {
            parent: nid("n-root"),
            index: None,
            node: text_node("n-a", "a"),
            assign_new_ids: false,
        }],
    );
    assert!(o
        .changes
        .contains(ChangeKind::Added, EntityKind::Node, "n-a"));
    let u = s.undo(None).unwrap().unwrap();
    assert!(u
        .changes
        .contains(ChangeKind::Removed, EntityKind::Node, "n-a"));
    let r = s.redo(None).unwrap().unwrap();
    assert!(r
        .changes
        .contains(ChangeKind::Added, EntityKind::Node, "n-a"));
}

#[test]
fn divergent_edit_invalidates_redo() {
    let mut s = session(minimal());
    run(
        &mut s,
        vec![Command::RenameScreen {
            screen: sid("scr-main"),
            name: "A".into(),
        }],
    );
    run(
        &mut s,
        vec![Command::RenameScreen {
            screen: sid("scr-main"),
            name: "B".into(),
        }],
    );
    s.undo(None).unwrap();
    assert_eq!(s.redo_depth(), 1);
    run(
        &mut s,
        vec![Command::RenameScreen {
            screen: sid("scr-main"),
            name: "C".into(),
        }],
    );
    assert_eq!(s.redo_depth(), 0);
    assert!(s.redo(None).unwrap().is_none());
    assert_eq!(s.project().screens[0].name, "C");
    // A failed transaction does not clear redo.
    s.undo(None).unwrap();
    assert!(s
        .execute(Transaction::single(Command::RemoveNode {
            node: nid("nope")
        }))
        .is_err());
    assert_eq!(s.redo_depth(), 1);
}

#[test]
fn history_limit_drops_oldest_steps() {
    let mut s = ProjectSession::new(
        minimal(),
        SessionConfig {
            history_limit: 3,
            ..SessionConfig::default()
        },
    );
    for i in 0..10 {
        run(
            &mut s,
            vec![Command::RenameScreen {
                screen: sid("scr-main"),
                name: format!("N{i}"),
            }],
        );
    }
    assert_eq!(s.undo_depth(), 3);
    for _ in 0..3 {
        s.undo(None).unwrap().unwrap();
    }
    assert!(s.undo(None).unwrap().is_none());
    assert_eq!(s.project().screens[0].name, "N6");
    assert!(s.is_dirty(), "the saved state is no longer reachable");
    let mut none = ProjectSession::new(
        minimal(),
        SessionConfig {
            history_limit: 0,
            ..SessionConfig::default()
        },
    );
    run(
        &mut none,
        vec![Command::RenameScreen {
            screen: sid("scr-main"),
            name: "x".into(),
        }],
    );
    assert!(!none.status().can_undo);
}

#[test]
fn multi_command_transactions_and_merge_keys_group_undo_steps() {
    let mut s = session(bottling());
    let start = canonical(s.project());
    // Multi-command transaction = one step.
    run(
        &mut s,
        vec![
            Command::RenameScreen {
                screen: sid("scr-filler"),
                name: "F".into(),
            },
            Command::RemoveNode {
                node: nid("n-fl-back"),
            },
        ],
    );
    assert_eq!(s.undo_depth(), 1);
    // A slider drag: 20 merged transactions = one step.
    for v in 0..20 {
        let o = s
            .execute(
                Transaction::single(Command::SetProperty {
                    node: nid("n-ov-speed-slider"),
                    name: "step".into(),
                    value: Some(json!(v)),
                })
                .merging("drag"),
            )
            .unwrap();
        assert_eq!(o.merged, v > 0);
    }
    assert_eq!(s.undo_depth(), 2);
    // A different key or an unkeyed transaction starts a new step.
    s.execute(
        Transaction::single(Command::SetProperty {
            node: nid("n-ov-speed-slider"),
            name: "step".into(),
            value: Some(json!(5)),
        })
        .merging("other"),
    )
    .unwrap();
    assert_eq!(s.undo_depth(), 3);
    s.undo(None).unwrap();
    s.undo(None).unwrap();
    assert_eq!(
        locate_prop(&s, "n-ov-speed-slider", "step"),
        Some(json!(10)),
        "whole drag undone in one step"
    );
    s.undo(None).unwrap();
    assert_eq!(canonical(s.project()), start);
    assert!(!s.is_dirty());
}

fn locate_prop(s: &ProjectSession, node: &str, name: &str) -> Option<serde_json::Value> {
    let p = s.project();
    let loc = hmi_engineering_core::locate::find(p, &nid(node))?;
    hmi_engineering_core::locate::get(p, &loc)?
        .properties
        .get(name)
        .cloned()
}

#[test]
fn undo_after_merge_does_not_reopen_the_group() {
    let mut s = session(bottling());
    let t = |v: i64| {
        Transaction::single(Command::SetProperty {
            node: nid("n-ov-speed-slider"),
            name: "step".into(),
            value: Some(json!(v)),
        })
        .merging("drag")
    };
    s.execute(t(1)).unwrap();
    s.execute(t(2)).unwrap();
    s.undo(None).unwrap();
    let o = s.execute(t(3)).unwrap();
    assert!(!o.merged);
}

#[test]
fn dirty_state_follows_undo_redo_and_save() {
    let dir = TempDir::new("dirty");
    let path = dir.join("p.hmi.json");
    let mut s = session(minimal());
    assert!(!s.is_dirty());
    run(
        &mut s,
        vec![Command::RenameScreen {
            screen: sid("scr-main"),
            name: "A".into(),
        }],
    );
    assert!(s.is_dirty());
    s.undo(None).unwrap();
    assert!(!s.is_dirty(), "back at the clean state");
    s.redo(None).unwrap();
    assert!(s.is_dirty());
    s.save_as(&path, SaveOptions::default()).unwrap();
    assert!(!s.is_dirty());
    s.undo(None).unwrap();
    assert!(s.is_dirty(), "undo past the save point is unsaved");
    s.redo(None).unwrap();
    assert!(!s.is_dirty(), "redo back to the saved state is clean");
    run(
        &mut s,
        vec![Command::RenameScreen {
            screen: sid("scr-main"),
            name: "B".into(),
        }],
    );
    assert!(s.is_dirty());
    run(
        &mut s,
        vec![Command::RenameScreen {
            screen: sid("scr-main"),
            name: "A".into(),
        }],
    );
    assert!(
        s.is_dirty(),
        "equal content via a new edit is still a new state"
    );
    let st = s.status();
    assert!(st.can_undo && !st.can_redo);
    assert_eq!(st.undo_label.as_deref(), Some("renameScreen"));
}

#[test]
fn identifiers_are_never_reused_after_undo() {
    let mut s = session(minimal());
    let a = run(
        &mut s,
        vec![Command::DuplicateNode {
            node: nid("n-hello"),
            parent: None,
            index: None,
        }],
    );
    let first = a.allocated_ids["0:node:n-hello"].clone();
    s.undo(None).unwrap();
    let b = run(
        &mut s,
        vec![Command::DuplicateNode {
            node: nid("n-hello"),
            parent: None,
            index: None,
        }],
    );
    let second = b.allocated_ids["0:node:n-hello"].clone();
    assert_ne!(
        first, second,
        "an id seen earlier in the session is retired"
    );
    assert_eq!(
        (first.as_str(), second.as_str()),
        ("n-hello-1", "n-hello-2")
    );
}

#[test]
fn undo_restores_references_exactly() {
    let mut s = session(bottling());
    let before = s.project().clone();
    run(
        &mut s,
        vec![
            Command::RemoveNode {
                node: nid("n-ov-conveyor"),
            },
            Command::RemoveNode {
                node: nid("n-ov-filler"),
            },
            Command::RemoveNode {
                node: nid("n-fl-motor"),
            },
            Command::DeleteComponentDefinition {
                definition: ComponentDefinitionId::new("cmp-motor-faceplate").unwrap(),
            },
        ],
    );
    assert!(s
        .project()
        .components
        .iter()
        .all(|d| d.id.as_str() != "cmp-motor-faceplate"));
    s.undo(None).unwrap();
    assert_eq!(s.project(), &before);
    assert!(s.report().is_valid());
}
