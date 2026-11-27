//! Crash-safe project persistence.
//!
//! ## On-disk layout
//!
//! ```text
//! line3.hmi.json                       project (canonical RUST-03A JSON)
//! .line3.hmi.json.<pid>-<counter>.tmp  uniquely staged temporary file
//! line3.hmi.json.studio/
//!     backups/backup-000001.hmi.json   previous versions (rotated)
//!     autosave.meta.json               atomic envelope (snapshot + fingerprints)
//! ```
//!
//! ## Save protocol
//!
//! 1. Serialize canonically and verify lossless (RUST-03A `to_json_string`).
//! 2. Refuse invalid projects unless explicitly allowed, and never replace a
//!    file that currently holds a valid project with an invalid one.
//! 3. Re-read the target and compare with the bytes last read/written: any
//!    difference is an external modification and aborts the save unless the
//!    caller explicitly overwrites.
//! 4. Write a temporary file in the target directory, fsync it, read it back
//!    and compare byte-for-byte.
//! 5. Copy the current target into the backup rotation (fsynced); if that
//!    fails the save is aborted.
//! 6. Atomically rename the temporary file over the target; fsync the
//!    directory where supported.
//!
//! A failure in steps 1–6 leaves the original file untouched. Autosave uses
//! the same temp+rename discipline for a single envelope.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::legacy_quarantine::{requires_review, storage_requires_review};

use hmi_project_model::migration::MigrationRegistry;
use hmi_project_model::serialization::{parse_project_with, to_json_string, LoadError};
use hmi_project_model::{validate, ValidationReport};
use serde::{Deserialize, Serialize};

use crate::fs::FileSystem;
use crate::session::{ProjectSession, SessionConfig, TransactionOutcome};

const AUTOSAVE_FORMAT: &str = "hmi-studio.autosave";
const SIDECAR_SUFFIX: &str = ".studio";

/// FNV-1a 64-bit fingerprint (change detection, not security).
pub fn fingerprint(data: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in data {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// Sidecar directory of a project file.
pub fn sidecar_dir(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(SIDECAR_SUFFIX);
    path.with_file_name(name)
}

fn temp_path(path: &Path) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let mut name = std::ffi::OsString::from(".");
    name.push(path.file_name().unwrap_or_default());
    name.push(format!(
        ".{}-{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    path.with_file_name(name)
}

fn backups_dir(path: &Path) -> PathBuf {
    sidecar_dir(path).join("backups")
}

fn parent_dir(path: &Path) -> PathBuf {
    match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

/// Save options.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveOptions {
    /// Allow saving a project that has validation errors (never over a file
    /// that currently holds a valid project).
    #[serde(default)]
    pub allow_invalid: bool,
    /// Overwrite even though the file changed on disk since it was read.
    #[serde(default)]
    pub overwrite_external_changes: bool,
    /// Save-as only: replace an existing, unrelated file at the target.
    #[serde(default)]
    pub replace_existing: bool,
}

/// Save / persistence failure. The target file is unchanged in every case.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, thiserror::Error)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SaveError {
    /// The session has no file yet; use save-as.
    #[error("project has no file; use save as")]
    NoPath,
    /// The project could not be serialized losslessly.
    #[error("serialization failed: {message}")]
    Serialize {
        /// Details.
        message: String,
    },
    /// The project has validation errors.
    #[error("project has {error_count} validation error(s)")]
    InvalidProject {
        /// Number of errors.
        error_count: usize,
    },
    /// Refused to replace a valid project file with an invalid project.
    #[error("refusing to replace the valid project at {path} with an invalid one")]
    WouldReplaceValidProject {
        /// Target.
        path: String,
    },
    /// The file was changed or deleted by someone else since it was read.
    #[error("{path} was modified externally ({status:?})")]
    ExternalModification {
        /// Target.
        path: String,
        /// What happened.
        status: ExternalStatus,
    },
    /// Save-as target exists and `replace_existing` was not set.
    #[error("{path} already exists")]
    TargetExists {
        /// Target.
        path: String,
    },
    /// I/O failure at a specific stage.
    #[error("{stage} failed for {path}: {message}")]
    Io {
        /// Stage: `write temp`, `verify temp`, `backup`, `replace`, ...
        stage: &'static str,
        /// File.
        path: String,
        /// OS message.
        message: String,
    },
    /// No location for autosave (untitled project without recovery dir).
    #[error("no autosave location for an untitled project")]
    NoAutosaveLocation,
    /// The session is closed.
    #[error("session is closed")]
    Closed,
}

fn io_err(stage: &'static str, path: &Path, e: &std::io::Error) -> SaveError {
    SaveError::Io {
        stage,
        path: path.display().to_string(),
        message: e.to_string(),
    }
}

/// Open failure.
#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    /// Imported provenance requires the dedicated review bundle reader.
    #[error("imported projects must be opened as verified review bundles")]
    ReviewRequired,
    /// File could not be read.
    #[error("cannot read {path}: {message}")]
    Io {
        /// File.
        path: String,
        /// OS message.
        message: String,
    },
    /// The document could not be loaded (syntax, structure, version).
    /// Nothing is repaired or discarded; fix the file or restore a backup.
    #[error("cannot load {path}: {error}")]
    Load {
        /// File.
        path: String,
        /// Cause (RUST-03A load error).
        error: LoadError,
    },
}

/// External modification status of the session's file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExternalStatus {
    /// Session has no file.
    NotSaved,
    /// File matches what the session last read or wrote.
    Unchanged,
    /// File content differs.
    Modified,
    /// File no longer exists.
    Deleted,
}

/// Result of a successful save.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveOutcome {
    /// File written.
    pub path: String,
    /// Bytes written.
    pub bytes: usize,
    /// Fingerprint of the written file.
    pub fingerprint: String,
    /// Backup created of the previous version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup: Option<String>,
    /// Non-fatal problems (backup rotation, directory sync, autosave cleanup).
    pub warnings: Vec<String>,
}

/// Result of opening a project.
#[derive(Debug)]
pub struct OpenOutcome {
    /// The session.
    pub session: ProjectSession,
    /// Validation report (may contain errors; nothing was repaired).
    pub report: ValidationReport,
    /// Schema version migrated from, if any (the file itself is untouched
    /// until the user saves).
    pub migrated_from: Option<u32>,
    /// Autosave data newer than the file, if present.
    pub recovery: Option<RecoveryInfo>,
}

/// Result of an autosave request.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "result", rename_all = "camelCase")]
pub enum AutosaveOutcome {
    /// Nothing unsaved; no snapshot needed.
    Clean,
    /// The autosave slot already holds this state.
    UpToDate,
    /// Snapshot written.
    #[serde(rename_all = "camelCase")]
    Written {
        /// Snapshot file.
        path: String,
        /// Revision captured.
        revision: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AutosaveMeta {
    format: String,
    version: u32,
    revision: u64,
    project_fingerprint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    base_fingerprint: Option<String>,
    project: String,
}

/// Information about recoverable autosave data.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryInfo {
    /// Autosave snapshot file.
    pub autosave_path: String,
    /// Revision captured in the snapshot.
    pub revision: u64,
    /// Snapshot and envelope agree (snapshot fully written).
    pub consistent: bool,
    /// The project file is still the version the snapshot was based on.
    pub base_unchanged: bool,
}

fn autosave_path(dir: &Path) -> PathBuf {
    dir.join("autosave.meta.json")
}

/// Writes and verifies `data` in the temporary file next to `target`.
pub(crate) fn stage_temp(
    fs: &dyn FileSystem,
    target: &Path,
    data: &[u8],
    write: impl Fn(&Path, &[u8]) -> std::io::Result<()>,
) -> Result<PathBuf, SaveError> {
    let tmp = loop {
        let candidate = temp_path(target);
        match write(&candidate, data) {
            Ok(()) => break candidate,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => {
                let _ = fs.remove_file(&candidate);
                return Err(io_err("write temp", &candidate, &e));
            }
        }
    };
    match fs.read(&tmp) {
        Ok(back) if back == data => Ok(tmp),
        Ok(_) => {
            let _ = fs.remove_file(&tmp);
            Err(SaveError::Io {
                stage: "verify temp",
                path: tmp.display().to_string(),
                message: "read-back differs from written data".into(),
            })
        }
        Err(e) => {
            let _ = fs.remove_file(&tmp);
            Err(io_err("verify temp", &tmp, &e))
        }
    }
}

/// Atomically replaces `target` with a staged temporary file.
fn commit_temp(fs: &dyn FileSystem, tmp: &Path, target: &Path) -> Result<(), SaveError> {
    if let Err(e) = fs.rename(tmp, target) {
        let _ = fs.remove_file(tmp);
        return Err(io_err("replace", target, &e));
    }
    Ok(())
}

/// Writes `data` to `target` through a synced, verified temp file + rename.
fn atomic_write(fs: &dyn FileSystem, target: &Path, data: &[u8]) -> Result<(), SaveError> {
    let tmp = stage_temp(fs, target, data, |path, data| fs.write_synced(path, data))?;
    commit_temp(fs, &tmp, target)
}

fn holds_valid_project(bytes: &[u8]) -> bool {
    std::str::from_utf8(bytes)
        .ok()
        .and_then(|t| parse_project_with(t, &MigrationRegistry::builtin()).ok())
        .is_some_and(|p| validate(&p.project).is_valid())
}

fn backup_sequence(file: &Path) -> Option<u64> {
    let name = file.file_name()?.to_str()?;
    name.strip_prefix("backup-")?
        .strip_suffix(".hmi.json")?
        .parse()
        .ok()
}

/// Lists backups of a project file, oldest first.
pub fn list_backups(fs: &dyn FileSystem, path: &Path) -> Vec<PathBuf> {
    let mut v: Vec<(u64, PathBuf)> = fs
        .list_dir(&backups_dir(path))
        .unwrap_or_default()
        .into_iter()
        .filter_map(|p| backup_sequence(&p).map(|n| (n, p)))
        .collect();
    v.sort();
    v.into_iter().map(|(_, p)| p).collect()
}

fn write_backup(
    fs: &dyn FileSystem,
    path: &Path,
    previous: &[u8],
    keep: usize,
    warnings: &mut Vec<String>,
) -> Result<Option<PathBuf>, SaveError> {
    if keep == 0 {
        return Ok(None);
    }
    let dir = backups_dir(path);
    fs.create_dir_all(&dir)
        .map_err(|e| io_err("backup", &dir, &e))?;
    let existing = list_backups(fs, path);
    let next = existing
        .last()
        .and_then(|p| backup_sequence(p))
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| SaveError::Io {
            stage: "backup",
            path: dir.display().to_string(),
            message: "backup sequence exhausted".into(),
        })?;
    let file = dir.join(format!("backup-{next:06}.hmi.json"));
    fs.write_synced(&file, previous)
        .map_err(|e| io_err("backup", &file, &e))?;
    if !matches!(fs.read(&file), Ok(bytes) if bytes == previous) {
        let _ = fs.remove_file(&file);
        return Err(SaveError::Io {
            stage: "backup",
            path: file.display().to_string(),
            message: "backup read-back differs".into(),
        });
    }
    let mut all = existing;
    all.push(file.clone());
    while all.len() > keep {
        let old = all.remove(0);
        if let Err(e) = fs.remove_file(&old) {
            warnings.push(format!("could not prune backup {}: {e}", old.display()));
        }
    }
    Ok(Some(file))
}

fn read_text(fs: &dyn FileSystem, path: &Path) -> Result<(Vec<u8>, String), OpenError> {
    let bytes = fs.read(path).map_err(|e| OpenError::Io {
        path: path.display().to_string(),
        message: e.to_string(),
    })?;
    let text = String::from_utf8(bytes.clone()).map_err(|e| OpenError::Io {
        path: path.display().to_string(),
        message: format!("not UTF-8: {e}"),
    })?;
    Ok((bytes, text))
}

/// Inspects autosave data for a project file (or untitled recovery dir).
pub fn inspect_recovery(fs: &dyn FileSystem, path: &Path) -> Option<RecoveryInfo> {
    inspect_dir(fs, &sidecar_dir(path), Some(path))
}

fn inspect_dir(fs: &dyn FileSystem, dir: &Path, base: Option<&Path>) -> Option<RecoveryInfo> {
    let path = autosave_path(dir);
    let meta: AutosaveMeta = serde_json::from_slice(&fs.read(&path).ok()?).ok()?;
    if meta.format != AUTOSAVE_FORMAT || meta.version != 2 {
        return None;
    }
    let current_base = base.and_then(|p| fs.read(p).ok()).map(|b| fingerprint(&b));
    Some(RecoveryInfo {
        autosave_path: path.display().to_string(),
        revision: meta.revision,
        consistent: fingerprint(meta.project.as_bytes()) == meta.project_fingerprint,
        base_unchanged: current_base == meta.base_fingerprint,
    })
}

/// Deletes autosave data for a project file.
pub fn discard_recovery(fs: &dyn FileSystem, path: &Path) {
    remove_autosave_in(fs, &sidecar_dir(path));
}

fn remove_autosave_in(fs: &dyn FileSystem, dir: &Path) {
    let _ = fs.remove_file(&autosave_path(dir));
}

impl ProjectSession {
    /// Opens a project file. Invalid projects open with their diagnostics;
    /// nothing is repaired or discarded. Migrated documents open dirty and
    /// the file is not rewritten until saved.
    pub fn open(
        path: &Path,
        config: SessionConfig,
        fs: Arc<dyn FileSystem>,
        migrations: &MigrationRegistry,
    ) -> Result<OpenOutcome, OpenError> {
        let (bytes, text) = read_text(fs.as_ref(), path)?;
        if storage_requires_review(&bytes) {
            return Err(OpenError::ReviewRequired);
        }
        let parsed = parse_project_with(&text, migrations).map_err(|error| OpenError::Load {
            path: path.display().to_string(),
            error,
        })?;
        if requires_review(&parsed.project) {
            return Err(OpenError::ReviewRequired);
        }
        let migrated_from = (!parsed.migrations.is_empty()).then_some(parsed.source_version);
        let recovery = inspect_recovery(fs.as_ref(), path);
        let mut session = ProjectSession::bare(parsed.project, config, fs);
        session.storage.path = Some(path.to_path_buf());
        session.storage.known_disk = Some(bytes);
        session.storage.migrated_from = migrated_from;
        if migrated_from.is_none() {
            session.storage.saved_state = Some(session.current_state_id());
        }
        let report = session.report().clone();
        Ok(OpenOutcome {
            session,
            report,
            migrated_from,
            recovery,
        })
    }

    /// Opens the autosave snapshot of `path` as an unsaved session bound to
    /// `path`. The autosave stays on disk until the user saves or discards.
    pub fn recover(
        path: &Path,
        config: SessionConfig,
        fs: Arc<dyn FileSystem>,
        migrations: &MigrationRegistry,
    ) -> Result<OpenOutcome, OpenError> {
        Self::recover_from_dir(&sidecar_dir(path), Some(path), config, fs, migrations)
    }

    /// Recovers an untitled project from `config.untitled_recovery_dir`.
    pub fn recover_untitled(
        config: SessionConfig,
        fs: Arc<dyn FileSystem>,
        migrations: &MigrationRegistry,
    ) -> Result<OpenOutcome, OpenError> {
        let dir = config.untitled_recovery_dir.clone().ok_or(OpenError::Io {
            path: String::new(),
            message: "no untitled recovery directory configured".into(),
        })?;
        Self::recover_from_dir(&dir, None, config, fs, migrations)
    }

    fn recover_from_dir(
        dir: &Path,
        base: Option<&Path>,
        config: SessionConfig,
        fs: Arc<dyn FileSystem>,
        migrations: &MigrationRegistry,
    ) -> Result<OpenOutcome, OpenError> {
        let info = inspect_dir(fs.as_ref(), dir, base).ok_or_else(|| OpenError::Io {
            path: dir.display().to_string(),
            message: "no autosave data".into(),
        })?;
        if !info.consistent {
            return Err(OpenError::Io {
                path: info.autosave_path,
                message: "autosave snapshot is incomplete or does not match its envelope".into(),
            });
        }
        let meta_path = autosave_path(dir);
        let (_, meta_text) = read_text(fs.as_ref(), &meta_path)?;
        let meta: AutosaveMeta = serde_json::from_str(&meta_text).map_err(|e| OpenError::Io {
            path: meta_path.display().to_string(),
            message: e.to_string(),
        })?;
        if meta.format != AUTOSAVE_FORMAT
            || meta.version != 2
            || meta.revision != info.revision
            || fingerprint(meta.project.as_bytes()) != meta.project_fingerprint
        {
            return Err(OpenError::Io {
                path: info.autosave_path,
                message: "autosave changed during recovery".into(),
            });
        }
        let parsed =
            parse_project_with(&meta.project, migrations).map_err(|error| OpenError::Load {
                path: info.autosave_path.clone(),
                error,
            })?;
        // A recovery payload cannot authorize opening a quarantined base. Keep
        // normal recovery of deleted or damaged ordinary projects available.
        let known_disk = match base.map(|path| fs.read(path)) {
            Some(Ok(bytes)) => Some(bytes),
            Some(Err(error)) if error.kind() != std::io::ErrorKind::NotFound => {
                return Err(OpenError::Io {
                    path: base.map_or_else(String::new, |path| path.display().to_string()),
                    message: error.to_string(),
                });
            }
            _ => None,
        };
        if requires_review(&parsed.project)
            || known_disk.as_deref().is_some_and(storage_requires_review)
        {
            return Err(OpenError::ReviewRequired);
        }
        let mut session = ProjectSession::bare(parsed.project, config, fs);
        session.storage.path = base.map(Path::to_path_buf);
        session.storage.known_disk = known_disk;
        session.storage.recovery_base = base.map(|_| meta.base_fingerprint);
        // Unsaved by definition; the autosave slot holds this state.
        session.storage.autosaved_state = Some(session.current_state_id());
        let report = session.report().clone();
        Ok(OpenOutcome {
            session,
            report,
            migrated_from: None,
            recovery: Some(info),
        })
    }

    /// Saves to the session's file.
    pub fn save(&mut self, options: SaveOptions) -> Result<SaveOutcome, SaveError> {
        let path = self.storage.path.clone().ok_or(SaveError::NoPath)?;
        self.save_to(&path, options, false)
    }

    /// Saves to a new file and binds the session to it.
    pub fn save_as(&mut self, path: &Path, options: SaveOptions) -> Result<SaveOutcome, SaveError> {
        let same = self.storage.path.as_deref() == Some(path);
        self.save_to(path, options, !same)
    }

    fn save_to(
        &mut self,
        path: &Path,
        options: SaveOptions,
        new_target: bool,
    ) -> Result<SaveOutcome, SaveError> {
        if self.closed {
            return Err(SaveError::Closed);
        }
        if requires_review(self.project()) {
            return Err(SaveError::Serialize {
                message: "legacy import requires quarantined bundle persistence".into(),
            });
        }
        let fs = self.fs.clone();
        let fs = fs.as_ref();
        let text = to_json_string(self.project()).map_err(|e| SaveError::Serialize {
            message: e.to_string(),
        })?;
        let data = text.into_bytes();
        let report = self.report();
        if report.has_errors() && !options.allow_invalid {
            return Err(SaveError::InvalidProject {
                error_count: report.error_count(),
            });
        }
        let on_disk = if fs.exists(path) {
            Some(fs.read(path).map_err(|e| io_err("read target", path, &e))?)
        } else {
            None
        };
        if on_disk.as_deref().is_some_and(storage_requires_review) {
            return Err(SaveError::Serialize {
                message: "cannot replace a quarantined file with an ordinary project".into(),
            });
        }
        // External modification / overwrite protection.
        if new_target {
            if on_disk.is_some() && !options.replace_existing {
                return Err(SaveError::TargetExists {
                    path: path.display().to_string(),
                });
            }
        } else if self
            .storage
            .recovery_base
            .as_ref()
            .is_some_and(|base| *base != on_disk.as_deref().map(fingerprint))
            && !options.overwrite_external_changes
        {
            return Err(SaveError::ExternalModification {
                path: path.display().to_string(),
                status: if on_disk.is_some() {
                    ExternalStatus::Modified
                } else {
                    ExternalStatus::Deleted
                },
            });
        } else if let Some(known) = &self.storage.known_disk {
            let status = match &on_disk {
                None => Some(ExternalStatus::Deleted),
                Some(d) if d != known => Some(ExternalStatus::Modified),
                Some(_) => None,
            };
            if let Some(status) = status {
                if !options.overwrite_external_changes {
                    return Err(SaveError::ExternalModification {
                        path: path.display().to_string(),
                        status,
                    });
                }
            }
        }
        if report.has_errors() {
            if let Some(d) = &on_disk {
                if holds_valid_project(d) {
                    return Err(SaveError::WouldReplaceValidProject {
                        path: path.display().to_string(),
                    });
                }
            }
        }

        let dir = parent_dir(path);
        let mut warnings = Vec::new();
        // 1. stage + verify; 2. back up the previous version (abort the save
        // if that fails); 3. atomic replace. A failed write never rotates
        // backups and never touches the target.
        let tmp = stage_temp(fs, path, &data, |path, data| fs.write_synced(path, data))?;
        let backup = match &on_disk {
            Some(prev) if !prev.is_empty() => {
                match write_backup(fs, path, prev, self.config.backup_count, &mut warnings) {
                    Ok(b) => b,
                    Err(e) => {
                        let _ = fs.remove_file(&tmp);
                        return Err(e);
                    }
                }
            }
            _ => None,
        };
        let current = match fs.read(path) {
            Ok(bytes) => Some(bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => {
                let _ = fs.remove_file(&tmp);
                return Err(io_err("recheck target", path, &e));
            }
        };
        if current != on_disk {
            let _ = fs.remove_file(&tmp);
            return Err(SaveError::ExternalModification {
                path: path.display().to_string(),
                status: if current.is_some() {
                    ExternalStatus::Modified
                } else {
                    ExternalStatus::Deleted
                },
            });
        }
        commit_temp(fs, &tmp, path)?;
        if let Err(e) = fs.sync_dir(&dir) {
            warnings.push(format!("directory sync failed: {e}"));
        }

        // Commit bookkeeping.
        let old_path = self.storage.path.replace(path.to_path_buf());
        if new_target {
            if let Some(old) = old_path {
                remove_autosave_in(fs, &sidecar_dir(&old));
            } else if let Some(dir) = &self.config.untitled_recovery_dir {
                remove_autosave_in(fs, dir);
            }
        }
        remove_autosave_in(fs, &sidecar_dir(path));
        self.storage.known_disk = Some(data.clone());
        self.storage.recovery_base = None;
        self.storage.autosaved_state = None;
        self.storage.migrated_from = None;
        self.mark_saved();
        Ok(SaveOutcome {
            path: path.display().to_string(),
            bytes: data.len(),
            fingerprint: fingerprint(&data),
            backup: backup.map(|p| p.display().to_string()),
            warnings,
        })
    }

    fn autosave_dir(&self) -> Result<PathBuf, SaveError> {
        match (&self.storage.path, &self.config.untitled_recovery_dir) {
            (Some(p), _) => Ok(sidecar_dir(p)),
            (None, Some(d)) => Ok(d.clone()),
            (None, None) => Err(SaveError::NoAutosaveLocation),
        }
    }

    /// Writes an autosave snapshot of the current state if it is unsaved
    /// and not yet autosaved. Invalid projects are autosaved too (the goal
    /// is to never lose work). The project file is never touched.
    pub fn autosave(&mut self) -> Result<AutosaveOutcome, SaveError> {
        if self.closed {
            return Err(SaveError::Closed);
        }
        if requires_review(self.project()) {
            return Err(SaveError::Serialize {
                message: "legacy import requires quarantined bundle persistence".into(),
            });
        }
        if !self.is_dirty() {
            // An undo can return to the saved state after a dirty autosave.
            // Retire only a slot this session owns, never an unresolved one.
            if self.storage.autosaved_state.is_some() {
                let dir = self.autosave_dir()?;
                let path = autosave_path(&dir);
                match self.fs.remove_file(&path) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(io_err("remove stale autosave", &path, &e)),
                }
                self.fs
                    .sync_dir(&dir)
                    .map_err(|e| io_err("autosave sync", &dir, &e))?;
                self.storage.autosaved_state = None;
            }
            return Ok(AutosaveOutcome::Clean);
        }
        if self.storage.autosaved_state == Some(self.current_state_id()) {
            return Ok(AutosaveOutcome::UpToDate);
        }
        let dir = self.autosave_dir()?;
        let fs = self.fs.clone();
        let fs = fs.as_ref();
        fs.create_dir_all(&dir)
            .map_err(|e| io_err("autosave", &dir, &e))?;
        let text = to_json_string(self.project()).map_err(|e| SaveError::Serialize {
            message: e.to_string(),
        })?;
        let meta_path = autosave_path(&dir);
        let meta = AutosaveMeta {
            format: AUTOSAVE_FORMAT.into(),
            version: 2,
            revision: self.revision(),
            project_fingerprint: fingerprint(text.as_bytes()),
            base_fingerprint: self
                .storage
                .recovery_base
                .clone()
                .unwrap_or_else(|| self.storage.known_disk.as_deref().map(fingerprint)),
            project: text,
        };
        let mut meta_text =
            serde_json::to_string_pretty(&meta).map_err(|e| SaveError::Serialize {
                message: e.to_string(),
            })?;
        meta_text.push('\n');
        // A single atomic envelope preserves the previous recoverable state
        // if writing or replacing the next snapshot fails.
        atomic_write(fs, &meta_path, meta_text.as_bytes())?;
        fs.sync_dir(&dir)
            .map_err(|e| io_err("autosave sync", &dir, &e))?;
        self.storage.autosaved_state = Some(self.current_state_id());
        Ok(AutosaveOutcome::Written {
            path: meta_path.display().to_string(),
            revision: self.revision(),
        })
    }

    /// Compares the file on disk with the bytes last read or written.
    pub fn check_external(&self) -> ExternalStatus {
        if let (Some(path), Some(base)) = (&self.storage.path, &self.storage.recovery_base) {
            let current = self.fs.read(path).ok().map(|b| fingerprint(&b));
            if &current != base {
                return if current.is_some() {
                    ExternalStatus::Modified
                } else {
                    ExternalStatus::Deleted
                };
            }
        }
        let (Some(path), Some(known)) = (&self.storage.path, &self.storage.known_disk) else {
            return ExternalStatus::NotSaved;
        };
        match self.fs.read(path) {
            Err(_) => ExternalStatus::Deleted,
            Ok(d) if &d == known => ExternalStatus::Unchanged,
            Ok(_) => ExternalStatus::Modified,
        }
    }

    /// Replaces the session's project with the file on disk. Refuses to
    /// discard unsaved changes unless `force`. Clears undo history.
    pub fn reload(
        &mut self,
        force: bool,
        migrations: &MigrationRegistry,
    ) -> Result<TransactionOutcome, ReloadError> {
        if self.closed {
            return Err(ReloadError::Closed);
        }
        if self.is_dirty() && !force {
            return Err(ReloadError::UnsavedChanges);
        }
        let path = self.storage.path.clone().ok_or(ReloadError::NotSaved)?;
        let (bytes, text) = read_text(self.fs.as_ref(), &path).map_err(ReloadError::Open)?;
        if storage_requires_review(&bytes) {
            return Err(ReloadError::Open(OpenError::ReviewRequired));
        }
        let parsed = parse_project_with(&text, migrations).map_err(|error| {
            ReloadError::Open(OpenError::Load {
                path: path.display().to_string(),
                error,
            })
        })?;
        if requires_review(&parsed.project) {
            return Err(ReloadError::Open(OpenError::ReviewRequired));
        }
        let outcome = self.replace_project(parsed.project);
        self.storage.known_disk = Some(bytes);
        self.storage.recovery_base = None;
        self.storage.migrated_from =
            (!parsed.migrations.is_empty()).then_some(parsed.source_version);
        if self.storage.migrated_from.is_none() {
            self.mark_saved();
        } else {
            self.storage.saved_state = None;
        }
        self.storage.autosaved_state = None;
        Ok(TransactionOutcome {
            status: self.status(),
            ..outcome
        })
    }

    /// Closes the session. Refuses with unsaved changes unless `force`.
    /// Closing preserves recovery data; only explicit discard or save removes it.
    pub fn close(&mut self, force: bool) -> Result<(), CloseError> {
        if self.closed {
            return Ok(());
        }
        if self.is_dirty() && !force {
            return Err(CloseError::UnsavedChanges);
        }
        // Closing an opened disk version is not permission to discard
        // pre-existing recovery data. Explicit discard or save owns cleanup.
        self.closed = true;
        Ok(())
    }
}

/// Reload failure.
#[derive(Debug, thiserror::Error)]
pub enum ReloadError {
    /// Unsaved changes would be lost.
    #[error("unsaved changes; reload with force to discard them")]
    UnsavedChanges,
    /// Session has no file.
    #[error("project has no file")]
    NotSaved,
    /// The file could not be loaded; the session is unchanged.
    #[error(transparent)]
    Open(OpenError),
    /// Session closed.
    #[error("session is closed")]
    Closed,
}

/// Close failure.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, thiserror::Error)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CloseError {
    /// Unsaved changes would be lost.
    #[error("unsaved changes")]
    UnsavedChanges,
}
