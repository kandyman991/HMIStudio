//! The boundary between the Kotlin side (project, Studio, renderer) and this engine.
//!
//! Newline-delimited JSON over stdin (requests) and stdout (replies and events). The shapes
//! here mirror `core/runtime/EngineProtocol.kt`; `protocol/messages.json` holds one example of
//! every message and both sides must round-trip it unchanged.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// 3: IDE-07 adds the `pulse` request, `pulseReleased`, point limits and pulse times, and the
/// refusals `OUT_OF_LIMITS`, `WRONG_COMMAND` and `BUSY`.
pub const PROTOCOL_VERSION: u32 = 3;
pub const CONFIG_VERSION: u32 = 1;

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

impl Value {
    pub fn value_type(&self) -> ValueType {
        match self {
            Value::Bool(_) => ValueType::Bool,
            Value::Int(_) => ValueType::Int,
            Value::Long(_) => ValueType::Long,
            Value::Float(_) => ValueType::Float,
            Value::Double(_) => ValueType::Double,
            Value::String(_) => ValueType::String,
        }
    }

    /// The number, for limit checks; None for BOOL and STRING.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Int(v) => Some(*v as f64),
            Value::Long(v) => Some(*v as f64),
            Value::Float(v) => Some(*v as f64),
            Value::Double(v) => Some(*v),
            _ => None,
        }
    }

    pub fn is_finite(&self) -> bool {
        match self {
            Value::Float(v) => v.is_finite(),
            Value::Double(v) => v.is_finite(),
            _ => true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Access {
    Read,
    Write,
    ReadWrite,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeSource {
    pub id: String,
    pub driver: String,
    #[serde(default)]
    pub settings: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimePoint {
    pub mapping_id: String,
    pub variable_id: String,
    pub source_id: String,
    pub address: String,
    pub value_type: ValueType,
    pub access: Access,
    pub write_confirmed: bool,
    /// Inclusive setpoint limits; a value outside them is never written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limits: Option<WriteLimits>,
    /// A momentary command: only `pulse` requests of this length, never plain writes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pulse_ms: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WriteLimits {
    pub min: f64,
    pub max: f64,
}

impl RuntimePoint {
    pub fn readable(&self) -> bool {
        self.access != Access::Write
    }
    pub fn writable(&self) -> bool {
        self.access != Access::Read
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeConfig {
    pub version: u32,
    pub sources: Vec<RuntimeSource>,
    pub points: Vec<RuntimePoint>,
}

/// Held in memory only. Never logged: `Debug` hides the password.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceCredential {
    pub username: String,
    pub password: String,
}

impl std::fmt::Debug for SourceCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "SourceCredential(username={}, password=***)",
            self.username
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Request {
    Hello {
        id: u64,
        protocol: u32,
    },
    Configure {
        id: u64,
        config: RuntimeConfig,
        #[serde(default)]
        credentials: BTreeMap<String, SourceCredential>,
    },
    Start {
        id: u64,
    },
    Stop {
        id: u64,
    },
    ArmWrites {
        id: u64,
        actor: String,
        ttl_seconds: u32,
    },
    DisarmWrites {
        id: u64,
    },
    Write {
        id: u64,
        variable_id: String,
        value: Value,
        actor: String,
    },
    /// A momentary command: TRUE now, FALSE after the point's own `pulseMs`, both by the engine.
    Pulse {
        id: u64,
        variable_id: String,
        actor: String,
    },
    Browse {
        id: u64,
        source_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        node_id: Option<String>,
    },
    /// Names the variables currently in use (shown on screen, logged, alarmed). Only those
    /// are read from the devices; the others are reported once as `UNCERTAIN` / `NotPolled`.
    /// Without `variableIds`, everything is in use again (the state after `configure`).
    /// Writes are not affected.
    SetDemand {
        id: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        variable_ids: Option<Vec<String>>,
    },
    Shutdown {
        id: u64,
    },
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
