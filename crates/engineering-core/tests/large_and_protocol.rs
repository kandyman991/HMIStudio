//! Large nested projects and the frontend protocol contract.
#![allow(clippy::unwrap_used)]

mod common;

use std::time::Instant;

use common::*;
use hmi_engineering_core::protocol::{dispatch, Request, Response, PROTOCOL_VERSION};
use hmi_engineering_core::{Command, ProjectSession, SaveOptions, SessionConfig, Transaction};
use hmi_project_model::model::*;
use hmi_project_model::MAX_NODE_DEPTH;
use serde_json::json;

fn large_project() -> Project {
    let mut p = minimal();
    p.screens.clear();
    for i in 0..40 {
        let root = deep_tree(&format!("s{i}"), MAX_NODE_DEPTH, 4);
        p.screens.push(Screen {
            id: sid(&format!("scr-{i}")),
            name: format!("Screen {i}"),
            kind: ScreenKind::Page,
            title: None,
            design_profile: None,
            events: vec![],
            root,
            extensions: Extensions::new(),
        });
    }
    p.navigation.start_screen = sid("scr-0");
    p
}

#[test]
fn large_nested_projects_edit_undo_and_persist() {
    let p = large_project();
    let nodes: usize = p
        .screens
        .iter()
        .map(|s| {
            let mut v = vec![];
            hmi_engineering_core::ids::collect_node_ids(&s.root, &mut v);
            v.len()
        })
        .sum();
    assert!(nodes > 5_000, "{nodes}");
    let mut s = session(p);
    assert!(s.report().is_valid(), "{}", s.report());
    let start = canonical(s.project());
    let t0 = Instant::now();
    for i in 0..30 {
        let deepest = format!("s{i}-d{}", MAX_NODE_DEPTH - 1);
        run(
            &mut s,
            vec![
                Command::InsertNode {
                    parent: nid(&deepest),
                    index: Some(0),
                    node: text_node("leaf", "deep"),
                    assign_new_ids: true,
                },
                Command::MoveNode {
                    node: nid(&format!("s{i}-d3-t0")),
                    parent: nid(&format!("s{}-d2", i + 1)),
                    index: None,
                },
                Command::DuplicateNode {
                    node: nid(&format!("s{i}-d10")),
                    parent: Some(nid(&format!("s{i}-d1"))),
                    index: None,
                },
            ],
        );
    }
    // Nesting beyond the limit is refused.
    let e = s.execute(Transaction::single(Command::InsertNode {
        parent: nid(&format!("s39-d{MAX_NODE_DEPTH}")),
        index: None,
        node: text_node("too-deep", "x"),
        assign_new_ids: false,
    }));
    let Err(hmi_engineering_core::TransactionError::Validation { errors }) = e else {
        panic!("{e:?}")
    };
    assert!(errors
        .iter()
        .any(|d| d.code == hmi_project_model::DiagnosticCode::NestingTooDeep));
    for _ in 0..30 {
        s.undo(None).unwrap();
    }
    assert_eq!(canonical(s.project()), start);
    for _ in 0..30 {
        s.redo(None).unwrap();
    }
    let elapsed = t0.elapsed();
    // Generous bound: catches accidental quadratic blow-ups, not a benchmark.
    assert!(elapsed.as_secs() < 60, "{elapsed:?}");
    let dir = TempDir::new("large");
    let path = dir.join("large.hmi.json");
    s.save_as(&path, SaveOptions::default()).unwrap();
    let o = ProjectSession::open(&path, SessionConfig::default(), std_fs(), &registry()).unwrap();
    assert_eq!(o.session.project(), s.project());
}

#[test]
fn protocol_requests_parse_from_json() {
    let r: Request = serde_json::from_value(json!({
        "request": "execute",
        "transaction": {
            "expectedRevision": 0,
            "label": "Rename",
            "commands": [ { "op": "renameScreen", "screen": "scr-main", "name": "X" } ]
        }
    }))
    .unwrap();
    assert!(matches!(r, Request::Execute { .. }));
    for bad in [
        json!({ "request": "shell", "cmd": "ls" }),
        json!({ "request": "readFile", "path": "/etc/passwd" }),
        json!({ "request": "snapshot", "extra": 1 }),
    ] {
        assert!(serde_json::from_value::<Request>(bad).is_err());
    }
}

#[test]
fn protocol_dispatch_flow() {
    let reg = registry();
    let dir = TempDir::new("proto");
    let mut s = session(minimal());

    let (resp, note) = dispatch(&mut s, Request::Snapshot {}, &reg);
    let Response::Snapshot { snapshot } = resp else {
        panic!()
    };
    assert_eq!(snapshot.revision, 0);
    assert!(note.is_none());

    let exec = |name: &str, rev: Option<u64>| Request::Execute {
        transaction: Transaction {
            expected_revision: rev,
            ..Transaction::single(Command::RenameScreen {
                screen: sid("scr-main"),
                name: name.into(),
            })
        },
    };
    let (resp, note) = dispatch(&mut s, exec("A", Some(0)), &reg);
    assert!(matches!(resp, Response::Changed { .. }));
    let note = note.unwrap();
    assert_eq!(note.protocol, PROTOCOL_VERSION);
    assert_eq!(note.outcome.revision, 1);
    assert!(note.outcome.status.dirty && note.outcome.status.can_undo);

    // Stale revision -> structured error, no notification.
    let (resp, note) = dispatch(&mut s, exec("B", Some(0)), &reg);
    let Response::Error { error } = resp else {
        panic!()
    };
    assert_eq!(error.code, "transaction");
    assert_eq!(error.details["kind"], "staleRevision");
    assert!(note.is_none());

    // Validation failure details carry diagnostics.
    let (resp, _) = dispatch(
        &mut s,
        Request::Execute {
            transaction: Transaction::single(Command::DeleteScreen {
                screen: sid("scr-main"),
            }),
        },
        &reg,
    );
    let Response::Error { error } = resp else {
        panic!()
    };
    assert_eq!(error.details["kind"], "validation");
    assert!(error.details["errors"][0]["code"].is_string());

    let (resp, note) = dispatch(
        &mut s,
        Request::Undo {
            expected_revision: Some(1),
        },
        &reg,
    );
    assert!(matches!(resp, Response::Changed { .. }) && note.is_some());
    let (resp, note) = dispatch(
        &mut s,
        Request::Undo {
            expected_revision: None,
        },
        &reg,
    );
    assert!(matches!(resp, Response::Nothing { .. }) && note.is_none());
    dispatch(
        &mut s,
        Request::Redo {
            expected_revision: None,
        },
        &reg,
    );

    let (resp, _) = dispatch(
        &mut s,
        Request::Save {
            options: SaveOptions::default(),
        },
        &reg,
    );
    let Response::Error { error } = resp else {
        panic!()
    };
    assert_eq!(error.details["kind"], "noPath");
    let (resp, _) = dispatch(
        &mut s,
        Request::SaveAs {
            path: dir.join("p.hmi.json"),
            options: SaveOptions::default(),
        },
        &reg,
    );
    let Response::Saved { status, .. } = resp else {
        panic!()
    };
    assert!(!status.dirty);
    let (resp, _) = dispatch(&mut s, Request::CheckExternal {}, &reg);
    assert!(matches!(
        resp,
        Response::External {
            external: hmi_engineering_core::ExternalStatus::Unchanged
        }
    ));
    let (resp, _) = dispatch(&mut s, Request::Autosave {}, &reg);
    assert!(matches!(resp, Response::Autosaved { .. }));
    let (resp, _) = dispatch(&mut s, Request::Close { force: false }, &reg);
    assert!(matches!(resp, Response::Closed {}));
    let (resp, _) = dispatch(&mut s, exec("C", None), &reg);
    assert!(matches!(resp, Response::Error { .. }));

    // Every response serializes to JSON for IPC.
    let v = serde_json::to_value(&Response::Status { status: s.status() }).unwrap();
    assert_eq!(v["response"], "status");
    assert_eq!(v["status"]["closed"], true);
}
