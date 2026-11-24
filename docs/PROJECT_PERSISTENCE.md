# Project persistence (RUST-03B)

Implemented in `crates/engineering-core/src/persistence.rs` and `fs.rs`. The
project document format is the RUST-03A canonical JSON
([PROJECT_SCHEMA.md](PROJECT_SCHEMA.md)); this layer adds crash-safe file handling.

## Files

```text
line3.hmi.json                          project document (canonical JSON, LF)
.line3.hmi.json.tmp                     exists only during a save
line3.hmi.json.studio/                  sidecar directory
    backups/backup-000001.hmi.json      previous file versions (rotation)
    autosave.hmi.json                   autosave snapshot (canonical project JSON)
    autosave.meta.json                  autosave envelope
```

Untitled projects autosave to `SessionConfig.untitled_recovery_dir` (host-chosen,
e.g. an app-data folder); without it, autosave returns `noAutosaveLocation`.

## Operations

| Operation | Behavior |
| --- | --- |
| New | `ProjectSession::new(project, config)`: untitled and clean. |
| Open | Reads bytes, parses with the given `MigrationRegistry` (duplicate keys, unknown fields, unsupported versions are load errors with no file changes). Valid **and invalid** projects open; the validation report is returned and nothing is repaired. Migrated documents open dirty, with `migratedFrom`; the file is untouched until saved. Reports recoverable autosave data. |
| Save | Requires a path (`noPath`). See protocol below. Marks clean, removes autosave. |
| Save as | Like save, to a new path; refuses an existing unrelated file unless `replaceExisting` (which still backs it up). Rebinds the session and clears the old autosave slot. |
| Autosave | Writes a snapshot only when dirty and not already autosaved (`clean`, `upToDate`, `written`). Invalid projects are autosaved too. Never touches the project file. |
| Recover | `ProjectSession::recover(path)` opens the autosave snapshot bound to `path`, dirty. Refuses inconsistent snapshots. The autosave stays until the user saves or calls `discard_recovery`. `recover_untitled` for untitled projects. |
| Check external | Compares the file with the exact bytes last read/written: `notSaved`, `unchanged`, `modified`, `deleted`. |
| Reload | Replaces the session's project with the disk version (refuses unsaved changes unless forced); clears history. |
| Close | Refuses unsaved changes unless forced. Clean close removes autosave; forced close keeps it so work stays recoverable. |

## Save protocol

1. Serialize canonically and verify lossless (`to_json_string`: NaN/∞ refused).
2. Validation: projects with errors are refused (`invalidProject`) unless
   `allowInvalid`; even then, a file that currently holds a **valid** project is
   never replaced by an invalid one (`wouldReplaceValidProject`).
3. External modification: if the target's bytes differ from the bytes last read or
   written (or it was deleted), abort (`externalModification`) unless
   `overwriteExternalChanges`.
4. Write `.<name>.tmp` in the target directory, flush, `fsync`; read it back and
   compare byte-for-byte (catches short or silently truncated writes).
5. Write a backup of the current target (`fsync`); failure aborts the save.
   Rotate to `backup_count` (default 5); pruning failures are warnings.
6. Rename the temp file over the target; `fsync` the directory (Unix).

Steps 1–6 leave the original file untouched on any failure; the temp file is
removed. The session stays dirty. A stale temp file from an earlier crash is
simply overwritten by the next save.

## Autosave and recovery

- The envelope holds `format: "hmi-studio.autosave"`, `version`, `revision`,
  `projectFingerprint` (FNV-1a 64 of the snapshot) and `baseFingerprint` (of the
  project file bytes the session was based on).
- Write order: delete envelope → atomically write snapshot → atomically write
  envelope. A crash at any point leaves either no recoverable data or a
  consistent pair; a torn snapshot is never offered (`consistent: false` is
  refused).
- `RecoveryInfo.baseUnchanged` tells the UI whether the project file changed after
  the autosave (e.g. saved elsewhere), so it can warn before recovering.
- Fingerprints detect changes; they are not a security mechanism.

## Platform behavior

| Aspect | Linux / macOS | Windows |
| --- | --- | --- |
| Replace | `rename(2)`: atomic replacement of the directory entry. | `std::fs::rename` → `MoveFileExW(MOVEFILE_REPLACE_EXISTING)`: replaces on the same volume. |
| Read-only target | Replaced (file mode does not block rename; directory permissions do). | Replacement fails (`replace` stage); original intact. Tested per platform. |
| File open by another process | Rename succeeds. | Fails if the other process did not open with `FILE_SHARE_DELETE`; original intact. |
| Durability | File `fsync` + directory `fsync`. | File `FlushFileBuffers` (`sync_all`); NTFS journals the rename; no directory sync. |
| Line endings | Files written with LF. CRLF files (e.g. Git `autocrlf`) open fine, are reported unchanged, and are rewritten as LF (original kept in backups). |
| Paths | Spaces and non-ASCII names supported; temp and sidecar always live in the target directory (same volume as the rename). |

## Migration

Open uses the RUST-03A `MigrationRegistry`. Newer schema versions and older
versions without a migration path fail to open with an explicit
`unsupportedVersion` error. Migrated projects are written in the current schema
only when the user saves; the pre-migration file is kept in the backups.

## Limitations

- No file locking or cross-process coordination; concurrent editors are detected,
  not prevented.
- Assets referenced by `resources` are not copied, packaged or verified here.
- Backup retention is count-based only; no time-based policy.
- Directory fsync is unavailable on Windows through std; power-loss durability of
  the rename relies on NTFS journaling.
- Network filesystems and cloud-sync folders may weaken atomic rename guarantees;
  not tested.
- Recovery of a project whose file was deleted works (`baseUnchanged: false`), but
  autosave data lives in the sidecar next to the deleted file.
