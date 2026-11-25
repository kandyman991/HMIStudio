//! Timestamps and injectable clocks.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

/// Microseconds since the Unix epoch (UTC).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct Timestamp(pub u64);

impl Timestamp {
    /// Milliseconds since the epoch.
    pub fn as_millis(self) -> u64 {
        self.0 / 1000
    }

    /// `self + ms`, saturating.
    pub fn plus_millis(self, ms: u64) -> Self {
        Timestamp(self.0.saturating_add(ms.saturating_mul(1000)))
    }
}

/// Source of time. Production uses [`SystemClock`]; tests use
/// [`ManualClock`] so timestamps, scan due-times and generators are
/// deterministic.
pub trait Clock: Send + Sync {
    /// Current time.
    fn now(&self) -> Timestamp;
}

/// Wall-clock time.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        Timestamp(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| u64::try_from(d.as_micros()).unwrap_or(u64::MAX)),
        )
    }
}

/// Manually advanced clock (shared by clones).
#[derive(Clone, Debug, Default)]
pub struct ManualClock(Arc<AtomicU64>);

impl ManualClock {
    /// Clock starting at `start`.
    pub fn new(start: Timestamp) -> Self {
        Self(Arc::new(AtomicU64::new(start.0)))
    }

    /// Advances by `ms` milliseconds.
    pub fn advance_millis(&self, ms: u64) {
        self.0.fetch_add(ms.saturating_mul(1000), Ordering::SeqCst);
    }

    /// Sets the time.
    pub fn set(&self, t: Timestamp) {
        self.0.store(t.0, Ordering::SeqCst);
    }
}

impl Clock for ManualClock {
    fn now(&self) -> Timestamp {
        Timestamp(self.0.load(Ordering::SeqCst))
    }
}
