//! Protocol-3 response/event decoder and bounded single-session dispatch.
//! Kept independent of the executable crate; the shared fixture verifies wire parity.
use crate::bridge_frame::MAX_FRAME_BYTES;
use serde::{Deserialize, Serialize};
use std::{collections::VecDeque, io};

/// A typed process value. Same JSON as the project model's `HmiValue`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "lowercase")]
pub enum Value {
    Bool(bool),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    String(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ValueType {
    Bool,
    Int,
    Long,
    Float,
    Double,
    String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SourceState {
    Disconnected,
    Connecting,
    Connected,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Quality {
    Good,
    Uncertain,
    Bad,
}

/// Why a write was refused. Same codes as `WriteRefusal` on the Kotlin side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WriteRefusal {
    NotConfigured,
    UnknownVariable,
    NotWritable,
    NotConfirmed,
    WritesDisarmed,
    TypeMismatch,
    NotFinite,
    /// A pulse to a point that is not momentary, or a plain write to one that is.
    WrongCommand,
    /// Outside the point's setpoint limits.
    OutOfLimits,
    SourceNotConnected,
    /// A pulse of this variable is still running.
    Busy,
    DriverError,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PointValue {
    pub variable_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
    pub quality: Quality,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_timestamp_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowseNode {
    pub node_id: String,
    pub display_name: String,
    pub node_class: String,
    #[serde(default)]
    pub has_children: bool,
    /// For a scalar variable: the variable type that can hold every value of the node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_type: Option<ValueType>,
    /// For a variable: whether the connected user may write it (the device's own answer).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub writable: Option<bool>,
    /// The node id with its namespace URI instead of an index (`nsu=<uri>;...`): unlike
    /// `nodeId` it stays valid when the server renumbers its namespaces.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stable_node_id: Option<String>,
}

/// Everything the engine sends: replies (with `id`) and unsolicited events.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Message {
    HelloOk {
        id: u64,
        protocol: u32,
        engine_version: String,
        drivers: Vec<String>,
    },
    Ok {
        id: u64,
    },
    Error {
        id: u64,
        code: String,
        message: String,
    },
    WriteResult {
        id: u64,
        accepted: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        refusal: Option<WriteRefusal>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    BrowseResult {
        id: u64,
        nodes: Vec<BrowseNode>,
    },
    SourceState {
        source_id: String,
        state: SourceState,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    Values {
        values: Vec<PointValue>,
    },
    WritesArmed {
        armed: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        actor: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expires_in_seconds: Option<u32>,
    },
    WriteAudit {
        variable_id: String,
        actor: String,
        value: Value,
        accepted: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        refusal: Option<WriteRefusal>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    /// The end of a pulse: FALSE was written (`released`), or could not be (`message`).
    PulseReleased {
        variable_id: String,
        actor: String,
        released: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
}

/// One in-flight request, with monotonically increasing IDs. No retries or reuse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplyKind {
    Hello,
    Ok,
    Browse,
    Write,
}

impl Message {
    fn reply(&self) -> Option<(u64, Option<ReplyKind>)> {
        match self {
            Self::HelloOk { id, .. } => Some((*id, Some(ReplyKind::Hello))),
            Self::Ok { id } => Some((*id, Some(ReplyKind::Ok))),
            Self::Error { id, .. } => Some((*id, None)),
            Self::BrowseResult { id, .. } => Some((*id, Some(ReplyKind::Browse))),
            Self::WriteResult { id, .. } => Some((*id, Some(ReplyKind::Write))),
            _ => None,
        }
    }
}

/// Bounded delivery. Overflow is terminal: do not treat a partial stream as synchronized.
pub const MAX_EVENTS: usize = 32;
pub const MAX_EVENT_BYTES: usize = 4 * MAX_FRAME_BYTES;

#[derive(Debug)]
pub struct Dispatcher {
    next_id: u64,
    pending: Option<(u64, ReplyKind)>,
    events: VecDeque<(Message, usize)>,
    bytes: usize,
    valid: bool,
}

impl Default for Dispatcher {
    fn default() -> Self {
        Self {
            next_id: 1,
            pending: None,
            events: VecDeque::new(),
            bytes: 0,
            valid: true,
        }
    }
}

impl Dispatcher {
    pub fn begin(&mut self, kind: ReplyKind) -> io::Result<u64> {
        if !self.valid {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "session invalid; resynchronization required",
            ));
        }
        if self.pending.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "request already in flight",
            ));
        }
        let id = self.next_id;
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or_else(|| self.fail("request ID exhausted"))?;
        self.pending = Some((id, kind));
        Ok(id)
    }

    /// Returns only the correlated response; unsolicited events are queued separately.
    pub fn accept(&mut self, frame: &[u8]) -> io::Result<Option<Message>> {
        if !self.valid {
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, "session invalid"));
        }
        if frame.len() > MAX_FRAME_BYTES {
            return Err(self.fail("oversized message"));
        }
        let message: Message = serde_json::from_slice(frame)
            .map_err(|_| self.fail("malformed or unknown protocol message"))?;
        if let Some((id, kind)) = message.reply() {
            if !self.pending.is_some_and(|(expected, expected_kind)| {
                id == expected && kind.is_none_or(|k| k == expected_kind)
            }) {
                return Err(self.fail("unexpected response ID or kind"));
            }
            self.pending = None;
            return Ok(Some(message));
        }
        if self.events.len() >= MAX_EVENTS || frame.len() > MAX_EVENT_BYTES - self.bytes {
            return Err(self.fail("event delivery overflow; resynchronization required"));
        }
        self.bytes += frame.len();
        self.events.push_back((message, frame.len()));
        Ok(None)
    }

    pub fn pop_event(&mut self) -> Option<Message> {
        let (event, size) = self.events.pop_front()?;
        self.bytes -= size;
        Some(event)
    }

    pub fn requires_resync(&self) -> bool {
        !self.valid
    }

    /// No partial backlog survives a failed session. A fresh handshake is required.
    pub fn invalidate(&mut self) {
        self.valid = false;
        self.pending = None;
        self.events.clear();
        self.bytes = 0;
    }

    fn fail(&mut self, reason: &str) -> io::Error {
        self.invalidate();
        io::Error::new(io::ErrorKind::InvalidData, reason)
    }
}
