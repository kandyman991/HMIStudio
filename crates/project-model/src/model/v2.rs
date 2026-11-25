//! Staged v2 industrial/security contracts. Not part of the active v1 project
//! document and never an authorization to connect, execute, or write.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{ConnectionId, DriverKind, NodeId, PermissionId, RoleId, ScreenId, TagId};

/// Candidate v2 driver vocabulary; the existing S7 spelling is retained.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum DriverKindV2 {
    Simulator,
    OpcUa,
    SiemensS7,
    ModbusTcp,
    Custom,
    /// In-process test transport only; no external endpoint or credentials.
    Loopback,
}
impl From<DriverKind> for DriverKindV2 {
    fn from(value: DriverKind) -> Self {
        match value {
            DriverKind::Simulator => Self::Simulator,
            DriverKind::OpcUa => Self::OpcUa,
            DriverKind::SiemensS7 => Self::SiemensS7,
            DriverKind::ModbusTcp => Self::ModbusTcp,
            DriverKind::Custom => Self::Custom,
        }
    }
}

/// Candidate connection metadata tied to an existing v1 connection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConnectionPolicy {
    pub connection: ConnectionId,
    pub driver: DriverKindV2,
    /// Engineering intent only; never active commissioning authorization.
    #[serde(default, skip_serializing_if = "super::common::is_false")]
    pub commissioning_allowed: bool,
}

/// Operator command semantics, distinct from tag read/write access.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum CommandKind {
    Momentary,
    Maintained,
    Setpoint,
}

/// Exact JSON-number declarations, distinct from the display range.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WriteLimits {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<serde_json::Number>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<serde_json::Number>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<serde_json::Number>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TagWritePolicy {
    pub tag: TagId,
    pub command: CommandKind,
    #[serde(default, skip_serializing_if = "super::common::is_false")]
    pub confirmation_required: bool,
    #[serde(default, skip_serializing_if = "super::common::is_false")]
    pub critical: bool,
    #[serde(default, skip_serializing_if = "super::common::is_false")]
    pub write_confirmed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pulse_ms: Option<u32>,
    #[serde(default, skip_serializing_if = "super::common::is_default")]
    pub limits: WriteLimits,
}

/// Permission and role declarations only; no identity or credential store.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Permission {
    pub id: PermissionId,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Role {
    pub id: RoleId,
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub permissions: Vec<PermissionId>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SecurityPolicy {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub permissions: Vec<Permission>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub roles: Vec<Role>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anonymous_role: Option<RoleId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_logout_seconds: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScreenPermission {
    pub screen: ScreenId,
    pub required_permission: PermissionId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NodePermission {
    pub node: NodeId,
    pub required_permission: PermissionId,
}

/// Staging representation for REUSE-03A. IDs refer to the existing project,
/// and declarations are validated together before future v2 integration.
/// Not a second editable project format or an accepted v2 wire schema.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IndustrialSecurityDeclarations {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub connections: Vec<ConnectionPolicy>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<TagWritePolicy>,
    #[serde(default, skip_serializing_if = "super::common::is_default")]
    pub security: SecurityPolicy,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub screens: Vec<ScreenPermission>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nodes: Vec<NodePermission>,
}
