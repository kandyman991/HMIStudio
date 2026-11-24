//! Value quality.

use serde::Serialize;

/// Quality level (OPC UA-style good / uncertain / bad).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum QualityLevel {
    /// Value is current and trustworthy.
    Good,
    /// Value is usable with caution (e.g. outside its engineering range).
    Uncertain,
    /// Value must not be trusted (not connected, error, shutdown). The last
    /// known value is retained for diagnostics only.
    Bad,
}

/// Why a value has its quality.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum QualityReason {
    /// No special condition.
    None,
    /// Not read from the device yet.
    NotConnected,
    /// Connection lost or connect failed.
    ConnectionLost,
    /// Device or protocol error for this item.
    CommunicationError,
    /// Request timed out.
    Timeout,
    /// Value outside the tag's engineering range.
    OutOfRange,
    /// Value type does not match the tag.
    TypeMismatch,
    /// Tag cannot be read (write-only) — value is the last written one.
    NotReadable,
    /// Configuration problem (unknown connection, missing driver/address,
    /// invalid initial value).
    ConfigError,
    /// Engine stopped.
    Shutdown,
}

/// Quality of a tag value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Quality {
    /// Level.
    pub level: QualityLevel,
    /// Reason.
    pub reason: QualityReason,
}

impl Quality {
    /// Good, no reason.
    pub const GOOD: Quality = Quality {
        level: QualityLevel::Good,
        reason: QualityReason::None,
    };

    /// Bad with a reason.
    pub fn bad(reason: QualityReason) -> Self {
        Self {
            level: QualityLevel::Bad,
            reason,
        }
    }

    /// Uncertain with a reason.
    pub fn uncertain(reason: QualityReason) -> Self {
        Self {
            level: QualityLevel::Uncertain,
            reason,
        }
    }

    /// Whether the level is good.
    pub fn is_good(&self) -> bool {
        self.level == QualityLevel::Good
    }
}
