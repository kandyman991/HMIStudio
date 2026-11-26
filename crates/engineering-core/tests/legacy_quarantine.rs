#![allow(clippy::unwrap_used)]
mod common;

use common::{std_fs, TempDir};
use hmi_engineering_core::fs::{Fault, FaultyFileSystem};
use hmi_engineering_core::legacy_quarantine::{check_production_admission, QuarantinedImport};
use hmi_engineering_core::{Command, ProjectSession, SaveOptions, SessionConfig, Transaction};
use serde_json::json;
use std::sync::Arc;

fn source() -> Vec<u8> {
    serde_json::to_vec(&json!({"schemaVersion":1,"metadata":{"name":"Synthetic","defaultScreenId":"screen/a","targetProfiles":[]},"screens":{"screen/a":{"id":"screen/a","title":"Main","widthDp":640,"heightDp":480,"components":{"label-a":{"id":"label-a","type":"label","properties":{"text":"Synthetic"}}}}},"variables":{},"externalSources":{},"externalMappings":{}})).unwrap()
}

#[test]
fn review_bundle_round_trip_and_no_plain_save() {
    let dir = TempDir::new("review");
    let original = dir.join("original.json");
    let path = dir.join("review.bundle");
    let edited = dir.join("edited.bundle");
    let bytes = source();
    std::fs::write(&original, &bytes).unwrap();
    let mut review =
        QuarantinedImport::import_new(&bytes, &path, SessionConfig::default(), std_fs()).unwrap();
    assert_eq!(std::fs::read(&original).unwrap(), bytes);
    assert!(review.check_production_admission().is_err());
    let before = review.snapshot();
    assert!(check_production_admission(&before.project).is_err());
    assert!(review
        .execute(Transaction::single(Command::SetExtension {
            key: "hmistudio.importQuarantine".into(),
            value: None
        }))
        .is_err());
    assert!(review
        .execute(Transaction::single(Command::SetExtension {
            key: "hmiforge.idMap".into(),
            value: None
        }))
        .is_err());
    assert_eq!(review.snapshot(), before);
    let mut raw = ProjectSession::new(before.project.clone(), SessionConfig::default());
    assert!(raw
        .save_as(&dir.join("plain.hmi.json"), SaveOptions::default())
        .is_err());
    let screen = before.project.screens[0].id.clone();
    review
        .execute(Transaction::single(Command::RenameScreen {
            screen,
            name: "Edited".into(),
        }))
        .unwrap();
    assert!(review.save_copy(&original).is_err());
    assert_eq!(std::fs::read(&original).unwrap(), bytes);
    review.save_copy(&edited).unwrap();
    let reopened = QuarantinedImport::reopen(&edited, SessionConfig::default(), std_fs()).unwrap();
    assert_eq!(reopened.snapshot().project.screens[0].name, "Edited");
    assert_eq!(reopened.report(), review.report());
    assert!(reopened.check_production_admission().is_err());
    assert!(hmi_project_model::load_project(&std::fs::read_to_string(&edited).unwrap()).is_err());
}

#[test]
fn refused_import_and_persistence_faults_do_not_publish() {
    let dir = TempDir::new("review-fault");
    let path = dir.join("review.bundle");
    assert!(
        QuarantinedImport::import_new(b"{}", &path, SessionConfig::default(), std_fs()).is_err()
    );
    for fault in [
        Fault::WriteFails,
        Fault::WriteInterrupted,
        Fault::WriteTruncatedSilently,
        Fault::RenameFails,
    ] {
        let fs = Arc::new(FaultyFileSystem::new(fault, "review.bundle", 0));
        assert!(QuarantinedImport::import_new(
            &source(),
            &path,
            SessionConfig::default(),
            fs.clone()
        )
        .is_err());
        assert!(fs.fired());
        assert!(!path.exists());
        assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 0);
    }
}

#[test]
fn corrupt_bundle_refused_and_existing_target_unchanged() {
    let dir = TempDir::new("review-corrupt");
    let path = dir.join("review.bundle");
    QuarantinedImport::import_new(&source(), &path, SessionConfig::default(), std_fs()).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    assert!(
        QuarantinedImport::import_new(&source(), &path, SessionConfig::default(), std_fs())
            .is_err()
    );
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    let mut tampered: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    tampered["payload"]["report"]["sourceSha256"] = json!("modified");
    std::fs::write(&path, serde_json::to_vec(&tampered).unwrap()).unwrap();
    assert!(QuarantinedImport::reopen(&path, SessionConfig::default(), std_fs()).is_err());
}
