#![allow(clippy::unwrap_used)]
mod common;

use common::{std_fs, TempDir};
use hmi_engineering_core::fs::{Fault, FaultyFileSystem};
use hmi_engineering_core::legacy_quarantine::{requires_review, QuarantinedImport};
use hmi_engineering_core::persistence::{fingerprint, sidecar_dir, OpenError, ReloadError};
use hmi_engineering_core::{
    Command, ProjectSession, Request, Response, SaveOptions, SessionConfig, Transaction,
};
use hmi_project_model::{load_project, save_project, MigrationRegistry};
use serde_json::json;
use std::sync::Arc;

fn source() -> Vec<u8> {
    serde_json::to_vec(&json!({"schemaVersion":1,"metadata":{"name":"Synthetic","defaultScreenId":"screen/a","targetProfiles":[]},"screens":{"screen/a":{"id":"screen/a","title":"Main","widthDp":640,"heightDp":480,"components":{"label-a":{"id":"label-a","type":"label","properties":{"text":"Synthetic"}}}}},"variables":{},"externalSources":{},"externalMappings":{}})).unwrap()
}

fn ordinary_project() -> hmi_project_model::model::Project {
    load_project(include_str!(
        "../../project-model/fixtures/valid/minimal.hmi.json"
    ))
    .unwrap()
    .project
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
    let before = review.snapshot();
    assert!(requires_review(&before.project));
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
    for request in [
        Request::Save {
            options: SaveOptions::default(),
        },
        Request::SaveAs {
            path: dir.join("plain.hmi.json"),
            options: SaveOptions::default(),
        },
        Request::Autosave {},
        Request::Reload { force: true },
        Request::Close { force: true },
    ] {
        assert!(
            matches!(review.request(request), Response::Error { error } if error.code == "importReviewRequired")
        );
        assert_eq!(review.snapshot(), before);
    }
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
    #[cfg(unix)]
    for bundle in [&path, &edited] {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(bundle).unwrap().permissions().mode() & 0o077,
            0
        );
    }
    let reopened = QuarantinedImport::reopen(&edited, SessionConfig::default(), std_fs()).unwrap();
    assert_eq!(reopened.snapshot().project.screens[0].name, "Edited");
    assert_eq!(reopened.report(), review.report());
    assert!(requires_review(&reopened.snapshot().project));
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

#[test]
fn failed_save_copy_preserves_session_and_previous_bundle() {
    let dir = TempDir::new("review-save-fault");
    let path = dir.join("review.bundle");
    let target = dir.join("failed.bundle");
    let fs = Arc::new(FaultyFileSystem::new(
        Fault::RenameFails,
        "failed.bundle",
        0,
    ));
    let mut review =
        QuarantinedImport::import_new(&source(), &path, SessionConfig::default(), fs.clone())
            .unwrap();
    let original = std::fs::read(&path).unwrap();
    let screen = review.snapshot().project.screens[0].id.clone();
    review
        .execute(Transaction::single(Command::RenameScreen {
            screen,
            name: "Unsaved edit".into(),
        }))
        .unwrap();
    let before = review.snapshot();
    assert!(review.save_copy(&target).is_err());
    assert!(fs.fired());
    assert_eq!(review.snapshot(), before);
    assert_eq!(review.path(), path.as_path());
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert!(!target.exists());
    assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 1);
}

#[test]
fn catalog_provenance_alone_refuses_production_and_plain_save() {
    let dir = TempDir::new("catalog-marker");
    let mut project = ordinary_project();
    assert!(!requires_review(&project));
    project
        .extensions
        .insert("hmiforge.standardCatalog".into(), json!(null));
    assert!(requires_review(&project));
    let mut session = ProjectSession::new(project, SessionConfig::default());
    assert!(session
        .save_as(&dir.join("plain.json"), SaveOptions::default())
        .is_err());
}

#[test]
fn save_copy_seals_the_undo_step_and_undo_returns_to_saved_state() {
    let dir = TempDir::new("review-history");
    let mut review = QuarantinedImport::import_new(
        &source(),
        &dir.join("first.bundle"),
        SessionConfig::default(),
        std_fs(),
    )
    .unwrap();
    let screen = review.snapshot().project.screens[0].id.clone();
    let rename = |name: &str| {
        Transaction::single(Command::RenameScreen {
            screen: screen.clone(),
            name: name.into(),
        })
        .merging("screen-name")
    };
    review.execute(rename("Saved edit")).unwrap();
    review.save_copy(&dir.join("saved.bundle")).unwrap();
    assert!(!review.is_dirty());
    review.execute(rename("Unsaved edit")).unwrap();
    assert!(matches!(
        review.request(Request::Undo {
            expected_revision: Some(2)
        }),
        Response::Changed { .. }
    ));
    assert_eq!(review.snapshot().project.screens[0].name, "Saved edit");
    assert!(!review.is_dirty());
}

#[test]
fn ordinary_storage_refuses_a_review_base_or_payload_without_mutating_either() {
    let dir = TempDir::new("review-alternate-storage");
    let path = dir.join("plain.hmi.json");
    let migrations = MigrationRegistry::builtin();
    let mut session = ProjectSession::new(ordinary_project(), SessionConfig::default());
    session.save_as(&path, SaveOptions::default()).unwrap();
    let original = std::fs::read(&path).unwrap();
    let screen = session.project().screens[0].id.clone();
    session
        .execute(Transaction::single(Command::RenameScreen {
            screen,
            name: "Pending edit".into(),
        }))
        .unwrap();
    session.autosave().unwrap();
    let envelope = sidecar_dir(&path).join("autosave.meta.json");
    let recovery = std::fs::read(&envelope).unwrap();
    let before = session.snapshot();

    let mut marked = ordinary_project();
    marked
        .extensions
        .insert("hmiforge.standardCatalog".into(), json!(null));
    let marked = save_project(&marked).unwrap();
    let review_path = dir.join("review.bundle");
    QuarantinedImport::import_new(&source(), &review_path, SessionConfig::default(), std_fs())
        .unwrap();
    for base in [
        marked.as_bytes().to_vec(),
        std::fs::read(&review_path).unwrap(),
    ] {
        std::fs::write(&path, &base).unwrap();
        assert!(matches!(
            ProjectSession::open(&path, SessionConfig::default(), std_fs(), &migrations),
            Err(OpenError::ReviewRequired)
        ));
        assert!(matches!(
            ProjectSession::recover(&path, SessionConfig::default(), std_fs(), &migrations),
            Err(OpenError::ReviewRequired)
        ));
        assert!(matches!(
            session.reload(true, &migrations),
            Err(ReloadError::Open(OpenError::ReviewRequired))
        ));
        assert!(session
            .save(SaveOptions {
                overwrite_external_changes: true,
                ..SaveOptions::default()
            })
            .is_err());
        assert_eq!(session.snapshot(), before);
        assert_eq!(std::fs::read(&path).unwrap(), base);
        assert_eq!(std::fs::read(&envelope).unwrap(), recovery);
    }

    std::fs::write(&path, original).unwrap();
    let mut marked_recovery: serde_json::Value = serde_json::from_slice(&recovery).unwrap();
    marked_recovery["projectFingerprint"] = json!(fingerprint(marked.as_bytes()));
    marked_recovery["project"] = json!(marked);
    let marked_recovery = serde_json::to_vec(&marked_recovery).unwrap();
    std::fs::write(&envelope, &marked_recovery).unwrap();
    assert!(matches!(
        ProjectSession::recover(&path, SessionConfig::default(), std_fs(), &migrations),
        Err(OpenError::ReviewRequired)
    ));
    assert!(matches!(
        ProjectSession::recover_untitled(
            SessionConfig {
                untitled_recovery_dir: Some(sidecar_dir(&path)),
                ..SessionConfig::default()
            },
            std_fs(),
            &migrations,
        ),
        Err(OpenError::ReviewRequired)
    ));
    assert_eq!(std::fs::read(&envelope).unwrap(), marked_recovery);
}

#[test]
fn bounded_bundle_read_refuses_oversize() {
    let dir = TempDir::new("oversize-review");
    let path = dir.join("oversize.bundle");
    std::fs::File::create(&path)
        .unwrap()
        .set_len(64 * 1024 * 1024 + 1)
        .unwrap();
    assert!(matches!(
        QuarantinedImport::reopen(&path, SessionConfig::default(), std_fs()),
        Err(hmi_engineering_core::legacy_quarantine::ImportError::SizeLimit)
    ));
}

#[test]
fn recomputed_checksum_cannot_hide_incomplete_report() {
    use sha2::{Digest, Sha256};

    // Match the persisted payload field order when recomputing its checksum.
    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct ChecksumPayload<'a> {
        source: &'a serde_json::Value,
        original_project: &'a serde_json::Value,
        project: &'a serde_json::Value,
        report: &'a serde_json::Value,
        resources: &'a serde_json::Value,
    }

    let dir = TempDir::new("review-replay");
    let path = dir.join("review.bundle");
    QuarantinedImport::import_new(&source(), &path, SessionConfig::default(), std_fs()).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let mut envelope: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let report = &mut envelope["payload"]["report"];
    let findings = report["findings"].as_array_mut().unwrap();
    assert!(!findings.is_empty());
    findings.pop();
    let count = findings.len();
    report["sourceLeafCount"] = json!(count);
    let payload = &envelope["payload"];
    let body = serde_json::to_vec(&ChecksumPayload {
        source: &payload["source"],
        original_project: &payload["originalProject"],
        project: &payload["project"],
        report: &payload["report"],
        resources: &payload["resources"],
    })
    .unwrap();
    envelope["sha256"] = json!(format!("{:x}", Sha256::digest(&body)));
    std::fs::write(&path, serde_json::to_vec(&envelope).unwrap()).unwrap();
    assert!(QuarantinedImport::reopen(&path, SessionConfig::default(), std_fs()).is_err());
}
