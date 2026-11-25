//! Canonical v2 declaration groups owned by the single Project snapshot.
use super::{
    v2::IndustrialSecurityDeclarations, v2_bindings::ExpressionEnumerationDeclarations,
    v2_components::ComponentActionDeclarations,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct V2Declarations {
    #[serde(default, skip_serializing_if = "super::common::is_default")]
    pub industrial: IndustrialSecurityDeclarations,
    #[serde(default, skip_serializing_if = "super::common::is_default")]
    pub bindings: ExpressionEnumerationDeclarations,
    #[serde(default, skip_serializing_if = "super::common::is_default")]
    pub components: ComponentActionDeclarations,
}
impl V2Declarations {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}
