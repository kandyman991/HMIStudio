//! Save/load equivalence, failed and interrupted writes, backups, external
//! modification, autosave, crash recovery, invalid projects and migration.
#![allow(clippy::unwrap_used)]

mod common;

use std::sync::Arc;

use common::*;
use hmi_engineering_core::fs::{Fault, FaultyFileSystem, FileSystem};
use hmi_engineering_core::persistence::{inspect_recovery, list_backups, sidecar_dir};
use hmi_engineering_core::{
    AutosaveOutcome, Command, ExternalStatus, OpenError, ProjectSession, SaveError, SaveOptions,
    SessionConfig,
};
use hmi_project_model::migration::{Migration, MigrationError, MigrationRegistry};
use hmi_project_model::serialization::LoadError;
use serde_json::{json, Value};

fn open(path: &std::path::Path) -> hmi_engineering_core::OpenOutcome {
    ProjectSession::open(path, SessionConfig::default(), std_fs(), &registry()).unwrap()
}

fn saved_session(dir: &TempDir) -> (ProjectSession, std::path::PathBuf) {
    let path = dir.join("line.hmi.json");
    let mut s = session(bottling());
    s.save_as(&path, SaveOptions::default()).unwrap();
    (s, path)
}

#[test]
fn save_then_open_is_equivalent_and_canonical() {
    let dir = TempDir::new("eq");
    let (mut s, path) = saved_session(&dir);
    run(
        &mut s,
        vec![Command::RenameScreen {
            screen: sid("scr-filler"),
            name: "Filler 2".into(),
        }],
    );
    let out = s.save(SaveOptions::default()).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert_eq!(text, canonical(s.project()));
    assert_eq!(out.bytes, text.len());
    let o = open(&path);
    assert_eq!(o.session.project(), s.project());
    assert!(o.report.diagnostics.is_empty());
    assert!(!o.session.is_dirty());
    assert!(o.recovery.is_none());
    // The original v1 fixture migrates only the schema version on explicit save.
    let fixture = std::fs::read_to_string(fixture_path("bottling-line.hmi.json")).unwrap();
    let copy = dir.join("copy.hmi.json");
    write(&copy, &fixture);
    let mut o = open(&copy);
    run(
        &mut o.session,
        vec![Command::RenameScreen {
            screen: sid("scr-filler"),
            name: "x".into(),
        }],
    );
    o.session.undo(None).unwrap();
    o.session
        .save(SaveOptions {
            overwrite_external_changes: false,
            ..SaveOptions::default()
        })
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(&copy).unwrap(),
        fixture.replacen("\"schemaVersion\": 1", "\"schemaVersion\": 2", 1)
    );
}

#[test]
fn save_requires_path_and_save_as_respects_existing_files() {
    let dir = TempDir::new("saveas");
    let mut s = session(minimal());
    assert_eq!(
        s.save(SaveOptions::default()).unwrap_err(),
        SaveError::NoPath
    );
    let other = dir.join("other.hmi.json");
    write(&other, "not mine");
    assert!(matches!(
        s.save_as(&other, SaveOptions::default()),
        Err(SaveError::TargetExists { .. })
    ));
    assert_eq!(std::fs::read_to_string(&other).unwrap(), "not mine");
    s.save_as(
        &other,
        SaveOptions {
            replace_existing: true,
            ..SaveOptions::default()
        },
    )
    .unwrap();
    assert_eq!(s.path(), Some(other.as_path()));
    // The replaced file was backed up.
    let backups = list_backups(&hmi_engineering_core::fs::StdFileSystem, &other);
    assert_eq!(backups.len(), 1);
    assert_eq!(std::fs::read_to_string(&backups[0]).unwrap(), "not mine");
}

#[test]
fn backups_rotate() {
    let dir = TempDir::new("backup");
    let path = dir.join("p.hmi.json");
    let mut s = ProjectSession::new(
        minimal(),
        SessionConfig {
            backup_count: 3,
            ..SessionConfig::default()
        },
    );
    s.save_as(&path, SaveOptions::default()).unwrap();
    let mut versions = vec![std::fs::read_to_string(&path).unwrap()];
    for i in 0..6 {
        run(
            &mut s,
            vec![Command::RenameScreen {
                screen: sid("scr-main"),
                name: format!("v{i}"),
            }],
        );
        let o = s.save(SaveOptions::default()).unwrap();
        assert!(o.backup.is_some());
        versions.push(std::fs::read_to_string(&path).unwrap());
    }
    let backups = list_backups(&hmi_engineering_core::fs::StdFileSystem, &path);
    assert_eq!(backups.len(), 3);
    let names: Vec<String> = backups
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        names,
        [
            "backup-000004.hmi.json",
            "backup-000005.hmi.json",
            "backup-000006.hmi.json"
        ]
    );
    // Newest backup is the version before the current file.
    assert_eq!(std::fs::read_to_string(&backups[2]).unwrap(), versions[5]);
}

fn faulty_save(
    fault: Fault,
    filter: &str,
) -> (
    TempDir,
    String,
    ProjectSession,
    SaveError,
    Arc<FaultyFileSystem>,
) {
    let dir = TempDir::new("fault");
    let path = dir.join("line.hmi.json");
    session(bottling())
        .save_as(&path, SaveOptions::default())
        .unwrap();
    let original = std::fs::read_to_string(&path).unwrap();
    let ffs = Arc::new(FaultyFileSystem::new(fault, filter, 0));
    let fs: Arc<dyn FileSystem> = ffs.clone();
    let mut s = ProjectSession::open(&path, SessionConfig::default(), fs, &registry())
        .unwrap()
        .session;
    run(
        &mut s,
        vec![Command::RenameScreen {
            screen: sid("scr-filler"),
            name: "Unsaved".into(),
        }],
    );
    let err = s.save(SaveOptions::default()).unwrap_err();
    (dir, original, s, err, ffs)
}

fn assert_original_intact(dir: &TempDir, original: &str, s: &ProjectSession) {
    let path = dir.join("line.hmi.json");
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        original,
        "target modified"
    );
    assert!(open(&path).report.is_valid());
    assert!(
        !dir.join(".line.hmi.json.tmp").exists(),
        "temp file left behind"
    );
    assert!(s.is_dirty(), "failed save must not mark the session clean");
    assert_eq!(s.check_external(), ExternalStatus::Unchanged);
}

#[test]
fn failed_write_leaves_original_intact() {
    let (dir, original, s, err, ffs) = faulty_save(Fault::WriteFails, ".tmp");
    assert!(ffs.fired());
    assert!(
        matches!(
            err,
            SaveError::Io {
                stage: "write temp",
                ..
            }
        ),
        "{err:?}"
    );
    assert_original_intact(&dir, &original, &s);
    assert!(
        list_backups(
            &hmi_engineering_core::fs::StdFileSystem,
            &dir.join("line.hmi.json")
        )
        .is_empty(),
        "no backup rotation on failed write"
    );
}

#[test]
fn interrupted_write_leaves_original_intact_and_next_save_succeeds() {
    let (dir, original, mut s, err, _) = faulty_save(Fault::WriteInterrupted, ".tmp");
    assert!(
        matches!(
            err,
            SaveError::Io {
                stage: "write temp",
                ..
            }
        ),
        "{err:?}"
    );
    assert_original_intact(&dir, &original, &s);
    // The fault fires once; retrying works and writes the edit.
    s.save(SaveOptions::default()).unwrap();
    assert!(std::fs::read_to_string(dir.join("line.hmi.json"))
        .unwrap()
        .contains("Unsaved"));
    assert!(!s.is_dirty());
}

#[test]
fn silently_truncated_write_is_caught_by_read_back() {
    let (dir, original, s, err, _) = faulty_save(Fault::WriteTruncatedSilently, ".tmp");
    assert!(
        matches!(
            err,
            SaveError::Io {
                stage: "verify temp",
                ..
            }
        ),
        "{err:?}"
    );
    assert_original_intact(&dir, &original, &s);
}

#[test]
fn failed_replace_leaves_original_intact() {
    let (dir, original, s, err, _) = faulty_save(Fault::RenameFails, "line.hmi.json");
    assert!(
        matches!(
            err,
            SaveError::Io {
                stage: "replace",
                ..
            }
        ),
        "{err:?}"
    );
    assert_original_intact(&dir, &original, &s);
}

#[test]
fn failed_backup_aborts_the_save() {
    let (dir, original, s, err, _) = faulty_save(Fault::WriteFails, "backup-");
    assert!(
        matches!(
            err,
            SaveError::Io {
                stage: "backup",
                ..
            }
        ),
        "{err:?}"
    );
    assert_original_intact(&dir, &original, &s);
}

#[test]
fn stale_temp_file_from_a_crash_does_not_block_saving() {
    let dir = TempDir::new("staletmp");
    let (mut s, path) = saved_session(&dir);
    write(&dir.join(".line.hmi.json.tmp"), "{ half a proj");
    run(
        &mut s,
        vec![Command::RenameScreen {
            screen: sid("scr-filler"),
            name: "after crash".into(),
        }],
    );
    s.save(SaveOptions::default()).unwrap();
    assert!(std::fs::read_to_string(&path)
        .unwrap()
        .contains("after crash"));
    assert_eq!(
        std::fs::read_to_string(dir.join(".line.hmi.json.tmp")).unwrap(),
        "{ half a proj"
    );
}

#[test]
fn external_modification_is_detected_and_never_overwritten_silently() {
    let dir = TempDir::new("ext");
    let (mut s, path) = saved_session(&dir);
    assert_eq!(s.check_external(), ExternalStatus::Unchanged);
    let theirs = std::fs::read_to_string(&path)
        .unwrap()
        .replace("Bottling Line 3", "Edited elsewhere");
    write(&path, &theirs);
    assert_eq!(s.check_external(), ExternalStatus::Modified);
    run(
        &mut s,
        vec![Command::RenameScreen {
            screen: sid("scr-filler"),
            name: "Mine".into(),
        }],
    );
    let err = s.save(SaveOptions::default()).unwrap_err();
    assert!(matches!(
        err,
        SaveError::ExternalModification {
            status: ExternalStatus::Modified,
            ..
        }
    ));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), theirs);
    // Reload refuses to discard unsaved work without force.
    assert!(s.reload(false, &registry()).is_err());
    // Explicit overwrite: their version goes to the backups.
    s.save(SaveOptions {
        overwrite_external_changes: true,
        ..SaveOptions::default()
    })
    .unwrap();
    let backups = list_backups(&hmi_engineering_core::fs::StdFileSystem, &path);
    assert_eq!(
        std::fs::read_to_string(backups.last().unwrap()).unwrap(),
        theirs
    );
    // Deleted externally.
    std::fs::remove_file(&path).unwrap();
    assert_eq!(s.check_external(), ExternalStatus::Deleted);
    run(
        &mut s,
        vec![Command::RenameScreen {
            screen: sid("scr-filler"),
            name: "Again".into(),
        }],
    );
    assert!(matches!(
        s.save(SaveOptions::default()),
        Err(SaveError::ExternalModification {
            status: ExternalStatus::Deleted,
            ..
        })
    ));
}

#[test]
fn reload_takes_the_disk_version() {
    let dir = TempDir::new("reload");
    let (mut s, path) = saved_session(&dir);
    let theirs = std::fs::read_to_string(&path)
        .unwrap()
        .replace("Bottling Line 3", "Theirs");
    write(&path, &theirs);
    let o = s.reload(false, &registry()).unwrap();
    assert!(o.changed);
    assert_eq!(s.project().metadata.name, "Theirs");
    assert!(!s.is_dirty());
    assert_eq!(s.undo_depth(), 0);
    assert_eq!(s.check_external(), ExternalStatus::Unchanged);
}

#[test]
fn autosave_and_crash_recovery() {
    let dir = TempDir::new("autosave");
    let (mut s, path) = saved_session(&dir);
    assert_eq!(s.autosave().unwrap(), AutosaveOutcome::Clean);
    run(
        &mut s,
        vec![Command::RenameScreen {
            screen: sid("scr-filler"),
            name: "Work in progress".into(),
        }],
    );
    assert!(!s.status().autosave_current);
    let AutosaveOutcome::Written { revision, .. } = s.autosave().unwrap() else {
        panic!()
    };
    assert_eq!(revision, s.revision());
    assert_eq!(s.autosave().unwrap(), AutosaveOutcome::UpToDate);
    assert!(s.status().autosave_current);
    let expected = s.project().clone();
    let disk_before = std::fs::read_to_string(&path).unwrap();
    drop(s); // simulated crash: no save, no close

    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        disk_before,
        "autosave never touches the project file"
    );
    let opened = open(&path);
    let info = opened.recovery.unwrap();
    assert!(info.consistent && info.base_unchanged);
    let mut r =
        ProjectSession::recover(&path, SessionConfig::default(), std_fs(), &registry()).unwrap();
    assert_eq!(r.session.project(), &expected);
    assert!(r.session.is_dirty());
    assert_eq!(r.session.path(), Some(path.as_path()));
    // Saving the recovered session writes the work and clears the autosave.
    r.session.save(SaveOptions::default()).unwrap();
    assert!(inspect_recovery(&hmi_engineering_core::fs::StdFileSystem, &path).is_none());
    assert_eq!(open(&path).session.project(), &expected);
}

#[test]
fn inconsistent_or_stale_autosave_is_reported_not_trusted() {
    let dir = TempDir::new("autosave2");
    let (mut s, path) = saved_session(&dir);
    run(
        &mut s,
        vec![Command::RenameScreen {
            screen: sid("scr-filler"),
            name: "WIP".into(),
        }],
    );
    s.autosave().unwrap();
    // The project file changed after the autosave was taken.
    let edited = std::fs::read_to_string(&path)
        .unwrap()
        .replace("Bottling Line 3", "Other");
    write(&path, &edited);
    let info = inspect_recovery(&hmi_engineering_core::fs::StdFileSystem, &path).unwrap();
    assert!(info.consistent && !info.base_unchanged);
    // Snapshot damaged (e.g. crash mid-write of a non-atomic copy).
    let snap = sidecar_dir(&path).join("autosave.meta.json");
    let mut envelope: Value =
        serde_json::from_str(&std::fs::read_to_string(&snap).unwrap()).unwrap();
    envelope["project"] = json!("damaged");
    write(&snap, &envelope.to_string());
    let info = inspect_recovery(&hmi_engineering_core::fs::StdFileSystem, &path).unwrap();
    assert!(!info.consistent);
    assert!(
        ProjectSession::recover(&path, SessionConfig::default(), std_fs(), &registry()).is_err()
    );
    hmi_engineering_core::persistence::discard_recovery(
        &hmi_engineering_core::fs::StdFileSystem,
        &path,
    );
    assert!(inspect_recovery(&hmi_engineering_core::fs::StdFileSystem, &path).is_none());
}

#[test]
fn interrupted_autosave_never_produces_a_bad_recovery() {
    let dir = TempDir::new("autosave3");
    let path = dir.join("line.hmi.json");
    session(bottling())
        .save_as(&path, SaveOptions::default())
        .unwrap();
    // First autosave succeeds.
    let ffs = Arc::new(FaultyFileSystem::new(
        Fault::WriteInterrupted,
        "autosave.meta",
        1,
    ));
    let fs: Arc<dyn FileSystem> = ffs.clone();
    let mut s = ProjectSession::open(&path, SessionConfig::default(), fs, &registry())
        .unwrap()
        .session;
    run(
        &mut s,
        vec![Command::RenameScreen {
            screen: sid("scr-filler"),
            name: "one".into(),
        }],
    );
    s.autosave().unwrap();
    // Second autosave is interrupted while writing the envelope.
    run(
        &mut s,
        vec![Command::RenameScreen {
            screen: sid("scr-filler"),
            name: "two".into(),
        }],
    );
    assert!(s.autosave().is_err());
    assert!(ffs.fired());
    let info = inspect_recovery(&hmi_engineering_core::fs::StdFileSystem, &path).unwrap();
    assert!(info.consistent);
    let recovered =
        ProjectSession::recover(&path, SessionConfig::default(), std_fs(), &registry()).unwrap();
    assert_eq!(
        recovered
            .session
            .project()
            .screens
            .iter()
            .find(|x| x.id.as_str() == "scr-filler")
            .unwrap()
            .name,
        "one"
    );
    // The session still knows the slot is stale and retries.
    assert!(!s.status().autosave_current);
    assert!(matches!(
        s.autosave().unwrap(),
        AutosaveOutcome::Written { .. }
    ));
    let r =
        ProjectSession::recover(&path, SessionConfig::default(), std_fs(), &registry()).unwrap();
    assert_eq!(
        r.session
            .project()
            .screens
            .iter()
            .find(|x| x.id.as_str() == "scr-filler")
            .unwrap()
            .name,
        "two"
    );
}

#[test]
fn untitled_projects_autosave_to_the_recovery_dir() {
    let dir = TempDir::new("untitled");
    let config = SessionConfig {
        untitled_recovery_dir: Some(dir.join("recovery")),
        ..SessionConfig::default()
    };
    let mut s = ProjectSession::new(minimal(), config.clone());
    let mut plain = session(minimal());
    run(
        &mut plain,
        vec![Command::RenameScreen {
            screen: sid("scr-main"),
            name: "x".into(),
        }],
    );
    assert_eq!(plain.autosave().unwrap_err(), SaveError::NoAutosaveLocation);
    run(
        &mut s,
        vec![Command::RenameScreen {
            screen: sid("scr-main"),
            name: "Draft".into(),
        }],
    );
    s.autosave().unwrap();
    let r = ProjectSession::recover_untitled(config, std_fs(), &registry()).unwrap();
    assert_eq!(r.session.project().screens[0].name, "Draft");
    assert!(r.session.path().is_none());
    // Saving the untitled session clears its recovery slot.
    s.save_as(&dir.join("named.hmi.json"), SaveOptions::default())
        .unwrap();
    assert!(!dir.join("recovery").join("autosave.meta.json").exists());
}

#[test]
fn close_preserves_autosave_until_explicit_discard() {
    let dir = TempDir::new("close");
    let (mut s, path) = saved_session(&dir);
    run(
        &mut s,
        vec![Command::RenameScreen {
            screen: sid("scr-filler"),
            name: "x".into(),
        }],
    );
    s.autosave().unwrap();
    assert!(s.close(false).is_err());
    s.close(true).unwrap();
    assert!(inspect_recovery(&hmi_engineering_core::fs::StdFileSystem, &path).is_some());
    assert!(s
        .execute(hmi_engineering_core::Transaction::single(
            Command::RenameScreen {
                screen: sid("scr-filler"),
                name: "y".into()
            }
        ))
        .is_err());
    let mut s2 = open(&path).session;
    s2.close(false).unwrap();
    assert!(inspect_recovery(&hmi_engineering_core::fs::StdFileSystem, &path).is_some());
    hmi_engineering_core::persistence::discard_recovery(
        &hmi_engineering_core::fs::StdFileSystem,
        &path,
    );
    assert!(inspect_recovery(&hmi_engineering_core::fs::StdFileSystem, &path).is_none());
}

#[test]
fn invalid_project_opens_with_diagnostics_and_is_not_repaired() {
    let dir = TempDir::new("invalid");
    let path = dir.join("bad.hmi.json");
    let mut doc: Value =
        serde_json::from_str(&std::fs::read_to_string(fixture_path("minimal.hmi.json")).unwrap())
            .unwrap();
    doc["schemaVersion"] = json!(2);
    doc["navigation"]["startScreen"] = json!("scr-gone");
    let text = serde_json::to_string_pretty(&doc).unwrap();
    write(&path, &text);
    let o = open(&path);
    assert!(o.report.has_errors());
    assert_eq!(o.report.diagnostics[0].path, "/navigation/startScreen");
    assert_eq!(
        o.session.project().navigation.start_screen.as_str(),
        "scr-gone",
        "not repaired"
    );
    assert!(!o.session.is_dirty());
    let mut s = o.session;
    // Saving an invalid project is refused by default...
    assert!(matches!(
        s.save(SaveOptions::default()),
        Err(SaveError::InvalidProject { .. })
    ));
    // ...allowed explicitly when the file is itself invalid...
    s.save(SaveOptions {
        allow_invalid: true,
        ..SaveOptions::default()
    })
    .unwrap();
    // ...but never over a valid project.
    let good = dir.join("good.hmi.json");
    session(minimal())
        .save_as(&good, SaveOptions::default())
        .unwrap();
    let good_text = std::fs::read_to_string(&good).unwrap();
    let err = s
        .save_as(
            &good,
            SaveOptions {
                allow_invalid: true,
                replace_existing: true,
                ..SaveOptions::default()
            },
        )
        .unwrap_err();
    assert!(matches!(err, SaveError::WouldReplaceValidProject { .. }));
    assert_eq!(std::fs::read_to_string(&good).unwrap(), good_text);
}

#[test]
fn unloadable_documents_fail_to_open_without_touching_the_file() {
    let dir = TempDir::new("unloadable");
    let path = dir.join("broken.hmi.json");
    for text in [
        "{ \"format\": ",
        "{\"format\":\"hmi-studio.project\",\"schemaVersion\":99}",
        "{\"format\":\"hmi-studio.project\",\"schemaVersion\":1,\"schemaVersion\":1}",
    ] {
        write(&path, text);
        let err = ProjectSession::open(&path, SessionConfig::default(), std_fs(), &registry())
            .unwrap_err();
        assert!(matches!(err, OpenError::Load { .. }), "{err}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
    }
    let err = ProjectSession::open(
        &dir.join("missing.hmi.json"),
        SessionConfig::default(),
        std_fs(),
        &registry(),
    )
    .unwrap_err();
    assert!(matches!(err, OpenError::Io { .. }));
    if let OpenError::Load {
        error: LoadError::UnsupportedVersion { .. },
        ..
    } = ProjectSession::open(
        &{
            write(
                &path,
                "{\"format\":\"hmi-studio.project\",\"schemaVersion\":99}",
            );
            path.clone()
        },
        SessionConfig::default(),
        std_fs(),
        &registry(),
    )
    .unwrap_err()
    {
    } else {
        panic!("expected unsupported version");
    }
}

struct V0ToV1;
impl Migration for V0ToV1 {
    fn source_version(&self) -> u32 {
        0
    }
    fn description(&self) -> &str {
        "synthetic test migration: root title -> metadata.name"
    }
    fn migrate(&self, mut doc: Value) -> Result<Value, MigrationError> {
        let o = doc.as_object_mut().unwrap();
        let title = o.remove("title").unwrap();
        o.insert("metadata".into(), json!({ "name": title }));
        o.insert("schemaVersion".into(), json!(1));
        Ok(doc)
    }
}

#[test]
fn opening_older_schema_migrates_in_memory_and_writes_only_on_save() {
    let dir = TempDir::new("migrate");
    let path = dir.join("old.hmi.json");
    let mut doc: Value =
        serde_json::from_str(&std::fs::read_to_string(fixture_path("minimal.hmi.json")).unwrap())
            .unwrap();
    let name = doc["metadata"]["name"].take();
    doc.as_object_mut().unwrap().remove("metadata");
    doc["title"] = name;
    doc["schemaVersion"] = json!(0);
    let old = serde_json::to_string_pretty(&doc).unwrap();
    write(&path, &old);
    // Without the migration: explicit unsupported-version error.
    assert!(ProjectSession::open(&path, SessionConfig::default(), std_fs(), &registry()).is_err());
    let mut reg = MigrationRegistry::builtin();
    reg.register(Box::new(V0ToV1)).unwrap();
    let o = ProjectSession::open(&path, SessionConfig::default(), std_fs(), &reg).unwrap();
    assert_eq!(o.migrated_from, Some(0));
    let mut s = o.session;
    assert!(s.is_dirty(), "migrated content differs from disk");
    assert_eq!(s.status().migrated_from, Some(0));
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        old,
        "file untouched until save"
    );
    s.save(SaveOptions::default()).unwrap();
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        canonical(&minimal())
    );
    // The pre-migration file is kept as a backup.
    let b = list_backups(&hmi_engineering_core::fs::StdFileSystem, &path);
    assert_eq!(std::fs::read_to_string(&b[0]).unwrap(), old);
}

#[test]
fn stale_recovery_requires_explicit_overwrite() {
    let dir = TempDir::new("recover-conflict");
    let (mut s, path) = saved_session(&dir);
    run(
        &mut s,
        vec![Command::RenameScreen {
            screen: sid("scr-filler"),
            name: "Recovered".into(),
        }],
    );
    s.autosave().unwrap();
    let edited = std::fs::read_to_string(&path)
        .unwrap()
        .replace("Bottling Line 3", "External");
    write(&path, &edited);
    let mut r = ProjectSession::recover(&path, SessionConfig::default(), std_fs(), &registry())
        .unwrap()
        .session;
    assert!(matches!(
        r.save(SaveOptions::default()),
        Err(SaveError::ExternalModification { .. })
    ));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), edited);
    run(
        &mut r,
        vec![Command::RenameScreen {
            screen: sid("scr-filler"),
            name: "Recovered again".into(),
        }],
    );
    r.autosave().unwrap();
    let mut r = ProjectSession::recover(&path, SessionConfig::default(), std_fs(), &registry())
        .unwrap()
        .session;
    assert!(matches!(
        r.save(SaveOptions::default()),
        Err(SaveError::ExternalModification { .. })
    ));
    r.save(SaveOptions {
        overwrite_external_changes: true,
        ..Default::default()
    })
    .unwrap();
}

#[cfg(unix)]
#[test]
fn old_temp_symlink_is_not_followed() {
    let dir = TempDir::new("tmp-link");
    let (mut s, path) = saved_session(&dir);
    let unrelated = dir.join("unrelated.txt");
    write(&unrelated, "keep me");
    std::os::unix::fs::symlink(&unrelated, dir.join(".line.hmi.json.tmp")).unwrap();
    run(
        &mut s,
        vec![Command::RenameScreen {
            screen: sid("scr-filler"),
            name: "New".into(),
        }],
    );
    s.save(SaveOptions::default()).unwrap();
    assert_eq!(std::fs::read_to_string(unrelated).unwrap(), "keep me");
    assert!(std::fs::read_to_string(path).unwrap().contains("New"));
}
