//! # HMI Studio engineering core
//!
//! The authoritative engineering engine built on the RUST-03A project model
//! (`hmi-project-model`). One [`ProjectSession`] owns one project; the
//! graphical editor (and, later, optional AI agents) change it only through
//! typed [`Command`]s grouped into atomic [`Transaction`]s.
//!
//! * [`session`] — state ownership, revisions, dirty state, transactions,
//!   undo/redo with grouping.
//! * [`command`] — the typed command vocabulary.
//! * [`persistence`] — open, save, save-as, atomic replacement, backups,
//!   autosave, crash recovery, external-modification detection, migration.
//! * [`protocol`] — serializable request/response/notification contract for
//!   the future Tauri integration.
//! * [`fs`] — narrow filesystem abstraction (with fault injection for tests).
//!
//! No dependency on Tauri, React, Kotlin, drivers or the network. See
//! `docs/ENGINEERING_CORE.md`.

mod apply;
pub mod change;
pub mod command;
pub mod error;
pub mod fs;
pub mod ids;
pub mod legacy_quarantine;
pub mod locate;
pub mod persistence;
pub mod protocol;
pub mod session;

pub use change::{Change, ChangeKind, ChangeSet, EntityKind};
pub use command::{Command, Transaction, ValidationPolicy};
pub use error::{CommandError, TransactionError};
pub use persistence::{
    AutosaveOutcome, ExternalStatus, OpenError, OpenOutcome, RecoveryInfo, SaveError, SaveOptions,
    SaveOutcome,
};
pub use protocol::{dispatch, ChangeNotification, Request, Response};
pub use session::{
    ChangeCause, ProjectSession, ProjectSnapshot, SessionConfig, SessionStatus, TransactionOutcome,
};
