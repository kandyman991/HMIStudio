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
    assert!(s
        .execute(Transaction::single(Command::DuplicateScreen {
            screen: sid("scr-main"),
            name: None
        }))
        .is_err());
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
