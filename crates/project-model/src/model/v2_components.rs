//! Staged v2 component and action contracts. No rendering or execution.
use super::{
    Action, ComponentDefinitionId, EnumerationId, EventKind, LocalizedText, NodeId, ScreenId, TagId,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Industrial symbol identity; symbols are reusable component definitions,
/// not opaque renderer-specific components.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum IndustrialSymbolKind {
    Motor,
    Valve,
    Conveyor,
    Sensor,
    Pump,
    Tank,
    Pipe,
    ThermalIndicator,
    FlowIndicator,
}
impl IndustrialSymbolKind {
    pub const ALL: [Self; 9] = [
        Self::Motor,
        Self::Valve,
        Self::Conveyor,
        Self::Sensor,
        Self::Pump,
        Self::Tank,
        Self::Pipe,
        Self::ThermalIndicator,
        Self::FlowIndicator,
    ];
    /// Required read-only tag parameter names and their expected value family.
    pub fn slots(self) -> &'static [(&'static str, SymbolSlotType)] {
        use SymbolSlotType::*;
        match self {
            Self::Motor | Self::Conveyor | Self::Pump => {
                &[("running", Boolean), ("fault", Boolean)]
            }
            Self::Valve => &[("open", Boolean), ("fault", Boolean)],
            Self::Sensor => &[("active", Boolean)],
            Self::Tank => &[("level", Numeric)],
            Self::Pipe => &[("flowing", Boolean)],
            Self::ThermalIndicator => &[("temperature", Numeric)],
            Self::FlowIndicator => &[("flow", Numeric)],
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SymbolSlotType {
    Boolean,
    Numeric,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum DefinitionKind {
    Faceplate {},
    #[serde(rename_all = "camelCase")]
    IndustrialSymbol {
        symbol: IndustrialSymbolKind,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DefinitionClassification {
    pub definition: ComponentDefinitionId,
    pub classification: DefinitionKind,
}

/// Coordinates are local dp within a future component's layout box.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// Missing primitive and input contracts. Values are engineering declarations;
/// the future canonical node integration must preserve them and bind them
/// through validated operations, not interpret them in the v1 renderer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum ComponentContract {
    #[serde(rename_all = "camelCase")]
    Line {
        start: Point,
        end: Point,
        stroke_color: String,
        stroke_width: f64,
    },
    #[serde(rename_all = "camelCase")]
    Arrow {
        start: Point,
        end: Point,
        stroke_color: String,
        stroke_width: f64,
        head_length: f64,
    },
    #[serde(rename_all = "camelCase")]
    Polygon {
        points: Vec<Point>,
        fill: String,
        stroke_color: String,
        stroke_width: f64,
    },
    #[serde(rename_all = "camelCase")]
    TextInput {
        tag: TagId,
        max_length: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<LocalizedText>,
    },
    #[serde(rename_all = "camelCase")]
    DropDown {
        tag: TagId,
        enumeration: EnumerationId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<LocalizedText>,
    },
}
impl ComponentContract {
    pub fn kind(&self) -> super::ComponentKind {
        use super::ComponentKind as K;
        match self {
            Self::Line { .. } => K::Line,
            Self::Arrow { .. } => K::Arrow,
            Self::Polygon { .. } => K::Polygon,
            Self::TextInput { .. } => K::TextInput,
            Self::DropDown { .. } => K::DropDown,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComponentCandidate {
    pub id: NodeId,
    pub contract: ComponentContract,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum NewAction {
    NavigateStartup {},
    #[serde(rename_all = "camelCase")]
    OpenFaceplate {
        definition: ComponentDefinitionId,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        parameters: BTreeMap<String, serde_json::Value>,
    },
}
impl NewAction {
    /// Resolves the current startup page at the time the action is planned.
    /// This only returns a reference; it does not perform navigation.
    pub fn startup_target<'a>(&self, project: &'a super::Project) -> Option<&'a ScreenId> {
        if !matches!(self, Self::NavigateStartup {}) {
            return None;
        }
        project
            .screens
            .iter()
            .find(|screen| {
                screen.id == project.navigation.start_screen
                    && screen.kind == super::ScreenKind::Page
            })
            .map(|screen| &screen.id)
    }
}

/// Explicit union preserves ordering of old and new actions without rewriting
/// a dynamic startup navigation to a fixed screen id.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum CandidateAction {
    New(NewAction),
    Existing(Action),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum EventOrigin {
    Screen(ScreenId),
    Node(NodeId),
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CandidateHandler {
    pub origin: EventOrigin,
    pub event: EventKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirm: Option<LocalizedText>,
    pub actions: Vec<CandidateAction>,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComponentActionDeclarations {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub definitions: Vec<DefinitionClassification>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<ComponentCandidate>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub handlers: Vec<CandidateHandler>,
}
