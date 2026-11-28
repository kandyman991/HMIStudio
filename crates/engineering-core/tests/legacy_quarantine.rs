#![allow(clippy::unwrap_used)]
mod common;

use common::{std_fs, TempDir};
use hmi_engineering_core::fs::{Fault, FaultyFileSystem};
use hmi_engineering_core::legacy_quarantine::{
    requires_review, review_recovery_path, QuarantinedImport, RevisionSource,
};
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
fn refused_import_diagnostics_are_redacted_and_distinct() {
    use hmi_engineering_core::legacy_quarantine::{ImportError, PreparedReview};

    let message = |result: Result<PreparedReview, ImportError>| match result {
        Err(error) => error.to_string(),
        Ok(_) => panic!("Invalid import was admitted"),
    };
    let malformed = message(QuarantinedImport::prepare_portable(b"{"));
    assert!(malformed.contains("invalid-or-duplicate-json"));
    let future = br#"{"schemaVersion":11,"private-key-sentinel":"secret-value-sentinel"}"#;
    let version = message(QuarantinedImport::prepare_portable(future));
    assert!(version.contains("unsupported-or-missing-version"));
    assert_eq!(
        version,
        message(QuarantinedImport::prepare_portable(future))
    );
    assert!(!version.contains("private-key-sentinel"));
    assert!(!version.contains("secret-value-sentinel"));
    let package = message(QuarantinedImport::prepare_package(b"not a package"));
    assert!(package.contains("InvalidArchive"));
    assert_ne!(package, malformed);
    assert!(!package.contains("not a package"));
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

#[test]
fn historical_asset_bundles_reopen_and_keep_their_original_evidence() {
    for bytes in [
        include_bytes!("fixtures/asset-size-4d1da3b.json").as_slice(),
        include_bytes!("fixtures/asset-size-1ea025c.json").as_slice(),
    ] {
        let dir = TempDir::new("asset-size-replay");
        let old = dir.join("prior.bundle");
        let copy = dir.join("saved.bundle");
        std::fs::write(&old, bytes).unwrap();
        let mut review =
            QuarantinedImport::reopen(&old, SessionConfig::default(), std_fs()).unwrap();
        assert_eq!(review.report()["importerVersion"], "0.1.0");
        review.save_copy(&copy).unwrap();
        let saved = QuarantinedImport::reopen(&copy, SessionConfig::default(), std_fs()).unwrap();
        assert!(requires_review(&saved.snapshot().project));
        assert_eq!(review.report(), saved.report());
        assert_eq!(std::fs::read(&old).unwrap(), bytes);
    }
}

fn rename_review(review: &mut QuarantinedImport, name: &str) {
    let screen = review.snapshot().project.screens[0].id.clone();
    review
        .execute(Transaction::single(Command::RenameScreen {
            screen,
            name: name.into(),
        }))
        .unwrap();
}

#[test]
fn review_recovery_restores_dirty_edits_and_retires_on_save_copy() {
    let dir = TempDir::new("review-recovery");
    let path = dir.join("review.bundle");
    let copy = dir.join("copy.bundle");
    let mut review =
        QuarantinedImport::import_new(&source(), &path, SessionConfig::default(), std_fs())
            .unwrap();
    let original = std::fs::read(&path).unwrap();
    let report = review.report().clone();
    rename_review(&mut review, "Unsaved");
    assert!(!review.snapshot().status.autosave_current);
    review.autosave().unwrap();
    assert!(review.snapshot().status.autosave_current);
    let slot = review_recovery_path(&path);
    assert!(slot.exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&slot).unwrap().permissions().mode() & 0o077,
            0
        );
    }
    drop(review);
    let mut recovered =
        QuarantinedImport::recover(&path, SessionConfig::default(), std_fs()).unwrap();
    assert!(recovered.is_dirty());
    assert!(recovered.snapshot().status.autosave_current);
    assert!(requires_review(&recovered.snapshot().project));
    assert_eq!(recovered.snapshot().project.screens[0].name, "Unsaved");
    assert_eq!(recovered.report(), &report);
    assert!(ProjectSession::open(
        &slot,
        SessionConfig::default(),
        std_fs(),
        &MigrationRegistry::builtin()
    )
    .is_err());
    recovered.save_copy(&copy).unwrap();
    assert!(!slot.exists());
    assert!(!recovered.is_dirty());
    assert_eq!(std::fs::read(&path).unwrap(), original);
    let reopened = QuarantinedImport::reopen(&copy, SessionConfig::default(), std_fs()).unwrap();
    assert_eq!(reopened.snapshot().project.screens[0].name, "Unsaved");
    assert!(requires_review(&reopened.snapshot().project));
}

#[test]
fn review_recovery_faults_preserve_previous_snapshot_and_undo_retires_it() {
    for fault in [
        Fault::WriteFails,
        Fault::WriteInterrupted,
        Fault::WriteTruncatedSilently,
        Fault::RenameFails,
    ] {
        let dir = TempDir::new("review-recovery-fault");
        let path = dir.join("review.bundle");
        let slot = review_recovery_path(&path);
        let fs = Arc::new(FaultyFileSystem::new(fault, "review-recovery.json", 1));
        let mut review =
            QuarantinedImport::import_new(&source(), &path, SessionConfig::default(), fs.clone())
                .unwrap();
        rename_review(&mut review, "First");
        review.autosave().unwrap();
        let previous = std::fs::read(&slot).unwrap();
        rename_review(&mut review, "Second");
        assert!(review.autosave().is_err());
        assert!(fs.fired());
        assert_eq!(std::fs::read(&slot).unwrap(), previous);
        assert_eq!(
            QuarantinedImport::recover(&path, SessionConfig::default(), std_fs())
                .unwrap()
                .snapshot()
                .project
                .screens[0]
                .name,
            "First"
        );
        for _ in 0..2 {
            let _ = review.request(Request::Undo {
                expected_revision: Some(review.snapshot().revision),
            });
        }
        assert!(!review.is_dirty());
        review.autosave().unwrap();
        assert!(!slot.exists());
        let _ = review.request(Request::Redo {
            expected_revision: Some(review.snapshot().revision),
        });
        review.autosave().unwrap();
        assert!(slot.exists());
    }
}

#[test]
fn review_recovery_refuses_foreign_base_corruption_and_unresolved_slot() {
    let dir = TempDir::new("review-recovery-refusal");
    let path = dir.join("review.bundle");
    let copy = dir.join("copy.bundle");
    let mut review =
        QuarantinedImport::import_new(&source(), &path, SessionConfig::default(), std_fs())
            .unwrap();
    rename_review(&mut review, "Pending");
    review.autosave().unwrap();
    let slot = review_recovery_path(&path);
    let bytes = std::fs::read(&slot).unwrap();
    let mut reopened =
        QuarantinedImport::reopen(&path, SessionConfig::default(), std_fs()).unwrap();
    rename_review(&mut reopened, "Other");
    assert!(reopened.autosave().is_err());
    assert_eq!(std::fs::read(&slot).unwrap(), bytes);
    reopened.save_copy(&copy).unwrap();
    let other_slot = review_recovery_path(&copy);
    std::fs::create_dir_all(other_slot.parent().unwrap()).unwrap();
    std::fs::write(&other_slot, &bytes).unwrap();
    assert!(QuarantinedImport::recover(&copy, SessionConfig::default(), std_fs()).is_err());
    std::fs::write(&slot, b"broken recovery").unwrap();
    assert!(QuarantinedImport::recover(&path, SessionConfig::default(), std_fs()).is_err());
    assert_eq!(std::fs::read(&slot).unwrap(), b"broken recovery");
    assert!(review.autosave().is_err());
    assert!(!review.snapshot().status.autosave_current);
}

#[test]
fn historical_replay_refuses_unknown_or_mixed_revision() {
    use sha2::{Digest, Sha256};
    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct ChecksumPayload<'a> {
        source: &'a serde_json::Value,
        original_project: &'a serde_json::Value,
        project: &'a serde_json::Value,
        report: &'a serde_json::Value,
        resources: &'a serde_json::Value,
    }
    let dir = TempDir::new("historical-revision-refusal");
    let path = dir.join("review.bundle");
    // This historical representation has no numeric size destination, so
    // relabeling it as 0.1.1 cannot agree with the corrected mapping.
    for version in ["99.0.0", "0.1.1"] {
        let mut bundle: serde_json::Value =
            serde_json::from_slice(include_bytes!("fixtures/asset-size-4d1da3b.json")).unwrap();
        bundle["payload"]["report"]["importerVersion"] = json!(version);
        let payload = &bundle["payload"];
        let body = serde_json::to_vec(&ChecksumPayload {
            source: &payload["source"],
            original_project: &payload["originalProject"],
            project: &payload["project"],
            report: &payload["report"],
            resources: &payload["resources"],
        })
        .unwrap();
        bundle["sha256"] = json!(format!("{:x}", Sha256::digest(&body)));
        let forged = serde_json::to_vec(&bundle).unwrap();
        std::fs::write(&path, &forged).unwrap();
        assert!(QuarantinedImport::reopen(&path, SessionConfig::default(), std_fs()).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), forged);
    }
}

#[test]
fn historical_replay_recovery_preserves_old_evidence() {
    for fixture in [
        include_bytes!("fixtures/asset-size-4d1da3b.json").as_slice(),
        include_bytes!("fixtures/asset-size-1ea025c.json").as_slice(),
    ] {
        let dir = TempDir::new("historical-recovery");
        let path = dir.join("review.bundle");
        let copy = dir.join("edited.bundle");
        std::fs::write(&path, fixture).unwrap();
        let mut review =
            QuarantinedImport::reopen(&path, SessionConfig::default(), std_fs()).unwrap();
        let report = review.report().clone();
        rename_review(&mut review, "Recovered historical edit");
        review.autosave().unwrap();
        drop(review);
        let mut recovered =
            QuarantinedImport::recover(&path, SessionConfig::default(), std_fs()).unwrap();
        assert_eq!(recovered.report(), &report);
        assert!(recovered.is_dirty());
        recovered.save_copy(&copy).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), fixture);
        let reopened =
            QuarantinedImport::reopen(&copy, SessionConfig::default(), std_fs()).unwrap();
        assert_eq!(reopened.report(), &report);
        assert_eq!(
            reopened.snapshot().project.screens[0].name,
            "Recovered historical edit"
        );
    }
}

#[test]
fn recomputed_recovery_checksum_cannot_strip_quarantine_provenance() {
    use sha2::{Digest, Sha256};
    let dir = TempDir::new("review-recovery-provenance");
    let path = dir.join("review.bundle");
    let mut review =
        QuarantinedImport::import_new(&source(), &path, SessionConfig::default(), std_fs())
            .unwrap();
    rename_review(&mut review, "Pending");
    review.autosave().unwrap();
    let slot = review_recovery_path(&path);
    let mut envelope: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&slot).unwrap()).unwrap();
    let mut project = review.snapshot().project;
    project.extensions.clear();
    envelope["body"]["project"] = json!(save_project(&project).unwrap());
    // RecoveryBody's two fields serialize in this exact order.
    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct RecoveryChecksum<'a> {
        base_sha256: &'a serde_json::Value,
        project: &'a serde_json::Value,
    }
    let body = &envelope["body"];
    let bytes = serde_json::to_vec(&RecoveryChecksum {
        base_sha256: &body["baseSha256"],
        project: &body["project"],
    })
    .unwrap();
    envelope["sha256"] = json!(format!("{:x}", Sha256::digest(&bytes)));
    let forged = serde_json::to_vec(&envelope).unwrap();
    std::fs::write(&slot, &forged).unwrap();
    assert!(QuarantinedImport::recover(&path, SessionConfig::default(), std_fs()).is_err());
    assert_eq!(std::fs::read(&slot).unwrap(), forged);
}

#[test]
fn review_recovery_refuses_even_semantically_equivalent_external_base_replacement() {
    let dir = TempDir::new("review-recovery-base");
    let path = dir.join("review.bundle");
    let mut review =
        QuarantinedImport::import_new(&source(), &path, SessionConfig::default(), std_fs())
            .unwrap();
    rename_review(&mut review, "Pending");
    review.autosave().unwrap();
    let slot = review_recovery_path(&path);
    let recovery = std::fs::read(&slot).unwrap();
    let mut base = std::fs::read(&path).unwrap();
    base.push(b'\n');
    std::fs::write(&path, &base).unwrap();
    // The modified base is itself valid, but not the exact saved envelope.
    assert!(QuarantinedImport::reopen(&path, SessionConfig::default(), std_fs()).is_ok());
    assert!(QuarantinedImport::recover(&path, SessionConfig::default(), std_fs()).is_err());
    assert!(review.autosave().is_err());
    assert!(!review.recovery_current());
    assert_eq!(std::fs::read(&slot).unwrap(), recovery);
}

#[test]
fn review_recovery_preserves_replaced_valid_same_base_snapshot() {
    let dir = TempDir::new("review-recovery-replaced");
    let path = dir.join("review.bundle");
    let copy = dir.join("copy.bundle");
    let mut review =
        QuarantinedImport::import_new(&source(), &path, SessionConfig::default(), std_fs())
            .unwrap();
    let slot = review_recovery_path(&path);
    rename_review(&mut review, "First");
    review.autosave().unwrap();
    let previous = std::fs::read(&slot).unwrap();
    rename_review(&mut review, "Second");
    review.autosave().unwrap();
    assert_ne!(std::fs::read(&slot).unwrap(), previous);
    std::fs::write(&slot, &previous).unwrap();
    assert!(review.autosave().is_err());
    assert!(!review.recovery_current());
    assert!(review.discard_recovery().is_err());
    assert_eq!(std::fs::read(&slot).unwrap(), previous);
    review.save_copy(&copy).unwrap();
    assert!(!review.warnings().is_empty());
    assert_eq!(std::fs::read(&slot).unwrap(), previous);
    assert_eq!(
        QuarantinedImport::recover(&path, SessionConfig::default(), std_fs())
            .unwrap()
            .snapshot()
            .project
            .screens[0]
            .name,
        "First"
    );
    assert_eq!(
        QuarantinedImport::reopen(&copy, SessionConfig::default(), std_fs())
            .unwrap()
            .snapshot()
            .project
            .screens[0]
            .name,
        "Second"
    );
}

#[test]
fn review_recovery_clean_undo_does_not_retire_snapshot_of_changed_base() {
    let dir = TempDir::new("review-recovery-changed-base-undo");
    let path = dir.join("review.bundle");
    let mut review =
        QuarantinedImport::import_new(&source(), &path, SessionConfig::default(), std_fs())
            .unwrap();
    rename_review(&mut review, "Pending");
    review.autosave().unwrap();
    let slot = review_recovery_path(&path);
    let recovery = std::fs::read(&slot).unwrap();
    let mut base = std::fs::read(&path).unwrap();
    base.push(b'\n');
    std::fs::write(&path, &base).unwrap();
    let undone = review.request(Request::Undo {
        expected_revision: Some(review.snapshot().revision),
    });
    assert!(matches!(undone, Response::Changed { outcome }
        if !outcome.status.dirty && !outcome.status.autosave_current));
    assert!(!review.is_dirty());
    assert!(!review.snapshot().status.autosave_current);
    assert!(review.autosave().is_err());
    assert!(!review.recovery_current());
    assert!(!review.snapshot().status.autosave_current);
    assert!(
        matches!(review.request(Request::Status {}), Response::Status { status }
        if !status.autosave_current)
    );
    assert!(
        matches!(review.request(Request::Snapshot {}), Response::Snapshot { snapshot }
        if !snapshot.status.autosave_current)
    );
    assert!(review.discard_recovery().is_err());
    assert_eq!(std::fs::read(&slot).unwrap(), recovery);
}

#[test]
fn review_recovery_status_is_consistent_across_undo_redo_and_cleanup() {
    let dir = TempDir::new("review-recovery-status");
    let path = dir.join("review.bundle");
    let slot = review_recovery_path(&path);
    let mut review =
        QuarantinedImport::import_new(&source(), &path, SessionConfig::default(), std_fs())
            .unwrap();
    assert!(review.recovery_current());
    let screen = review.snapshot().project.screens[0].id.clone();
    let outcome = review
        .execute(Transaction::single(Command::RenameScreen {
            screen,
            name: "Pending".into(),
        }))
        .unwrap();
    assert!(!outcome.status.autosave_current);
    assert_eq!(outcome.status.path, Some(path.display().to_string()));
    review.autosave().unwrap();
    assert!(review.recovery_current());
    let undone = review.request(Request::Undo {
        expected_revision: Some(review.snapshot().revision),
    });
    assert!(matches!(undone, Response::Changed { outcome }
        if !outcome.status.dirty && !outcome.status.autosave_current));
    assert!(slot.exists());
    assert!(!review.recovery_current());
    let redone = review.request(Request::Redo {
        expected_revision: Some(review.snapshot().revision),
    });
    assert!(matches!(redone, Response::Changed { outcome }
        if outcome.status.dirty && outcome.status.autosave_current));
    let _ = review.request(Request::Undo {
        expected_revision: Some(review.snapshot().revision),
    });
    assert!(!review.snapshot().status.autosave_current);
    review.autosave().unwrap();
    assert!(!slot.exists());
    assert!(review.recovery_current());
    assert!(
        matches!(review.request(Request::Status {}), Response::Status { status }
        if !status.dirty && status.autosave_current && status.path == Some(path.display().to_string()))
    );
}

#[test]
fn explicit_review_recovery_retirement_invalidates_dirty_snapshot() {
    let dir = TempDir::new("review-recovery-retire");
    let path = dir.join("review.bundle");
    let slot = review_recovery_path(&path);
    let mut review =
        QuarantinedImport::import_new(&source(), &path, SessionConfig::default(), std_fs())
            .unwrap();
    rename_review(&mut review, "Pending");
    review.autosave().unwrap();
    assert!(review.recovery_current());
    review.discard_recovery().unwrap();
    assert!(!slot.exists());
    assert!(review.is_dirty());
    assert!(!review.recovery_current());
    assert!(!review.snapshot().status.autosave_current);
    review.autosave().unwrap();
    assert!(slot.exists());
    assert!(review.recovery_current());
}

fn revision_records() -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    use sha2::{Digest, Sha256};
    let checked =
        |text: String| format!("{:x}\n{text}", Sha256::digest(text.as_bytes())).into_bytes();
    let descriptor = br#"{"format":"hmistudio-project-bundle","formatVersion":1}"#.to_vec();
    let head = checked(r#"{"formatVersion":1,"sequence":1,"currentRevisionId":"0000000001-1234abcd","nextRevisionNumber":2}"#.into());
    let revision = checked(format!(
        r#"{{"formatVersion":1,"id":"0000000001-1234abcd","number":1,"kind":"IMPORTED","label":"PRIVATE-HISTORY-TEST","project":{}}}"#,
        std::str::from_utf8(&source()).unwrap()
    ));
    (descriptor, head, revision)
}

#[test]
fn revision_review_persists_exact_records_through_recovery_and_save_copy() {
    let dir = TempDir::new("revision-review");
    let path = dir.join("revision.bundle");
    let copy = dir.join("copy.bundle");
    let (descriptor, head, revision) = revision_records();
    let evidence = RevisionSource::from_records(&descriptor, &head, &revision).unwrap();
    let mut review =
        QuarantinedImport::import_revision(evidence, &path, SessionConfig::default(), std_fs())
            .unwrap();
    assert!(review.has_revision_source());
    assert!(requires_review(&review.snapshot().project));
    assert!(!serde_json::to_string(review.report())
        .unwrap()
        .contains("PRIVATE-HISTORY-TEST"));
    let envelope: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(envelope["version"], 3);
    let records = &envelope["payload"]["revisionSource"];
    assert_eq!(
        records["descriptor"].as_str().unwrap().as_bytes(),
        descriptor
    );
    assert_eq!(records["head"].as_str().unwrap().as_bytes(), head);
    assert_eq!(records["revision"].as_str().unwrap().as_bytes(), revision);
    rename_review(&mut review, "Revision edit");
    review.autosave().unwrap();
    drop(review);
    let mut restored =
        QuarantinedImport::recover(&path, SessionConfig::default(), std_fs()).unwrap();
    assert!(restored.has_revision_source());
    assert!(restored.is_dirty());
    assert_eq!(restored.snapshot().project.screens[0].name, "Revision edit");
    restored.save_copy(&copy).unwrap();
    let copied: serde_json::Value = serde_json::from_slice(&std::fs::read(&copy).unwrap()).unwrap();
    assert_eq!(copied["version"], 3);
    assert_eq!(copied["payload"]["revisionSource"], *records);
    assert!(
        QuarantinedImport::reopen(&copy, SessionConfig::default(), std_fs())
            .unwrap()
            .has_revision_source()
    );
    assert!(!review_recovery_path(&path).exists());
}

#[test]
fn revision_review_rejects_recomputed_outer_checksum_and_wrong_container_version() {
    use sha2::{Digest, Sha256};
    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Records<'a> {
        descriptor: &'a serde_json::Value,
        head: &'a serde_json::Value,
        revision: &'a serde_json::Value,
    }
    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Checksum<'a> {
        source: &'a serde_json::Value,
        original_project: &'a serde_json::Value,
        project: &'a serde_json::Value,
        report: &'a serde_json::Value,
        resources: &'a serde_json::Value,
        revision_source: Records<'a>,
    }
    fn seal(value: &mut serde_json::Value) {
        let p = &value["payload"];
        let r = &p["revisionSource"];
        let bytes = serde_json::to_vec(&Checksum {
            source: &p["source"],
            original_project: &p["originalProject"],
            project: &p["project"],
            report: &p["report"],
            resources: &p["resources"],
            revision_source: Records {
                descriptor: &r["descriptor"],
                head: &r["head"],
                revision: &r["revision"],
            },
        })
        .unwrap();
        value["sha256"] = json!(format!("{:x}", Sha256::digest(bytes)));
    }
    let dir = TempDir::new("revision-evidence");
    let path = dir.join("revision.bundle");
    let (descriptor, head, revision) = revision_records();
    QuarantinedImport::import_revision(
        RevisionSource::from_records(&descriptor, &head, &revision).unwrap(),
        &path,
        SessionConfig::default(),
        std_fs(),
    )
    .unwrap();
    let good: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let mut wrong = good.clone();
    wrong["version"] = json!(2);
    std::fs::write(&path, serde_json::to_vec(&wrong).unwrap()).unwrap();
    assert!(QuarantinedImport::reopen(&path, SessionConfig::default(), std_fs()).is_err());
    let mut bad = good.clone();
    bad["payload"]["revisionSource"]["head"] = json!("damaged");
    seal(&mut bad);
    std::fs::write(&path, serde_json::to_vec(&bad).unwrap()).unwrap();
    assert!(QuarantinedImport::reopen(&path, SessionConfig::default(), std_fs()).is_err());
    // Fully checksummed alternate revision with the same ID but a different project.
    let mut foreign = good;
    let altered = std::str::from_utf8(&revision[65..])
        .unwrap()
        .replace("Synthetic", "Different");
    foreign["payload"]["revisionSource"]["revision"] = json!(format!(
        "{:x}\n{altered}",
        Sha256::digest(altered.as_bytes())
    ));
    seal(&mut foreign);
    std::fs::write(&path, serde_json::to_vec(&foreign).unwrap()).unwrap();
    assert!(QuarantinedImport::reopen(&path, SessionConfig::default(), std_fs()).is_err());
}

#[test]
fn package_review_replays_exact_archive_on_recovery_and_copy() {
    let dir = TempDir::new("package-review");
    let path = dir.join("review.bundle");
    let copy = dir.join("copy.bundle");
    let archive =
        include_bytes!("../../legacy-import/tests/fixtures/runtime-signed-java.synthetic");
    let mut review =
        QuarantinedImport::import_package(archive, &path, SessionConfig::default(), std_fs())
            .unwrap();
    assert!(review.has_package_source());
    assert!(!review.has_revision_source());
    assert!(requires_review(&review.snapshot().project));
    let before: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(before["version"], 4);
    let evidence = &before["payload"]["packageSource"];
    assert_eq!(evidence["decoderVersion"], 1);
    assert_eq!(evidence["archive"], json!(archive.as_slice()));
    assert!(!serde_json::to_string(review.report())
        .unwrap()
        .contains("packageSource"));
    rename_review(&mut review, "Package edit");
    review.autosave().unwrap();
    drop(review);
    let mut restored =
        QuarantinedImport::recover(&path, SessionConfig::default(), std_fs()).unwrap();
    assert!(restored.has_package_source());
    assert!(restored.is_dirty());
    assert_eq!(restored.snapshot().project.screens[0].name, "Package edit");
    restored.save_copy(&copy).unwrap();
    let after: serde_json::Value = serde_json::from_slice(&std::fs::read(&copy).unwrap()).unwrap();
    assert_eq!(after["version"], 4);
    assert_eq!(after["payload"]["packageSource"], *evidence);
    assert!(
        QuarantinedImport::reopen(&copy, SessionConfig::default(), std_fs())
            .unwrap()
            .has_package_source()
    );
    assert!(!review_recovery_path(&path).exists());
}

#[test]
fn package_review_rejects_resealed_invalid_archive_and_foreign_project() {
    use sha2::{Digest, Sha256};
    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Evidence<'a> {
        decoder_version: &'a serde_json::Value,
        archive: &'a serde_json::Value,
        archive_sha256: &'a serde_json::Value,
    }
    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Checksum<'a> {
        source: &'a serde_json::Value,
        original_project: &'a serde_json::Value,
        project: &'a serde_json::Value,
        report: &'a serde_json::Value,
        resources: &'a serde_json::Value,
        package_source: Evidence<'a>,
    }
    fn seal(value: &mut serde_json::Value) {
        let p = &value["payload"];
        let e = &p["packageSource"];
        let bytes = serde_json::to_vec(&Checksum {
            source: &p["source"],
            original_project: &p["originalProject"],
            project: &p["project"],
            report: &p["report"],
            resources: &p["resources"],
            package_source: Evidence {
                decoder_version: &e["decoderVersion"],
                archive: &e["archive"],
                archive_sha256: &e["archiveSha256"],
            },
        })
        .unwrap();
        value["sha256"] = json!(format!("{:x}", Sha256::digest(bytes)));
    }

    let dir = TempDir::new("package-evidence");
    let path = dir.join("review.bundle");
    let archive =
        include_bytes!("../../legacy-import/tests/fixtures/runtime-signed-java.synthetic");
    QuarantinedImport::import_package(archive, &path, SessionConfig::default(), std_fs()).unwrap();
    let good: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let mut invalid = good.clone();
    invalid["version"] = json!(3);
    std::fs::write(&path, serde_json::to_vec(&invalid).unwrap()).unwrap();
    assert!(QuarantinedImport::reopen(&path, SessionConfig::default(), std_fs()).is_err());
    let mut invalid = good.clone();
    invalid["payload"]["packageSource"]["decoderVersion"] = json!(2);
    seal(&mut invalid);
    std::fs::write(&path, serde_json::to_vec(&invalid).unwrap()).unwrap();
    assert!(QuarantinedImport::reopen(&path, SessionConfig::default(), std_fs()).is_err());
    let mut invalid = good.clone();
    let damaged = b"not a zip";
    invalid["payload"]["packageSource"]["archive"] = json!(damaged.as_slice());
    invalid["payload"]["packageSource"]["archiveSha256"] =
        json!(format!("{:x}", Sha256::digest(damaged)));
    seal(&mut invalid);
    std::fs::write(&path, serde_json::to_vec(&invalid).unwrap()).unwrap();
    assert!(QuarantinedImport::reopen(&path, SessionConfig::default(), std_fs()).is_err());
    // Retain a valid archive but replace its extracted portable project.
    let mut invalid = good;
    invalid["payload"]["source"] = json!(std::str::from_utf8(&source()).unwrap());
    seal(&mut invalid);
    std::fs::write(&path, serde_json::to_vec(&invalid).unwrap()).unwrap();
    assert!(QuarantinedImport::reopen(&path, SessionConfig::default(), std_fs()).is_err());
    let target = dir.join("refused.bundle");
    assert!(QuarantinedImport::import_package(
        damaged,
        &target,
        SessionConfig::default(),
        std_fs()
    )
    .is_err());
    assert!(!target.exists());
}

#[test]
fn transport_integrity_is_not_complete_portable_admission() {
    use hmi_engineering_core::legacy_quarantine::package_transport;
    let archive =
        include_bytes!("../../legacy-import/tests/fixtures/runtime-unsupported.synthetic");
    assert!(package_transport::decode_runtime_package(archive).is_ok());
    assert!(QuarantinedImport::prepare_package(archive).is_err());
    let dir = TempDir::new("unsupported-package");
    let target = dir.join("review.bundle");
    assert!(QuarantinedImport::import_package(
        archive,
        &target,
        SessionConfig::default(),
        std_fs()
    )
    .is_err());
    assert!(!target.exists());
}

#[test]
fn b5_07_raster_consumer_gate_enforces_full_decode_and_bounds() {
    use hmi_engineering_core::legacy_quarantine::raster_gate::{
        RasterError, RasterFormat, RasterLimits,
    };
    use hmi_engineering_core::legacy_quarantine::ImportError;
    use sha2::{Digest, Sha256};

    fn b64_encode(input: &[u8]) -> String {
        const TABLE: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
        for chunk in input.chunks(3) {
            let b0 = u32::from(chunk[0]);
            let b1 = u32::from(*chunk.get(1).unwrap_or(&0));
            let b2 = u32::from(*chunk.get(2).unwrap_or(&0));
            let n = (b0 << 16) | (b1 << 8) | b2;
            out.push(TABLE[((n >> 18) & 0x3f) as usize] as char);
            out.push(TABLE[((n >> 12) & 0x3f) as usize] as char);
            out.push(if chunk.len() > 1 {
                TABLE[((n >> 6) & 0x3f) as usize] as char
            } else {
                '='
            });
            out.push(if chunk.len() > 2 {
                TABLE[(n & 0x3f) as usize] as char
            } else {
                '='
            });
        }
        out
    }

    fn crc32(data: &[u8]) -> u32 {
        let mut crc = 0xffff_ffffu32;
        for &byte in data {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                let mask = (crc & 1).wrapping_neg();
                crc = (crc >> 1) ^ (0xedb8_8320 & mask);
            }
        }
        !crc
    }

    fn png_chunk(kind: [u8; 4], body: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(12 + body.len());
        out.extend_from_slice(&(body.len() as u32).to_be_bytes());
        out.extend_from_slice(&kind);
        out.extend_from_slice(body);
        let crc = crc32(&out[4..]);
        out.extend_from_slice(&crc.to_be_bytes());
        out
    }

    fn corrupt_deflate_png_2x2() -> Vec<u8> {
        let mut out = Vec::from(*b"\x89PNG\r\n\x1a\n");
        let mut ihdr = Vec::with_capacity(13);
        ihdr.extend_from_slice(&2u32.to_be_bytes());
        ihdr.extend_from_slice(&2u32.to_be_bytes());
        ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
        out.extend_from_slice(&png_chunk(*b"IHDR", &ihdr));
        out.extend_from_slice(&png_chunk(*b"IDAT", &[0xde, 0xad, 0xbe, 0xef]));
        out.extend_from_slice(&png_chunk(*b"IEND", &[]));
        out
    }

    // Valid 1x1 red RGBA PNG (70 bytes).
    let valid_png: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f,
        0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8,
        0xcf, 0xc0, 0xf0, 0x1f, 0x00, 0x05, 0x00, 0x01, 0xff, 0x89, 0x99, 0x3d, 0x1d, 0x00, 0x00,
        0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];
    let valid_png_b64 = b64_encode(valid_png);
    let valid_png_sha = format!("{:x}", Sha256::digest(valid_png));

    // Valid 2x3 baseline JPEG.
    let valid_jpeg_b64 = "/9j/4AAQSkZJRgABAQAAAQABAAD/2wBDAAMCAgICAgMCAgIDAwMDBAYEBAQEBAgGBgUGCQgKCgkICQkKDA8MCgsOCwkJDRENDg8QEBEQCgwSExIQEw8QEBD/wAALCAADAAIBAREA/8QAFAABAAAAAAAAAAAAAAAAAAAACf/EABQQAQAAAAAAAAAAAAAAAAAAAAD/2gAIAQEAAD8AVN//2Q==";
    let valid_jpeg_bytes: Vec<u8> = {
        // Decode the known constant inline without adding a base64 dependency.
        let mut out = Vec::new();
        let mut buf = 0u32;
        let mut bits = 0u32;
        for b in valid_jpeg_b64.bytes() {
            if b == b'=' {
                break;
            }
            let v = match b {
                b'A'..=b'Z' => u32::from(b - b'A'),
                b'a'..=b'z' => u32::from(b - b'a' + 26),
                b'0'..=b'9' => u32::from(b - b'0' + 52),
                b'+' => 62,
                b'/' => 63,
                _ => panic!("unexpected base64 char"),
            };
            buf = (buf << 6) | v;
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                out.push(((buf >> bits) & 0xff) as u8);
            }
        }
        out
    };
    let valid_jpeg_sha = format!("{:x}", Sha256::digest(&valid_jpeg_bytes));

    // Structurally valid PNG envelope with corrupt zlib stream (retained inert at import,
    // MUST be refused at the B5-07 consumer gate before rendering/extraction/packaging).
    let corrupt_png = corrupt_deflate_png_2x2();
    let corrupt_png_b64 = b64_encode(&corrupt_png);
    let corrupt_png_sha = format!("{:x}", Sha256::digest(&corrupt_png));

    let svg_text = r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect x="0" y="0" width="10" height="10" fill="#112233"/></svg>"##;
    let svg_sha = format!("{:x}", Sha256::digest(svg_text.as_bytes()));

    let legacy_project = serde_json::to_vec(&json!({
        "schemaVersion": 7,
        "metadata": {
            "name": "B5-07 Consumer Gate",
            "defaultScreenId": "screen/a",
            "targetProfiles": []
        },
        "screens": {
            "screen/a": {
                "id": "screen/a",
                "title": "Main",
                "widthDp": 640,
                "heightDp": 480,
                "components": {
                    "img-ok": { "id": "img-ok", "type": "image", "properties": { "assetId": "asset-png", "fit": "CONTAIN" } }
                }
            }
        },
        "variables": {},
        "externalSources": {},
        "externalMappings": {},
        "assets": {
            "asset-png": {
                "id": "asset-png",
                "name": "ok.png",
                "kind": "PNG",
                "data": valid_png_b64,
                "sha256": valid_png_sha,
                "sizeBytes": valid_png.len(),
                "widthPx": 1,
                "heightPx": 1
            },
            "asset-jpg": {
                "id": "asset-jpg",
                "name": "ok.jpg",
                "kind": "JPEG",
                "data": valid_jpeg_b64,
                "sha256": valid_jpeg_sha,
                "sizeBytes": valid_jpeg_bytes.len(),
                "widthPx": 2,
                "heightPx": 3
            },
            "asset-corrupt": {
                "id": "asset-corrupt",
                "name": "corrupt.png",
                "kind": "PNG",
                "data": corrupt_png_b64,
                "sha256": corrupt_png_sha,
                "sizeBytes": corrupt_png.len(),
                "widthPx": 2,
                "heightPx": 2
            },
            "asset-svg": {
                "id": "asset-svg",
                "name": "icon.svg",
                "kind": "SVG",
                "data": svg_text,
                "sha256": svg_sha,
                "sizeBytes": svg_text.len(),
                "widthPx": 10,
                "heightPx": 10
            }
        }
    }))
    .unwrap();

    let dir = TempDir::new("b5-07-gate");
    let bundle = dir.join("review.bundle");
    let review =
        QuarantinedImport::import_new(&legacy_project, &bundle, SessionConfig::default(), std_fs())
            .unwrap();

    // Snapshot remains strictly inert: no raw base64 raster bytes leak into the project snapshot.
    let snapshot_json = serde_json::to_string(&review.snapshot()).unwrap();
    assert!(!snapshot_json.contains(&valid_png_b64));
    assert!(!snapshot_json.contains(&corrupt_png_b64));
    assert!(!snapshot_json.contains(valid_jpeg_b64));

    let find_id = |media: &str, name: &str| {
        review
            .snapshot()
            .project
            .resources
            .iter()
            .find(|r| {
                r.media_type.as_deref() == Some(media)
                    && r.extensions.get("legacyName").and_then(|v| v.as_str()) == Some(name)
            })
            .unwrap()
            .id
            .as_str()
            .to_owned()
    };

    let png_id = find_id("image/png", "ok.png");
    let jpg_id = find_id("image/jpeg", "ok.jpg");
    let corrupt_id = find_id("image/png", "corrupt.png");
    let svg_id = find_id("image/svg+xml", "icon.svg");

    let decoded_png = review.decode_verified_raster(&png_id).unwrap();
    assert_eq!(decoded_png.format(), RasterFormat::Png);
    assert_eq!((decoded_png.width(), decoded_png.height()), (1, 1));
    assert_eq!(decoded_png.rgba8(), &[255, 0, 0, 255]);

    let decoded_jpg = review.decode_verified_raster(&jpg_id).unwrap();
    assert_eq!(decoded_jpg.format(), RasterFormat::Jpeg);
    assert_eq!((decoded_jpg.width(), decoded_jpg.height()), (2, 3));
    assert_eq!(decoded_jpg.rgba8().len(), 24);

    // Corrupt compressed stream retained in inert evidence MUST be refused at consumer decode.
    assert!(matches!(
        review.decode_verified_raster(&corrupt_id),
        Err(ImportError::RasterRefused(
            RasterError::MalformedCompressedData
        ))
    ));

    // Tight dimension, pixel-count, and decoded-byte bounds fail closed at consumer decode.
    assert!(matches!(
        review.decode_verified_raster_with_limits(
            &jpg_id,
            &RasterLimits {
                max_height: 2,
                ..RasterLimits::DEFAULT
            }
        ),
        Err(ImportError::RasterRefused(RasterError::DimensionLimit))
    ));
    assert!(matches!(
        review.decode_verified_raster_with_limits(
            &jpg_id,
            &RasterLimits {
                max_pixels: 5,
                ..RasterLimits::DEFAULT
            }
        ),
        Err(ImportError::RasterRefused(RasterError::PixelCountLimit))
    ));
    assert!(matches!(
        review.decode_verified_raster_with_limits(
            &jpg_id,
            &RasterLimits {
                max_decoded_bytes: 20,
                ..RasterLimits::DEFAULT
            }
        ),
        Err(ImportError::RasterRefused(RasterError::DecodedByteLimit))
    ));

    // Non-raster SVG and unknown resource IDs fail closed.
    assert!(matches!(
        review.decode_verified_raster(&svg_id),
        Err(ImportError::RasterRefused(
            RasterError::UnsupportedMediaType
        ))
    ));
    assert!(matches!(
        review.decode_verified_raster("missing-resource"),
        Err(ImportError::ResourceNotFound)
    ));
}
