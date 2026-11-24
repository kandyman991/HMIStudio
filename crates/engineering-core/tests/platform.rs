//! Windows/Linux filesystem behavior. Runs on both CI hosts; expectations
//! that differ by platform are branched with `cfg`.
#![allow(clippy::unwrap_used)]

mod common;

use common::*;
use hmi_engineering_core::persistence::{list_backups, sidecar_dir};
use hmi_engineering_core::{
    Command, ExternalStatus, ProjectSession, SaveError, SaveOptions, SessionConfig,
};

#[test]
fn paths_with_spaces_and_unicode() {
    let dir = TempDir::new("unicode");
    let sub = dir.join("Linea 3 — impianto ü");
    std::fs::create_dir_all(&sub).unwrap();
    let path = sub.join("Progetto bottiglie.hmi.json");
    let mut s = session(bottling());
    s.save_as(&path, SaveOptions::default()).unwrap();
    run(
        &mut s,
        vec![Command::RenameScreen {
            screen: sid("scr-filler"),
            name: "Riempitrice".into(),
        }],
    );
    s.save(SaveOptions::default()).unwrap();
    s.autosave().unwrap();
    assert!(sidecar_dir(&path).is_dir());
    assert_eq!(
        list_backups(&hmi_engineering_core::fs::StdFileSystem, &path).len(),
        1
    );
    let o = ProjectSession::open(&path, SessionConfig::default(), std_fs(), &registry()).unwrap();
    assert_eq!(o.session.project(), s.project());
}

#[test]
fn replace_over_existing_file_is_atomic_rename() {
    let dir = TempDir::new("rename");
    let path = dir.join("p.hmi.json");
    let mut s = session(minimal());
    s.save_as(&path, SaveOptions::default()).unwrap();
    for i in 0..20 {
        run(
            &mut s,
            vec![Command::RenameScreen {
                screen: sid("scr-main"),
                name: format!("v{i}"),
            }],
        );
        s.save(SaveOptions::default()).unwrap();
        // After every save the file is complete and loadable.
        let o =
            ProjectSession::open(&path, SessionConfig::default(), std_fs(), &registry()).unwrap();
        assert_eq!(o.session.project().screens[0].name, format!("v{i}"));
    }
    let leftovers: Vec<_> = std::fs::read_dir(&dir.0)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[test]
fn crlf_and_non_canonical_files_open_and_are_detected_as_unchanged() {
    let dir = TempDir::new("crlf");
    let path = dir.join("win.hmi.json");
    let text = std::fs::read_to_string(fixture_path("minimal.hmi.json"))
        .unwrap()
        .replace('\n', "\r\n");
    write(&path, &text);
    let mut s = ProjectSession::open(&path, SessionConfig::default(), std_fs(), &registry())
        .unwrap()
        .session;
    assert_eq!(s.project(), &minimal());
    assert_eq!(s.check_external(), ExternalStatus::Unchanged);
    // A save rewrites in canonical LF form; the CRLF original is backed up.
    run(
        &mut s,
        vec![Command::RenameScreen {
            screen: sid("scr-main"),
            name: "x".into(),
        }],
    );
    s.save(SaveOptions::default()).unwrap();
    assert!(!std::fs::read_to_string(&path).unwrap().contains('\r'));
    let b = list_backups(&hmi_engineering_core::fs::StdFileSystem, &path);
    assert_eq!(std::fs::read_to_string(&b[0]).unwrap(), text);
}

#[test]
fn read_only_target_behaves_per_platform_without_data_loss() {
    let dir = TempDir::new("readonly");
    let path = dir.join("ro.hmi.json");
    let mut s = session(minimal());
    s.save_as(&path, SaveOptions::default()).unwrap();
    let original = std::fs::read_to_string(&path).unwrap();
    let mut perms = std::fs::metadata(&path).unwrap().permissions();
    perms.set_readonly(true);
    std::fs::set_permissions(&path, perms.clone()).unwrap();
    run(
        &mut s,
        vec![Command::RenameScreen {
            screen: sid("scr-main"),
            name: "changed".into(),
        }],
    );
    let result = s.save(SaveOptions::default());
    #[cfg(windows)]
    {
        // MoveFileEx cannot replace a read-only file: the save fails and the
        // original stays intact.
        assert!(
            matches!(
                result,
                Err(SaveError::Io {
                    stage: "replace",
                    ..
                })
            ),
            "{result:?}"
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        assert!(s.is_dirty());
    }
    #[cfg(not(windows))]
    {
        // POSIX rename replaces the directory entry; the file's own mode
        // does not block it (directory permissions do).
        let _ = &original;
        assert!(result.is_ok(), "{result:?}");
        assert!(std::fs::read_to_string(&path).unwrap().contains("changed"));
    }
    // Restore permissions so the temp dir can be removed on every platform.
    if let Ok(meta) = std::fs::metadata(&path) {
        let mut p = meta.permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        p.set_readonly(false);
        let _ = std::fs::set_permissions(&path, p);
    }
    let _ = SaveError::NoPath;
}

#[test]
fn sidecar_and_temp_names_stay_in_the_project_directory() {
    let dir = TempDir::new("names");
    let path = dir.join("a.hmi.json");
    assert_eq!(sidecar_dir(&path), dir.join("a.hmi.json.studio"));
    assert_eq!(sidecar_dir(&path).parent(), path.parent());
    // A relative path without a directory component is supported.
    let rel = std::path::Path::new("rel.hmi.json");
    assert_eq!(
        sidecar_dir(rel),
        std::path::PathBuf::from("rel.hmi.json.studio")
    );
}
