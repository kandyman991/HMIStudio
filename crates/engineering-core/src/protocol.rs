//! Serializable frontend integration contract.
//!
//! A host (the Tauri shell in a later milestone, tests, or an optional AI
//! agent bridge) owns a [`ProjectSession`] and forwards [`Request`]s from the
//! UI to [`dispatch`]. The UI never receives a mutable project: it renders
//! [`ProjectSnapshot`]s and change notifications and sends typed requests
//! back. There is deliberately no request for arbitrary filesystem access or
//! command execution; file paths only appear in save-as, which the host
//! should obtain from a native file dialog.
//!
//! Opening, creating and recovering projects create sessions, so they are
//! host-level operations (see `docs/ENGINEERING_CORE.md`).

use std::path::PathBuf;

use hmi_project_model::migration::MigrationRegistry;
use hmi_project_model::Diagnostic;
use serde::{Deserialize, Serialize};

use crate::command::Transaction;
use crate::error::TransactionError;
use crate::persistence::{
    AutosaveOutcome, CloseError, ExternalStatus, ReloadError, SaveError, SaveOptions, SaveOutcome,
};
use crate::session::{ProjectSession, ProjectSnapshot, SessionStatus, TransactionOutcome};

/// Version of this request/response contract.
pub const PROTOCOL_VERSION: u32 = 1;

/// A request from the UI to the engineering session.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "request", rename_all = "camelCase", deny_unknown_fields)]
pub enum Request {
    /// Full project snapshot.
    Snapshot {},
    /// Status only (cheap; for title bar and menus).
    Status {},
    /// Current diagnostics.
    Validate {},
    /// Execute a transaction.
    #[serde(rename_all = "camelCase")]
    Execute {
        /// Transaction.
        transaction: Transaction,
    },
    /// Undo one step.
    #[serde(rename_all = "camelCase")]
    Undo {
        /// Optimistic revision check.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected_revision: Option<u64>,
    },
    /// Redo one step.
    #[serde(rename_all = "camelCase")]
    Redo {
        /// Optimistic revision check.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected_revision: Option<u64>,
    },
    /// Save to the current file.
    #[serde(rename_all = "camelCase")]
    Save {
        /// Options.
        #[serde(default)]
        options: SaveOptions,
    },
    /// Save to a new file (path from a native dialog).
    #[serde(rename_all = "camelCase")]
    SaveAs {
        /// Target.
        path: PathBuf,
        /// Options.
        #[serde(default)]
        options: SaveOptions,
    },
    /// Autosave tick.
    Autosave {},
    /// External modification check.
    CheckExternal {},
    /// Reload from disk.
    #[serde(rename_all = "camelCase")]
    Reload {
        /// Discard unsaved changes.
        #[serde(default)]
        force: bool,
    },
    /// Close the session.
    #[serde(rename_all = "camelCase")]
    Close {
        /// Discard unsaved changes.
        #[serde(default)]
        force: bool,
    },
}

/// Push notification emitted after any change of project content.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeNotification {
    /// Contract version.
    pub protocol: u32,
    /// The outcome (revision, change set, diagnostics, status).
    pub outcome: TransactionOutcome,
}

/// Structured error for the UI.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorBody {
    /// Stable code: `transaction`, `save`, `reload`, `close`.
    pub code: &'static str,
    /// Human-readable message.
    pub message: String,
    /// Structured details (the serialized error enum).
    pub details: serde_json::Value,
}

/// Response to a [`Request`].
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "response", rename_all = "camelCase")]
pub enum Response {
    /// Snapshot.
    Snapshot {
        /// Snapshot.
        snapshot: Box<ProjectSnapshot>,
    },
    /// Status.
    Status {
        /// Status.
        status: SessionStatus,
    },
    /// Diagnostics.
    Diagnostics {
        /// Revision.
        revision: u64,
        /// Diagnostics.
        diagnostics: Vec<Diagnostic>,
    },
    /// Transaction / undo / redo / reload result.
    Changed {
        /// Outcome.
        outcome: Box<TransactionOutcome>,
    },
    /// Undo/redo with nothing to do.
    Nothing {
        /// Status.
        status: SessionStatus,
    },
    /// Save result.
    Saved {
        /// Outcome.
        outcome: SaveOutcome,
        /// Status.
        status: SessionStatus,
    },
    /// Autosave result.
    Autosaved {
        /// Outcome.
        outcome: AutosaveOutcome,
        /// Status.
        status: SessionStatus,
    },
    /// External status.
    External {
        /// Status of the file.
        external: ExternalStatus,
    },
    /// Closed.
    Closed {},
    /// Failure; the session is unchanged.
    Error {
        /// Error.
        error: ErrorBody,
    },
}

fn error_response<E: std::fmt::Display + Serialize>(code: &'static str, e: &E) -> Response {
    Response::Error {
        error: ErrorBody {
            code,
            message: e.to_string(),
            details: serde_json::to_value(e).unwrap_or(serde_json::Value::Null),
        },
    }
}

fn reload_error(e: &ReloadError) -> Response {
    let kind = match e {
        ReloadError::UnsavedChanges => "unsavedChanges",
        ReloadError::NotSaved => "notSaved",
        ReloadError::Open(_) => "open",
        ReloadError::Closed => "closed",
    };
    Response::Error {
        error: ErrorBody {
            code: "reload",
            message: e.to_string(),
            details: serde_json::json!({ "kind": kind }),
        },
    }
}

fn changed(outcome: TransactionOutcome) -> (Response, Option<ChangeNotification>) {
    let note = outcome.changed.then(|| ChangeNotification {
        protocol: PROTOCOL_VERSION,
        outcome: outcome.clone(),
    });
    (
        Response::Changed {
            outcome: Box::new(outcome),
        },
        note,
    )
}

fn tx_result(
    session: &ProjectSession,
    r: Result<Option<TransactionOutcome>, TransactionError>,
) -> (Response, Option<ChangeNotification>) {
    match r {
        Ok(Some(o)) => changed(o),
        Ok(None) => (
            Response::Nothing {
                status: session.status(),
            },
            None,
        ),
        Err(e) => (error_response("transaction", &e), None),
    }
}

/// Handles one request. Returns the response for the requester and, when
/// project content changed, a notification to broadcast to every view.
pub fn dispatch(
    session: &mut ProjectSession,
    request: Request,
    migrations: &MigrationRegistry,
) -> (Response, Option<ChangeNotification>) {
    match request {
        Request::Snapshot {} => (
            Response::Snapshot {
                snapshot: Box::new(session.snapshot()),
            },
            None,
        ),
        Request::Status {} => (
            Response::Status {
                status: session.status(),
            },
            None,
        ),
        Request::Validate {} => (
            Response::Diagnostics {
                revision: session.revision(),
                diagnostics: session.report().diagnostics.clone(),
            },
            None,
        ),
        Request::Execute { transaction } => {
            let r = session.execute(transaction).map(Some);
            tx_result(session, r)
        }
        Request::Undo { expected_revision } => {
            let r = session.undo(expected_revision);
            tx_result(session, r)
        }
        Request::Redo { expected_revision } => {
            let r = session.redo(expected_revision);
            tx_result(session, r)
        }
        Request::Save { options } => save_response(session, |s| s.save(options)),
        Request::SaveAs { path, options } => save_response(session, |s| s.save_as(&path, options)),
        Request::Autosave {} => match session.autosave() {
            Ok(outcome) => (
                Response::Autosaved {
                    outcome,
                    status: session.status(),
                },
                None,
            ),
            Err(e) => (error_response("save", &e), None),
        },
        Request::CheckExternal {} => (
            Response::External {
                external: session.check_external(),
            },
            None,
        ),
        Request::Reload { force } => match session.reload(force, migrations) {
            Ok(o) => changed(o),
            Err(e) => (reload_error(&e), None),
        },
        Request::Close { force } => match session.close(force) {
            Ok(()) => (Response::Closed {}, None),
            Err(e @ CloseError::UnsavedChanges) => (error_response("close", &e), None),
        },
    }
}

fn save_response(
    session: &mut ProjectSession,
    f: impl FnOnce(&mut ProjectSession) -> Result<SaveOutcome, SaveError>,
) -> (Response, Option<ChangeNotification>) {
    match f(session) {
        Ok(outcome) => (
            Response::Saved {
                outcome,
                status: session.status(),
            },
            None,
        ),
        Err(e) => (error_response("save", &e), None),
    }
}
