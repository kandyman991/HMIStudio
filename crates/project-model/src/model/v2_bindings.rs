//! Staged enumeration and expression binding declarations for canonical v2.
use super::{DataType, EnumerationId, LocalizedText, NodeId, TagId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A discrete display mapping. This never changes tag write semantics.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnumerationEntry {
    pub value: serde_json::Value,
    pub label: LocalizedText,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnumerationDefinition {
    pub id: EnumerationId,
    pub name: String,
    pub data_type: DataType,
    pub entries: Vec<EnumerationEntry>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnumerationMapping {
    pub tag: TagId,
    pub enumeration: EnumerationId,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExpressionBinding {
    pub node: NodeId,
    pub property: String,
    pub contract_version: u32,
    pub expression: String,
}
/// Candidate declarations only, not a separately editable project format.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExpressionEnumerationDeclarations {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub enumerations: Vec<EnumerationDefinition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mappings: Vec<EnumerationMapping>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expressions: Vec<ExpressionBinding>,
}
/// Resolve a matching exact value; no fallback, no implicit conversion,
/// and never map an enumeration label back to an operator write.
impl EnumerationDefinition {
    pub fn lookup(&self, value: &serde_json::Value) -> Option<&EnumerationEntry> {
        self.entries.iter().find(|e| e.value == *value)
    }
}
