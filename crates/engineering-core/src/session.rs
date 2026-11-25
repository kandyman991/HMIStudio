//! The authoritative project session: state, revisions, transactions and
//! undo/redo.
//!
//! ## State model
//!
//! The session owns the project as an immutable snapshot (`Arc<Project>`).
//! A transaction clones the snapshot into a private working copy, applies
//! every command, validates the result and only then swaps the snapshot.
//! Any failure drops the working copy, so the visible project can never be
//! partially modified. Undo/redo swap back to earlier snapshots; because a
//! snapshot is the complete project, identifiers and references are restored
//! exactly.
//!
//! * **revision** — monotonically increasing counter, +1 for every commit,
//!   undo and redo. Clients send it back for optimistic concurrency.
//! * **state id** — identifies a project *content* state. Undo returns to an
//!   earlier state id; the session is dirty iff the current state id differs
//!   from the one last saved.

use std::collections::{BTreeSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;

use hmi_project_model::model::Project;
use hmi_project_model::{validate, Diagnostic, Severity, ValidationReport};
use serde::Serialize;

use crate::apply::{apply, Ctx};
use crate::change::{ChangeKind, ChangeSet, EntityKind};
use crate::command::{AllocatedIds, Transaction, ValidationPolicy};
use crate::error::TransactionError;
use crate::fs::{FileSystem, StdFileSystem};
use crate::ids;

/// Session configuration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionConfig {
    /// Maximum number of undo steps kept (0 disables undo).
    pub history_limit: usize,
    /// Number of backups kept per project file.
    pub backup_count: usize,
    /// Autosave location for projects that have never been saved.
    pub untitled_recovery_dir: Option<PathBuf>,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            history_limit: 200,
            backup_count: 5,
            untitled_recovery_dir: None,
        }
    }
}

/// One immutable project state.
#[derive(Clone, Debug)]
pub(crate) struct State {
    pub project: Arc<Project>,
    pub report: Arc<ValidationReport>,
    pub id: u64,
}

#[derive(Clone, Debug)]
struct HistoryEntry {
    label: String,
    before: State,
    after: State,
    changes: ChangeSet,
    merge_key: Option<String>,
}

/// Storage-related session state (see `persistence.rs`).
#[derive(Clone, Debug, Default)]
pub(crate) struct Storage {
    pub path: Option<PathBuf>,
    /// Exact bytes last read from / written to `path`.
    pub known_disk: Option<Vec<u8>>,
    /// Original disk fingerprint from a recovered autosave, including absence.
    pub recovery_base: Option<Option<String>>,
    /// State id that matches `path` on disk.
    pub saved_state: Option<u64>,
    /// State id last written to the autosave slot.
    pub autosaved_state: Option<u64>,
    /// Schema version the document was migrated from on open.
    pub migrated_from: Option<u32>,
}

/// What caused a change notification.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ChangeCause {
    /// A committed transaction.
    Transaction,
    /// Undo.
    Undo,
    /// Redo.
    Redo,
    /// Reload from disk.
    Reload,
}

/// Session status for UI chrome (title bar, menus, save indicator).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStatus {
    /// Current revision.
    pub revision: u64,
    /// Unsaved changes exist.
    pub dirty: bool,
    /// Undo available.
    pub can_undo: bool,
    /// Redo available.
    pub can_redo: bool,
    /// Label of the next undo step.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub undo_label: Option<String>,
    /// Label of the next redo step.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redo_label: Option<String>,
    /// Project file, if saved/opened.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Validation error count of the current state.
    pub error_count: usize,
    /// Validation warning count of the current state.
    pub warning_count: usize,
    /// Schema version migrated from on open (save writes the current one).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub migrated_from: Option<u32>,
    /// The autosave slot holds the current state (or nothing needs saving).
    pub autosave_current: bool,
    /// The session has been closed.
    pub closed: bool,
}

/// Result of a committed (or no-op) transaction, undo or redo.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionOutcome {
    /// Revision before.
    pub previous_revision: u64,
    /// Revision after (equal to `previous_revision` for a no-op).
    pub revision: u64,
    /// Whether the project changed.
    pub changed: bool,
    /// Cause.
    pub cause: ChangeCause,
    /// Undo label of the affected step.
    pub label: String,
    /// What changed.
    pub changes: ChangeSet,
    /// Identifiers allocated by the transaction (`"<command index>:<key>"`).
    pub allocated_ids: AllocatedIds,
    /// Whether this transaction was merged into the previous undo step.
    pub merged: bool,
    /// Diagnostics of the resulting state.
    pub diagnostics: Vec<Diagnostic>,
    /// Status after the operation.
    pub status: SessionStatus,
}

/// Full project snapshot for the UI.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSnapshot {
    /// Revision the snapshot belongs to.
    pub revision: u64,
    /// The project.
    pub project: Project,
    /// Diagnostics of this state.
    pub diagnostics: Vec<Diagnostic>,
    /// Status.
    pub status: SessionStatus,
}

/// The authoritative owner of one open project.
pub struct ProjectSession {
    pub(crate) current: State,
    revision: u64,
    next_state_id: u64,
    undo: VecDeque<HistoryEntry>,
    redo: Vec<HistoryEntry>,
    merge_open: bool,
    retired: BTreeSet<String>,
    pub(crate) config: SessionConfig,
    pub(crate) storage: Storage,
    pub(crate) fs: Arc<dyn FileSystem>,
    pub(crate) closed: bool,
}

impl std::fmt::Debug for ProjectSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProjectSession")
            .field("revision", &self.revision)
            .field("state", &self.current.id)
            .field("undo", &self.undo.len())
            .field("redo", &self.redo.len())
            .field("path", &self.storage.path)
            .finish()
    }
}

fn error_key(d: &Diagnostic) -> (String, String, String) {
    (format!("{:?}", d.code), d.path.clone(), d.message.clone())
}

fn invert(changes: &ChangeSet) -> ChangeSet {
    let mut out = ChangeSet::default();
    for c in changes.changes.iter().rev() {
        let kind = match c.kind {
            ChangeKind::Added => ChangeKind::Removed,
            ChangeKind::Removed => ChangeKind::Added,
            k => k,
        };
        out.push(kind, c.entity, c.id.clone());
    }
    out
}

impl ProjectSession {
    /// Creates an in-memory session for a new (untitled, clean) project.
    pub fn new(project: Project, config: SessionConfig) -> Self {
        Self::with_fs(project, config, Arc::new(StdFileSystem))
    }

    /// Like [`ProjectSession::new`] with a specific filesystem.
    pub fn with_fs(project: Project, config: SessionConfig, fs: Arc<dyn FileSystem>) -> Self {
        let mut s = Self::bare(project, config, fs);
        s.storage.saved_state = Some(s.current.id);
        s
    }

    pub(crate) fn bare(project: Project, config: SessionConfig, fs: Arc<dyn FileSystem>) -> Self {
        let report = validate(&project);
        let retired = ids::collect_ids(&project).into_iter().collect();
        Self {
            current: State {
                project: Arc::new(project),
                report: Arc::new(report),
                id: 0,
            },
            revision: 0,
            next_state_id: 1,
            undo: VecDeque::new(),
            redo: Vec::new(),
            merge_open: false,
            retired,
            config,
            storage: Storage::default(),
            fs,
            closed: false,
        }
    }

    // ------------------------------------------------------------ queries

    /// Read-only view of the authoritative project.
    pub fn project(&self) -> &Project {
        &self.current.project
    }

    /// Current revision.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Validation report of the current state (cached per state).
    pub fn report(&self) -> &ValidationReport {
        &self.current.report
    }

    /// Unsaved changes exist.
    pub fn is_dirty(&self) -> bool {
        self.storage.saved_state != Some(self.current.id)
    }

    /// Project file path.
    pub fn path(&self) -> Option<&std::path::Path> {
        self.storage.path.as_deref()
    }

    /// Number of undo steps.
    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }

    /// Number of redo steps.
    pub fn redo_depth(&self) -> usize {
        self.redo.len()
    }

    /// Status summary.
    pub fn status(&self) -> SessionStatus {
        let r = &self.current.report;
        SessionStatus {
            revision: self.revision,
            dirty: self.is_dirty(),
            can_undo: !self.closed && !self.undo.is_empty(),
            can_redo: !self.closed && !self.redo.is_empty(),
            undo_label: self.undo.back().map(|e| e.label.clone()),
            redo_label: self.redo.last().map(|e| e.label.clone()),
            path: self.storage.path.as_ref().map(|p| p.display().to_string()),
            error_count: r.error_count(),
            warning_count: r.warnings().count(),
            migrated_from: self.storage.migrated_from,
            autosave_current: !self.is_dirty()
                || self.storage.autosaved_state == Some(self.current.id),
            closed: self.closed,
        }
    }

    /// Full snapshot.
    pub fn snapshot(&self) -> ProjectSnapshot {
        ProjectSnapshot {
            revision: self.revision,
            project: (*self.current.project).clone(),
            diagnostics: self.current.report.diagnostics.clone(),
            status: self.status(),
        }
    }

    // -------------------------------------------------------- transactions

    /// Executes a transaction atomically. On error the project, revision and
    /// history are untouched.
    pub fn execute(&mut self, txn: Transaction) -> Result<TransactionOutcome, TransactionError> {
        if self.closed {
            return Err(TransactionError::Closed);
        }
        if txn.commands.is_empty() {
            return Err(TransactionError::Empty);
        }
        self.check_revision(txn.expected_revision)?;

        let mut work: Project = (*self.current.project).clone();
        let mut changes = ChangeSet::default();
        let mut allocated = AllocatedIds::new();
        let mut retired = self.retired.clone();
        for (index, cmd) in txn.commands.iter().enumerate() {
            let prior: BTreeSet<_> = ids::collect_ids(&work).into_iter().collect();
            let mut cx = Ctx {
                alloc: ids::allocator(&work, &retired),
                changes: &mut changes,
                allocated: &mut allocated,
                index,
            };
            apply(&mut work, cmd, &mut cx).map_err(|error| TransactionError::Command {
                index,
                op: cmd.op_name(),
                error,
            })?;
            for id in ids::collect_ids(&work) {
                if !prior.contains(&id) && retired.contains(&id) {
                    return Err(TransactionError::Command {
                        index,
                        op: cmd.op_name(),
                        error: crate::error::CommandError::AlreadyExists { id },
                    });
                }
                retired.insert(id);
            }
        }

        let label = txn.display_label();
        if work == *self.current.project {
            if txn.validation == ValidationPolicy::Strict && self.current.report.has_errors() {
                return Err(TransactionError::Validation {
                    errors: self.current.report.errors().cloned().collect(),
                });
            }
            return Ok(self.outcome(
                self.revision,
                false,
                ChangeCause::Transaction,
                label,
                ChangeSet::default(),
                AllocatedIds::new(),
                false,
            ));
        }

        let report = validate(&work);
        // Candidate v2 validation is fail-closed even if a base project was
        // previously invalid: do not admit unvalidated policy declarations.
        let policy = if work.has_v2_features() || self.current.project.has_v2_features() {
            ValidationPolicy::Strict
        } else {
            txn.validation
        };
        let new_errors: Vec<Diagnostic> = match policy {
            ValidationPolicy::Strict => report.errors().cloned().collect(),
            ValidationPolicy::RejectNewErrors => {
                let before: BTreeSet<_> = self.current.report.errors().map(error_key).collect();
                report
                    .diagnostics
                    .iter()
                    .filter(|d| d.severity == Severity::Error && !before.contains(&error_key(d)))
                    .cloned()
                    .collect()
            }
        };
        if !new_errors.is_empty() {
            return Err(TransactionError::Validation { errors: new_errors });
        }

        // ---- commit (infallible from here on)
        let previous = self.revision;
        let before = self.current.clone();
        let after = State {
            project: Arc::new(work),
            report: Arc::new(report),
            id: self.next_state_id,
        };
        self.next_state_id += 1;
        self.retired = retired;
        self.current = after.clone();
        self.revision += 1;
        self.redo.clear();

        let merged = match (&txn.merge_key, self.undo.back_mut()) {
            (Some(k), Some(last)) if self.merge_open && last.merge_key.as_ref() == Some(k) => {
                last.after = after;
                last.changes.extend(changes.clone());
                true
            }
            _ => {
                self.undo.push_back(HistoryEntry {
                    label: label.clone(),
                    before,
                    after,
                    changes: changes.clone(),
                    merge_key: txn.merge_key.clone(),
                });
                false
            }
        };
        self.merge_open = txn.merge_key.is_some();
        while self.undo.len() > self.config.history_limit {
            self.undo.pop_front();
        }
        Ok(self.outcome(
            previous,
            true,
            ChangeCause::Transaction,
            label,
            changes,
            allocated,
            merged,
        ))
    }

    fn check_revision(&self, expected: Option<u64>) -> Result<(), TransactionError> {
        match expected {
            Some(e) if e != self.revision => Err(TransactionError::StaleRevision {
                expected: e,
                actual: self.revision,
            }),
            _ => Ok(()),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn outcome(
        &self,
        previous_revision: u64,
        changed: bool,
        cause: ChangeCause,
        label: String,
        changes: ChangeSet,
        allocated_ids: AllocatedIds,
        merged: bool,
    ) -> TransactionOutcome {
        TransactionOutcome {
            previous_revision,
            revision: self.revision,
            changed,
            cause,
            label,
            changes,
            allocated_ids,
            merged,
            diagnostics: self.current.report.diagnostics.clone(),
            status: self.status(),
        }
    }

    // ----------------------------------------------------------- undo/redo

    /// Undoes the last step. `Ok(None)` when there is nothing to undo.
    pub fn undo(
        &mut self,
        expected_revision: Option<u64>,
    ) -> Result<Option<TransactionOutcome>, TransactionError> {
        if self.closed {
            return Err(TransactionError::Closed);
        }
        self.check_revision(expected_revision)?;
        let Some(entry) = self.undo.pop_back() else {
            return Ok(None);
        };
        let previous = self.revision;
        self.current = entry.before.clone();
        self.revision += 1;
        self.merge_open = false;
        let changes = invert(&entry.changes);
        let label = entry.label.clone();
        self.redo.push(entry);
        Ok(Some(self.outcome(
            previous,
            true,
            ChangeCause::Undo,
            label,
            changes,
            AllocatedIds::new(),
            false,
        )))
    }

    /// Redoes the last undone step. `Ok(None)` when there is nothing to redo.
    pub fn redo(
        &mut self,
        expected_revision: Option<u64>,
    ) -> Result<Option<TransactionOutcome>, TransactionError> {
        if self.closed {
            return Err(TransactionError::Closed);
        }
        self.check_revision(expected_revision)?;
        let Some(entry) = self.redo.pop() else {
            return Ok(None);
        };
        let previous = self.revision;
        self.current = entry.after.clone();
        self.revision += 1;
        self.merge_open = false;
        let changes = entry.changes.clone();
        let label = entry.label.clone();
        self.undo.push_back(entry);
        Ok(Some(self.outcome(
            previous,
            true,
            ChangeCause::Redo,
            label,
            changes,
            AllocatedIds::new(),
            false,
        )))
    }

    /// Clears undo and redo history (e.g. after reload).
    pub fn clear_history(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.merge_open = false;
    }

    // ------------------------------------------------- persistence helpers

    /// Marks the current state as matching disk (used after save).
    pub(crate) fn mark_saved(&mut self) {
        self.storage.saved_state = Some(self.current.id);
        self.merge_open = false;
    }

    /// Replaces the whole project (reload). Clears history.
    pub(crate) fn replace_project(&mut self, project: Project) -> TransactionOutcome {
        let previous = self.revision;
        let report = validate(&project);
        self.retired.extend(ids::collect_ids(&project));
        self.current = State {
            project: Arc::new(project),
            report: Arc::new(report),
            id: self.next_state_id,
        };
        self.next_state_id += 1;
        self.revision += 1;
        self.clear_history();
        let mut changes = ChangeSet::default();
        changes.push(ChangeKind::Modified, EntityKind::Project, None);
        self.outcome(
            previous,
            true,
            ChangeCause::Reload,
            "reload".into(),
            changes,
            AllocatedIds::new(),
            false,
        )
    }

    pub(crate) fn current_state_id(&self) -> u64 {
        self.current.id
    }
}
