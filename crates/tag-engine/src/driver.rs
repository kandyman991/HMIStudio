//! Communication driver abstraction.
//!
//! A [`Driver`] talks to one device / server for one project
//! [`Connection`]. The engine owns each driver behind a mutex, so a driver
//! never sees concurrent calls and may keep plain mutable state. All
//! blocking operations receive an [`OpContext`] with a timeout and a
//! cancellation token and must honor both.
//!
//! RUST-06A ships only the simulator ([`crate::simulator`]). Siemens S7,
//! S7CommPlus, OPC UA, Modbus TCP and other drivers plug in through
//! [`DriverFactory`] in later milestones; see `docs/DRIVER_INTERFACE.md`.

use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use hmi_project_model::model::{Connection, DataType, DriverKind, Tag, TagId};
use serde::Serialize;

use crate::value::{MemberPath, TagValue, TypeContext};

/// Driver error category.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DriverErrorKind {
    /// Operation needs a connection.
    NotConnected,
    /// Connecting failed.
    ConnectFailed,
    /// The connection dropped during the operation.
    ConnectionLost,
    /// Timed out.
    Timeout,
    /// Cancelled by shutdown.
    Cancelled,
    /// Address unknown or malformed.
    AddressInvalid,
    /// Device value / requested type mismatch.
    TypeMismatch,
    /// Device refused the access.
    AccessDenied,
    /// Not supported by this driver.
    Unsupported,
    /// Other protocol error.
    Protocol,
}

impl DriverErrorKind {
    /// Whether the error means the connection is gone.
    pub fn is_connection_loss(self) -> bool {
        matches!(
            self,
            DriverErrorKind::NotConnected | DriverErrorKind::ConnectionLost
        )
    }
}

/// Driver error.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, thiserror::Error)]
#[serde(rename_all = "camelCase")]
#[error("{kind:?}: {message}")]
pub struct DriverError {
    /// Category.
    pub kind: DriverErrorKind,
    /// Details.
    pub message: String,
}

impl DriverError {
    /// New error.
    pub fn new(kind: DriverErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

/// Connection status reported by the engine.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum ConnectionStatus {
    /// Not connected (initial).
    Disconnected,
    /// Connect in progress.
    Connecting,
    /// Connected.
    Connected,
    /// Failed; the engine retries after the reconnect interval.
    Faulted {
        /// Last error.
        error: DriverError,
    },
    /// No usable driver (configuration); never retried.
    Unavailable {
        /// Reason.
        error: DriverError,
    },
    /// Engine shut down.
    Stopped,
}

/// What a driver can do.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriverCapabilities {
    /// Driver name.
    pub driver: String,
    /// Several items per read request.
    pub grouped_read: bool,
    /// Several items per write request.
    pub grouped_write: bool,
    /// Max items per read request (≥ 1).
    pub max_items_per_read: usize,
    /// Max items per write request (≥ 1).
    pub max_items_per_write: usize,
    /// Can write a member of a structured/array tag without rewriting it.
    pub member_writes: bool,
    /// Supports array values.
    pub arrays: bool,
    /// Supports struct values.
    pub structs: bool,
    /// Smallest useful polling interval (ms).
    pub min_scan_interval_ms: u32,
    /// Talks to real hardware (false for the simulator). Informative: the
    /// engine applies the same rules either way (D-004).
    pub physical: bool,
}

/// Cancellation token with an interruptible wait.
#[derive(Clone, Debug, Default)]
pub struct CancelToken(Arc<(Mutex<bool>, Condvar)>);

impl CancelToken {
    /// New, not cancelled.
    pub fn new() -> Self {
        Self::default()
    }

    /// Cancels and wakes all waiters.
    pub fn cancel(&self) {
        let (m, cv) = &*self.0;
        *m.lock().unwrap_or_else(|e| e.into_inner()) = true;
        cv.notify_all();
    }

    /// Whether cancelled.
    pub fn is_cancelled(&self) -> bool {
        *self.0 .0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Sleeps up to `d`; returns `true` if cancelled (immediately or while
    /// waiting).
    pub fn wait(&self, d: Duration) -> bool {
        let (m, cv) = &*self.0;
        let start = Instant::now();
        let mut cancelled = m.lock().unwrap_or_else(|e| e.into_inner());
        while !*cancelled {
            let remaining = d.saturating_sub(start.elapsed());
            if remaining.is_zero() {
                return false;
            }
            // Bound each platform wait; huge durations must not overflow an Instant
            // or a platform absolute timeout.
            cancelled = cv
                .wait_timeout(cancelled, remaining.min(Duration::from_secs(3600)))
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
        true
    }
}

/// Per-operation context.
#[derive(Clone, Debug)]
pub struct OpContext {
    /// Maximum time for the operation.
    pub timeout: Duration,
    /// Cancellation (engine shutdown).
    pub cancel: CancelToken,
}

/// One item of a grouped read. Reads always return the whole tag value.
#[derive(Clone, Debug, PartialEq)]
pub struct ReadItem {
    /// Tag.
    pub tag: TagId,
    /// Driver address.
    pub address: String,
    /// Tag data type.
    pub data_type: DataType,
}

/// One item of a grouped write.
#[derive(Clone, Debug, PartialEq)]
pub struct WriteItem {
    /// Tag.
    pub tag: TagId,
    /// Driver address.
    pub address: String,
    /// Tag data type (whole tag).
    pub data_type: DataType,
    /// Member written (root = whole value).
    pub member: MemberPath,
    /// Value, already type-checked against the member type.
    pub value: TagValue,
}

/// Per-item result of a grouped operation.
pub type ItemResult<T> = Result<T, DriverError>;

/// A communication driver for one connection.
///
/// Contract:
/// * `read`/`write` return `Err` for request-level failures (connection lost,
///   timeout, cancellation) and `Ok(results)` with **exactly one result per
///   item, in order**, otherwise.
/// * Request sizes never exceed the advertised `max_items_per_*`.
/// * Member writes are only issued when `member_writes` is advertised.
/// * Implementations must return promptly after `ctx.cancel` fires.
pub trait Driver: Send {
    /// Capabilities (constant for the driver's lifetime).
    fn capabilities(&self) -> DriverCapabilities;
    /// Establishes the connection.
    fn connect(&mut self, ctx: &OpContext) -> Result<(), DriverError>;
    /// Closes the connection (idempotent, must not block indefinitely).
    fn disconnect(&mut self);
    /// Whether the driver believes it is connected.
    fn is_connected(&self) -> bool;
    /// Grouped read.
    fn read(
        &mut self,
        items: &[ReadItem],
        ctx: &OpContext,
    ) -> Result<Vec<ItemResult<TagValue>>, DriverError>;
    /// Grouped write.
    fn write(
        &mut self,
        items: &[WriteItem],
        ctx: &OpContext,
    ) -> Result<Vec<ItemResult<()>>, DriverError>;
}

/// Creates drivers for connections of one [`DriverKind`].
pub trait DriverFactory: Send + Sync {
    /// Creates a driver for `connection`. `tags` are the project tags using
    /// it (e.g. for address pre-validation or simulator seeding).
    fn create(
        &self,
        connection: &Connection,
        tags: &[&Tag],
        types: &TypeContext,
    ) -> Result<Box<dyn Driver>, DriverError>;
}

/// Maps driver kinds to factories.
#[derive(Clone, Default)]
pub struct DriverRegistry {
    factories: Vec<(DriverKind, Arc<dyn DriverFactory>)>,
}

impl std::fmt::Debug for DriverRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list()
            .entries(self.factories.iter().map(|(k, _)| k))
            .finish()
    }
}

impl DriverRegistry {
    /// Empty registry (every connection is unavailable).
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers (or replaces) the factory for `kind`.
    pub fn register(&mut self, kind: DriverKind, factory: Arc<dyn DriverFactory>) {
        self.factories.retain(|(k, _)| *k != kind);
        self.factories.push((kind, factory));
    }

    /// Factory for `kind`.
    pub fn factory(&self, kind: DriverKind) -> Option<Arc<dyn DriverFactory>> {
        self.factories
            .iter()
            .find(|(k, _)| *k == kind)
            .map(|(_, f)| f.clone())
    }
}
