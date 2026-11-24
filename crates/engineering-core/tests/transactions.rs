//! Atomic transactions, rollback, validation gate, revisions, determinism.
#![allow(clippy::unwrap_used)]

mod common;

use common::*;
use hmi_engineering_core::{
    Command, CommandError, ProjectSession, Transaction, TransactionError, ValidationPolicy,
};
use hmi_project_model::model::*;
use hmi_project_model::DiagnosticCode;
use serde_json::json;

fn assert_untouched(s: &ProjectSession, before: &Project, rev: u64, undo: usize) {
    assert_eq!(
        s.project(),
        before,
        "project changed after failed transaction"
    );
    assert_eq!(s.revision(), rev);
    assert_eq!(s.undo_depth(), undo);
}

#[test]
fn multi_command_transaction_is_atomic_on_command_failure() {
    let mut s = session(minimal());
    let before = s.project().clone();
    let err = s
        .execute(Transaction::new(vec![
            Command::RenameScreen {
                screen: sid("scr-main"),
                name: "Renamed".into(),
            },
            Command::InsertNode {
                parent: nid("n-root"),
                index: None,
                node: text_node("n-a", "a"),
                assign_new_ids: false,
            },
            Command::SetProperty {
                node: nid("n-a"),
                name: "text".into(),
                value: Some(json!("b")),
            },
            Command::RemoveNode {
                node: nid("n-missing"),
            },
        ]))
        .unwrap_err();
    assert!(
        matches!(
            err,
            TransactionError::Command {
                index: 3,
                op: "removeNode",
                error: CommandError::NotFound { .. }
            }
        ),
        "{err:?}"
    );
    assert_untouched(&s, &before, 0, 0);
    assert!(!s.is_dirty());
}

#[test]
fn transaction_rejected_by_validation_rolls_back_everything() {
    let mut s = session(bottling());
    let before = s.project().clone();
    let err = s
        .execute(Transaction::new(vec![
            Command::RenameScreen {
                screen: sid("scr-filler"),
                name: "Changed".into(),
            },
            // Still referenced by alarms, recipes, bindings and actions.
            Command::DeleteTag {
                tag: TagId::new("tag-line-speed-sp").unwrap(),
            },
        ]))
        .unwrap_err();
    let TransactionError::Validation { errors } = err else {
        panic!("{err:?}")
    };
    assert!(
        errors
            .iter()
            .all(|d| d.code == DiagnosticCode::UnknownReference),
        "{errors:?}"
    );
    assert!(errors.len() >= 5);
    assert_untouched(&s, &before, 0, 0);
}

#[test]
fn invalid_references_are_rejected() {
    let mut s = session(bottling());
    let before = s.project().clone();
    for cmd in [
        Command::SetBinding {
            node: nid("n-ov-auto"),
            binding: serde_json::from_value(json!({ "property": "checked", "source": { "tag": "tag-nope" }, "mode": "readWrite" })).unwrap(),
        },
        Command::SetNodeStyle { node: nid("n-ov-title"), style: Some(StyleId::new("style-nope").unwrap()) },
        Command::DeleteScreen { screen: sid("pop-settings") },
        Command::DeleteComponentDefinition { definition: ComponentDefinitionId::new("cmp-kpi").unwrap() },
        Command::SetNavigation { navigation: Navigation { start_screen: sid("scr-nope"), items: vec![] } },
    ] {
        let op = cmd.op_name();
        let err = s.execute(Transaction::single(cmd)).unwrap_err();
        let TransactionError::Validation { errors } = err else { panic!("{op}: {err:?}") };
        assert!(errors.iter().any(|d| d.code == DiagnosticCode::UnknownReference), "{op}: {errors:?}");
        assert_untouched(&s, &before, 0, 0);
    }
}

#[test]
fn circular_hierarchies_are_rejected() {
    let mut s = session(bottling());
    let before = s.project().clone();
    // Node into its own descendant.
    let err = s
        .execute(Transaction::single(Command::MoveNode {
            node: nid("n-ov-main"),
            parent: nid("n-ov-speed-row"),
            index: None,
        }))
        .unwrap_err();
    assert!(
        matches!(
            err,
            TransactionError::Command {
                error: CommandError::CircularHierarchy { .. },
                ..
            }
        ),
        "{err:?}"
    );
    // Node into itself.
    let err = s
        .execute(Transaction::single(Command::MoveNode {
            node: nid("n-ov-kpis"),
            parent: nid("n-ov-kpis"),
            index: None,
        }))
        .unwrap_err();
    assert!(matches!(
        err,
        TransactionError::Command {
            error: CommandError::CircularHierarchy { .. },
            ..
        }
    ));
    // Component definition instantiating itself (via an instance node).
    let err = s
        .execute(Transaction::single(Command::InsertNode {
            parent: nid("n-kpi-col"),
            index: None,
            node: serde_json::from_value(json!({ "id": "n-kpi-self", "element": { "type": "instance", "definition": "cmp-kpi", "parameters": { "label": "x" } } })).unwrap(),
            assign_new_ids: false,
        }))
        .unwrap_err();
    let TransactionError::Validation { errors } = err else {
        panic!("{err:?}")
    };
    assert!(errors
        .iter()
        .any(|d| d.code == DiagnosticCode::CircularComponentReference));
    // Indirect cycle: tank instantiates kpi... and kpi instantiates tank.
    let err = s
        .execute(Transaction::new(vec![
            Command::InsertNode { parent: nid("n-tk-root"), index: None, node: serde_json::from_value(json!({ "id": "n-x1", "element": { "type": "instance", "definition": "cmp-kpi", "parameters": { "label": "x" } } })).unwrap(), assign_new_ids: false },
            Command::InsertNode { parent: nid("n-kpi-col"), index: None, node: serde_json::from_value(json!({ "id": "n-x2", "element": { "type": "instance", "definition": "cmp-tank", "parameters": { "tank": "tag-tank-1" } } })).unwrap(), assign_new_ids: false },
        ]))
        .unwrap_err();
    let TransactionError::Validation { errors } = err else {
        panic!("{err:?}")
    };
    assert!(errors
        .iter()
        .any(|d| d.code == DiagnosticCode::CircularComponentReference));
    // Roots cannot move.
    let err = s
        .execute(Transaction::single(Command::MoveNode {
            node: nid("n-ov-root"),
            parent: nid("n-fl-col"),
            index: None,
        }))
        .unwrap_err();
    assert!(matches!(
        err,
        TransactionError::Command {
            error: CommandError::RootNode { .. },
            ..
        }
    ));
    assert_untouched(&s, &before, 0, 0);
}

#[test]
fn stale_revisions_are_rejected() {
    let mut s = session(minimal());
    let r0 = s.revision();
    run(
        &mut s,
        vec![Command::RenameScreen {
            screen: sid("scr-main"),
            name: "A".into(),
        }],
    );
    let before = s.project().clone();
    let err = s
        .execute(
            Transaction::single(Command::RenameScreen {
                screen: sid("scr-main"),
                name: "B".into(),
            })
            .at_revision(r0),
        )
        .unwrap_err();
    assert_eq!(
        err,
        TransactionError::StaleRevision {
            expected: r0,
            actual: r0 + 1
        }
    );
    assert_untouched(&s, &before, 1, 1);
    assert!(matches!(
        s.undo(Some(r0)),
        Err(TransactionError::StaleRevision { .. })
    ));
    assert!(matches!(
        s.redo(Some(99)),
        Err(TransactionError::StaleRevision { .. })
    ));
    // Correct revision succeeds.
    let o = s
        .execute(
            Transaction::single(Command::RenameScreen {
                screen: sid("scr-main"),
                name: "B".into(),
            })
            .at_revision(1),
        )
        .unwrap();
    assert_eq!((o.previous_revision, o.revision), (1, 2));
}

#[test]
fn empty_and_no_op_transactions() {
    let mut s = session(minimal());
    assert_eq!(
        s.execute(Transaction::default()).unwrap_err(),
        TransactionError::Empty
    );
    let o = s
        .execute(Transaction::single(Command::RenameScreen {
            screen: sid("scr-main"),
            name: "Main".into(),
        }))
        .unwrap();
    assert!(!o.changed);
    assert_eq!(o.revision, 0);
    assert_eq!(s.undo_depth(), 0);
    assert!(!s.is_dirty());
}

#[test]
fn reject_new_errors_allows_incremental_repair_of_invalid_projects() {
    let mut p = minimal();
    p.navigation.start_screen = sid("scr-gone");
    let mut s = session(p);
    assert!(s.report().has_errors());
    // Unrelated edit on an invalid project: allowed (no *new* errors).
    run(
        &mut s,
        vec![Command::RenameScreen {
            screen: sid("scr-main"),
            name: "X".into(),
        }],
    );
    // Strict policy refuses because the project is still invalid.
    let mut strict = Transaction::single(Command::RenameScreen {
        screen: sid("scr-main"),
        name: "Y".into(),
    });
    strict.validation = ValidationPolicy::Strict;
    assert!(matches!(
        s.execute(strict),
        Err(TransactionError::Validation { .. })
    ));
    // A new error is refused under the default policy.
    let err = s
        .execute(Transaction::single(Command::SetProperty {
            node: nid("n-hello"),
            name: "maxLines".into(),
            value: Some(json!(0)),
        }))
        .unwrap_err();
    assert!(matches!(err, TransactionError::Validation { .. }));
    // Repair.
    let o = run(
        &mut s,
        vec![Command::SetNavigation {
            navigation: Navigation {
                start_screen: sid("scr-main"),
                items: vec![],
            },
        }],
    );
    assert!(o.diagnostics.is_empty());
    assert!(!s.report().has_errors());
}

#[test]
fn execution_is_deterministic() {
    let script = || {
        vec![
            Transaction::single(Command::CreateScreen {
                id: None,
                name: "S".into(),
                kind: ScreenKind::Page,
                root: None,
                index: None,
            }),
            Transaction::single(Command::InsertNode {
                parent: nid("n-root"),
                index: Some(0),
                node: text_node("tmp", "t"),
                assign_new_ids: true,
            }),
            Transaction::single(Command::DuplicateNode {
                node: nid("n-hello"),
                parent: None,
                index: None,
            }),
            Transaction::single(Command::DuplicateScreen {
                screen: sid("scr-main"),
                name: None,
            }),
            Transaction::single(Command::SetExtension {
                key: "a.b".into(),
                value: Some(json!({ "z": 1, "a": [1, 2] })),
            }),
        ]
    };
    let mut outputs = Vec::new();
    for _ in 0..3 {
        let mut s = session(minimal());
        let mut outcomes = Vec::new();
        for t in script() {
            outcomes.push(serde_json::to_string(&s.execute(t).unwrap()).unwrap());
        }
        s.undo(None).unwrap();
        s.redo(None).unwrap();
        outputs.push((canonical(s.project()), outcomes));
    }
    assert_eq!(outputs[0], outputs[1]);
    assert_eq!(outputs[1], outputs[2]);
    assert!(outputs[0].0.contains("scr-1"));
}

#[test]
fn commands_round_trip_as_json() {
    let t = Transaction::new(vec![
        Command::MoveNode {
            node: nid("a"),
            parent: nid("b"),
            index: Some(2),
        },
        Command::SetProperty {
            node: nid("a"),
            name: "text".into(),
            value: None,
        },
        Command::CreateScreen {
            id: None,
            name: "S".into(),
            kind: ScreenKind::Popup,
            root: None,
            index: None,
        },
    ])
    .at_revision(7)
    .labeled("Move")
    .merging("drag-1");
    let text = serde_json::to_string(&t).unwrap();
    assert!(text.contains(r#""op":"moveNode""#), "{text}");
    let back: Transaction = serde_json::from_str(&text).unwrap();
    assert_eq!(back, t);
    // Unknown fields and unknown ops are rejected.
    assert!(
        serde_json::from_str::<Command>(r#"{"op":"removeNode","node":"a","force":true}"#).is_err()
    );
    assert!(serde_json::from_str::<Command>(r#"{"op":"exec","cmd":"rm -rf /"}"#).is_err());
}
