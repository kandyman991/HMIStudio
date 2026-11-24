//! Error types.

use hmi_project_model::Diagnostic;
use serde::Serialize;

/// Why one command could not be applied.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, thiserror::Error)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CommandError {
    /// Referenced object does not exist.
    #[error("{entity} {id:?} not found")]
    NotFound {
        /// Entity kind.
        entity: &'static str,
        /// Identifier.
        id: String,
    },
    /// An object with this identifier already exists.
    #[error("identifier {id:?} is already in use")]
    AlreadyExists {
        /// Identifier.
        id: String,
    },
    /// Tree roots cannot be removed or moved.
    #[error("{id:?} is a tree root and cannot be {action}")]
    RootNode {
        /// Node id.
        id: String,
        /// Attempted action.
        action: &'static str,
    },
    /// Moving a node into itself or its own subtree.
    #[error("cannot move {node:?} into its own subtree ({parent:?})")]
    CircularHierarchy {
        /// Node being moved.
        node: String,
        /// Requested parent.
        parent: String,
    },
    /// Index outside the valid range.
    #[error("index {index} out of range (max {max})")]
    IndexOutOfRange {
        /// Requested index.
        index: usize,
        /// Largest valid index.
        max: usize,
    },
    /// Argument rejected for another reason.
    #[error("{message}")]
    InvalidArgument {
        /// Explanation.
        message: String,
    },
}

/// Why a transaction was rejected. The project is unchanged in every case.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, thiserror::Error)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TransactionError {
    /// The transaction has no commands.
    #[error("transaction is empty")]
    Empty,
    /// Optimistic concurrency check failed.
    #[error("stale revision: expected {expected}, session is at {actual}")]
    StaleRevision {
        /// Revision the client expected.
        expected: u64,
        /// Current revision.
        actual: u64,
    },
    /// A command failed; nothing was committed.
    #[error("command {index} ({op}) failed: {error}")]
    Command {
        /// Zero-based command index.
        index: usize,
        /// Command name.
        op: &'static str,
        /// Cause.
        error: CommandError,
    },
    /// The result would violate the validation policy; nothing was committed.
    #[error("transaction rejected by validation ({} new error(s))", errors.len())]
    Validation {
        /// Errors the transaction would introduce.
        errors: Vec<Diagnostic>,
    },
    /// The session is closed.
    #[error("session is closed")]
    Closed,
}
