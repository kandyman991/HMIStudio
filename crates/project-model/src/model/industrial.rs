//! Industrial engineering contracts: connections, tags, data types, units,
//! alarms and recipes.
//!
//! These are *declarations* only. Communication drivers, alarm evaluation
//! and recipe execution are separate milestones; the runtime consumes these
//! contracts.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::common::{
    default_priority, default_true, is_default, is_default_priority, is_false, is_true,
    is_zero_u32, Extensions, LocalizedText, ValueRange,
};
use super::{
    AlarmId, ConnectionId, DataTypeId, RecipeFieldId, RecipeId, RecipeSetId, TagId, UnitId,
};

/// Tag / member data type.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub enum DataType {
    /// Boolean.
    #[serde(rename = "bool")]
    Bool,
    /// Signed 8-bit integer.
    #[serde(rename = "int8")]
    Int8,
    /// Signed 16-bit integer.
    #[serde(rename = "int16")]
    Int16,
    /// Signed 32-bit integer.
    #[serde(rename = "int32")]
    Int32,
    /// Signed 64-bit integer.
    #[serde(rename = "int64")]
    Int64,
    /// Unsigned 8-bit integer.
    #[serde(rename = "uint8")]
    UInt8,
    /// Unsigned 16-bit integer.
    #[serde(rename = "uint16")]
    UInt16,
    /// Unsigned 32-bit integer.
    #[serde(rename = "uint32")]
    UInt32,
    /// Unsigned 64-bit integer.
    #[serde(rename = "uint64")]
    UInt64,
    /// IEEE-754 single precision.
    #[serde(rename = "float32")]
    Float32,
    /// IEEE-754 double precision.
    #[serde(rename = "float64")]
    Float64,
    /// UTF-8 string.
    #[serde(rename = "string")]
    String,
    /// Date and time (ISO-8601 text in JSON values).
    #[serde(rename = "dateTime")]
    DateTime,
    /// Fixed-length array: `{ "array": { "element": ..., "length": n } }`.
    #[serde(rename = "array")]
    Array {
        /// Element type.
        element: Box<DataType>,
        /// Number of elements (>= 1).
        length: u32,
    },
    /// User-defined structure: `{ "struct": "<dataTypeId>" }`.
    #[serde(rename = "struct")]
    Struct(DataTypeId),
}

impl DataType {
    /// Integer bit width for integer types.
    pub fn integer_bits(&self) -> Option<u8> {
        match self {
            DataType::Int8 | DataType::UInt8 => Some(8),
            DataType::Int16 | DataType::UInt16 => Some(16),
            DataType::Int32 | DataType::UInt32 => Some(32),
            DataType::Int64 | DataType::UInt64 => Some(64),
            _ => None,
        }
    }

    /// Whether this is an integer type.
    pub fn is_integer(&self) -> bool {
        self.integer_bits().is_some()
    }

    /// Whether this is an integer or floating point type.
    pub fn is_numeric(&self) -> bool {
        self.is_integer() || matches!(self, DataType::Float32 | DataType::Float64)
    }

    /// Short name for diagnostics.
    pub fn describe(&self) -> String {
        match self {
            DataType::Bool => "bool".into(),
            DataType::Int8 => "int8".into(),
            DataType::Int16 => "int16".into(),
            DataType::Int32 => "int32".into(),
            DataType::Int64 => "int64".into(),
            DataType::UInt8 => "uint8".into(),
            DataType::UInt16 => "uint16".into(),
            DataType::UInt32 => "uint32".into(),
            DataType::UInt64 => "uint64".into(),
            DataType::Float32 => "float32".into(),
            DataType::Float64 => "float64".into(),
            DataType::String => "string".into(),
            DataType::DateTime => "dateTime".into(),
            DataType::Array { element, length } => format!("{}[{length}]", element.describe()),
            DataType::Struct(id) => format!("struct {id}"),
        }
    }
}

/// A user-defined structured type (PLC UDT).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StructType {
    /// Stable identifier.
    pub id: DataTypeId,
    /// Display name.
    pub name: String,
    /// Members in declaration order.
    pub members: Vec<StructMember>,
}

/// A member of a [`StructType`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StructMember {
    /// Member name (`[A-Za-z_][A-Za-z0-9_]*`).
    pub name: String,
    /// Member type.
    pub data_type: DataType,
    /// Description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Driver family of a connection. The driver itself is not part of this
/// crate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum DriverKind {
    /// Built-in simulator (development and testing; never touches hardware).
    Simulator,
    /// OPC UA client.
    OpcUa,
    /// Siemens S7 (ISO-on-TCP).
    SiemensS7,
    /// Modbus TCP.
    ModbusTcp,
    /// Third-party driver identified in `parameters`.
    Custom,
}

/// Reference to a PLC or other data source.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Connection {
    /// Stable identifier.
    pub id: ConnectionId,
    /// Display name.
    pub name: String,
    /// Driver family.
    pub driver: DriverKind,
    /// Endpoint (e.g. `opc.tcp://10.0.0.5:4840`). Deployment-specific
    /// endpoints may be overridden at publish time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    /// Driver-specific, non-secret parameters.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub parameters: BTreeMap<String, serde_json::Value>,
    /// Name of a credential held in the runtime's secure store. Projects
    /// never contain secrets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_ref: Option<String>,
    /// Extensible metadata.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: Extensions,
}

/// Tag access rights as seen by the HMI.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum TagAccess {
    /// HMI may only read.
    #[default]
    Read,
    /// HMI may only write.
    Write,
    /// HMI may read and write.
    ReadWrite,
}

impl TagAccess {
    /// Whether the HMI may write.
    pub fn writable(self) -> bool {
        matches!(self, TagAccess::Write | TagAccess::ReadWrite)
    }
    /// Whether the HMI may read.
    pub fn readable(self) -> bool {
        matches!(self, TagAccess::Read | TagAccess::ReadWrite)
    }
}

/// A tag: a typed process variable. Tags without a connection are internal
/// (HMI memory) tags.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Tag {
    /// Stable identifier.
    pub id: TagId,
    /// Display / browse name (may contain `/` for folders).
    pub name: String,
    /// Data type.
    pub data_type: DataType,
    /// Source connection; unset for internal tags.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection: Option<ConnectionId>,
    /// Driver-specific address (required with a connection), e.g.
    /// `ns=3;s="DB_Filler"."Speed"` or `DB10.DBD4`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    /// HMI access rights.
    #[serde(default, skip_serializing_if = "is_default")]
    pub access: TagAccess,
    /// Engineering unit (numeric tags only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<UnitId>,
    /// Engineering range (numeric tags only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<ValueRange>,
    /// Initial / simulation value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_value: Option<serde_json::Value>,
    /// Requested update interval in milliseconds (0 = driver default).
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub scan_rate_ms: u32,
    /// Description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Extensible metadata.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: Extensions,
}

/// An engineering unit.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EngineeringUnit {
    /// Stable identifier.
    pub id: UnitId,
    /// Display symbol (`°C`, `bar`, `m/s`).
    pub symbol: String,
    /// Display name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<LocalizedText>,
    /// Physical quantity (`temperature`, `pressure`, ...).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quantity: Option<String>,
}

/// Alarm severity class.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum AlarmSeverity {
    /// Information.
    Info,
    /// Warning.
    Warning,
    /// Alarm.
    #[default]
    Alarm,
    /// Critical / safety-relevant indication (the HMI is never the safety
    /// function itself).
    Critical,
}

/// Alarm trigger condition.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum AlarmCondition {
    /// Value > limit.
    #[serde(rename_all = "camelCase")]
    High {
        /// Limit.
        limit: f64,
    },
    /// Value < limit.
    #[serde(rename_all = "camelCase")]
    Low {
        /// Limit.
        limit: f64,
    },
    /// Value == constant.
    #[serde(rename_all = "camelCase")]
    Equals {
        /// Constant.
        value: serde_json::Value,
    },
    /// Bit `index` of an integer value is set.
    #[serde(rename_all = "camelCase")]
    BitSet {
        /// Bit index.
        index: u8,
    },
    /// Boolean value is true.
    #[serde(rename_all = "camelCase")]
    IsTrue {},
    /// Boolean value is false.
    #[serde(rename_all = "camelCase")]
    IsFalse {},
}

/// An alarm definition.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AlarmDefinition {
    /// Stable identifier.
    pub id: AlarmId,
    /// Engineer-facing name.
    pub name: String,
    /// Monitored tag.
    pub tag: TagId,
    /// Member path into the tag.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub member: Option<String>,
    /// Trigger condition.
    pub condition: AlarmCondition,
    /// Hysteresis for `high`/`low` (>= 0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadband: Option<f64>,
    /// Condition must hold this long before raising (ms).
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub delay_ms: u32,
    /// Severity class.
    #[serde(default, skip_serializing_if = "is_default")]
    pub severity: AlarmSeverity,
    /// Priority 1..=1000 (higher = more urgent).
    #[serde(
        default = "default_priority",
        skip_serializing_if = "is_default_priority"
    )]
    pub priority: u16,
    /// Operator message.
    pub message: LocalizedText,
    /// Group for filtering and group acknowledgement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// Operator acknowledgement required.
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub acknowledge: bool,
    /// Stays active until acknowledged even if the condition clears.
    #[serde(default, skip_serializing_if = "is_false")]
    pub latched: bool,
}

/// A recipe definition: a set of tags written together.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecipeDefinition {
    /// Stable identifier.
    pub id: RecipeId,
    /// Display name.
    pub name: LocalizedText,
    /// Description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Fields.
    pub fields: Vec<RecipeField>,
    /// Stored value sets.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sets: Vec<RecipeSet>,
}

/// A recipe field mapped to a writable tag.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecipeField {
    /// Stable identifier.
    pub id: RecipeFieldId,
    /// Display name.
    pub name: LocalizedText,
    /// Target tag (must be writable).
    pub tag: TagId,
    /// Member path into the tag.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub member: Option<String>,
}

/// A stored recipe value set.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecipeSet {
    /// Stable identifier.
    pub id: RecipeSetId,
    /// Display name.
    pub name: LocalizedText,
    /// Field id -> value.
    pub values: BTreeMap<RecipeFieldId, serde_json::Value>,
}
