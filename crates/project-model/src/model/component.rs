//! Components, reusable definitions, bindings, events and actions.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::common::{is_default, is_false, Extensions, LocalizedText};
use super::industrial::DataType;
use super::layout::Node;
use super::{AlarmId, ComponentDefinitionId, RecipeId, RecipeSetId, ScreenId, TagId};

/// Built-in component kinds. The property catalog for each kind lives in
/// [`crate::catalog`].
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub enum ComponentKind {
    /// Push button.
    Button,
    /// Static or bound text.
    Text,
    /// Raster or vector image.
    Image,
    /// Rectangle shape.
    Rectangle,
    /// Ellipse / circle shape.
    Ellipse,
    /// Read-only numeric value.
    NumericDisplay,
    /// Operator numeric entry.
    NumericInput,
    /// Status lamp / LED.
    Indicator,
    /// Analog gauge.
    Gauge,
    /// Slider.
    Slider,
    /// On/off switch.
    Switch,
    /// Progress / level bar.
    ProgressBar,
    /// Decorated surface that may host one child layout.
    Container,
}

impl ComponentKind {
    /// All kinds in declaration order.
    pub const ALL: [ComponentKind; 13] = [
        ComponentKind::Button,
        ComponentKind::Text,
        ComponentKind::Image,
        ComponentKind::Rectangle,
        ComponentKind::Ellipse,
        ComponentKind::NumericDisplay,
        ComponentKind::NumericInput,
        ComponentKind::Indicator,
        ComponentKind::Gauge,
        ComponentKind::Slider,
        ComponentKind::Switch,
        ComponentKind::ProgressBar,
        ComponentKind::Container,
    ];

    /// JSON name of the kind.
    pub fn as_str(self) -> &'static str {
        match self {
            ComponentKind::Button => "button",
            ComponentKind::Text => "text",
            ComponentKind::Image => "image",
            ComponentKind::Rectangle => "rectangle",
            ComponentKind::Ellipse => "ellipse",
            ComponentKind::NumericDisplay => "numericDisplay",
            ComponentKind::NumericInput => "numericInput",
            ComponentKind::Indicator => "indicator",
            ComponentKind::Gauge => "gauge",
            ComponentKind::Slider => "slider",
            ComponentKind::Switch => "switch",
            ComponentKind::ProgressBar => "progressBar",
            ComponentKind::Container => "container",
        }
    }

    /// Maximum number of children this kind accepts.
    pub fn max_children(self) -> usize {
        match self {
            ComponentKind::Container => 1,
            _ => 0,
        }
    }
}

/// Type of a property or component parameter.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum PropertyType {
    /// `true` / `false`.
    #[serde(rename_all = "camelCase")]
    Bool {},
    /// Any finite number, optionally bounded.
    #[serde(rename_all = "camelCase")]
    Number {
        /// Inclusive minimum.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<f64>,
        /// Inclusive maximum.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<f64>,
    },
    /// Integer, optionally bounded.
    #[serde(rename_all = "camelCase")]
    Integer {
        /// Inclusive minimum.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<i64>,
        /// Inclusive maximum.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<i64>,
    },
    /// Plain, non-translated string.
    #[serde(rename_all = "camelCase")]
    String {},
    /// Operator text: literal string or `{ "textKey": ... }`.
    #[serde(rename_all = "camelCase")]
    Text {},
    /// Color: `#RRGGBB`, `#RRGGBBAA` or `{ "token": "<theme color>" }`.
    #[serde(rename_all = "camelCase")]
    Color {},
    /// Resource identifier, optionally restricted to some kinds.
    #[serde(rename_all = "camelCase")]
    Resource {
        /// Accepted resource kinds; empty = any.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        kinds: Vec<super::style::ResourceKind>,
    },
    /// One of a fixed set of strings.
    #[serde(rename_all = "camelCase")]
    Enum {
        /// Allowed values.
        options: Vec<String>,
    },
    /// A tag reference (component definition parameters only). Lets a
    /// reusable faceplate be wired to different tags per instance.
    #[serde(rename_all = "camelCase")]
    Tag {
        /// Required data type of the supplied tag, if constrained.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        data_type: Option<DataType>,
        /// Whether the supplied tag must be writable.
        #[serde(default, skip_serializing_if = "is_false")]
        writable: bool,
    },
}

/// Parameter of a reusable component definition.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ParameterDefinition {
    /// Parameter name (`[A-Za-z][A-Za-z0-9_]*`), unique per definition.
    pub name: String,
    /// Parameter type.
    #[serde(rename = "type")]
    pub ty: PropertyType,
    /// Default value. Parameters without a default must be supplied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    /// Engineer-facing description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// A reusable component (faceplate, symbol, composite widget).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComponentDefinition {
    /// Stable identifier.
    pub id: ComponentDefinitionId,
    /// Display name.
    pub name: String,
    /// Description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Library category for the editor palette.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// Parameters exposed to instances.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<ParameterDefinition>,
    /// Root of the definition's layout tree.
    pub root: Node,
    /// Extensible engineering metadata.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: Extensions,
}

/// Binding data flow.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum BindingMode {
    /// Source -> property.
    #[default]
    Read,
    /// Property -> source (operator input writes the tag).
    Write,
    /// Both directions.
    ReadWrite,
}

impl BindingMode {
    /// Whether the binding writes to its source.
    pub fn writes(self) -> bool {
        matches!(self, BindingMode::Write | BindingMode::ReadWrite)
    }
    /// Whether the binding reads from its source.
    pub fn reads(self) -> bool {
        matches!(self, BindingMode::Read | BindingMode::ReadWrite)
    }
}

/// Where a binding takes its value from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum BindingSource {
    /// A project tag: `{ "tag": "<tagId>" }`.
    Tag(TagId),
    /// The tag supplied to a `tag`-typed parameter of the enclosing
    /// component definition: `{ "tagParameter": "<name>" }`.
    TagParameter(String),
    /// A value parameter of the enclosing component definition:
    /// `{ "parameter": "<name>" }` (read-only).
    Parameter(String),
}

/// A tag target for actions.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum TagRef {
    /// A project tag.
    Tag(TagId),
    /// A `tag`-typed parameter of the enclosing component definition.
    TagParameter(String),
}

/// Binds a node property (or, on instances, a value parameter) to data.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PropertyBinding {
    /// Target property name.
    pub property: String,
    /// Data source.
    pub source: BindingSource,
    /// Member path into a structured/array tag, e.g. `status.running` or
    /// `values[3]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub member: Option<String>,
    /// Data flow direction.
    #[serde(default, skip_serializing_if = "is_default")]
    pub mode: BindingMode,
    /// Value conversion between source and property.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<ValueTransform>,
}

/// Value conversion applied by a binding.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum ValueTransform {
    /// `property = source * factor + offset` (invertible if factor != 0).
    #[serde(rename_all = "camelCase")]
    Linear {
        /// Multiplier.
        factor: f64,
        /// Offset.
        #[serde(default, skip_serializing_if = "super::common::is_zero_f64")]
        offset: f64,
    },
    /// Boolean negation (invertible).
    #[serde(rename_all = "camelCase")]
    Invert {},
    /// Extract one bit of an integer source as a boolean (read-only).
    #[serde(rename_all = "camelCase")]
    Bit {
        /// Bit index, 0 = least significant.
        index: u8,
    },
    /// Map discrete source values to property values (read-only).
    #[serde(rename_all = "camelCase")]
    Map {
        /// Ordered entries; first match wins.
        entries: Vec<MapEntry>,
        /// Value when nothing matches.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default: Option<serde_json::Value>,
    },
}

/// One entry of a [`ValueTransform::Map`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MapEntry {
    /// Source value to match.
    pub when: serde_json::Value,
    /// Resulting property value.
    pub value: serde_json::Value,
}

/// Event raised by a node or screen.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub enum EventKind {
    /// Tap / click.
    Click,
    /// Pointer pressed.
    Press,
    /// Pointer released.
    Release,
    /// Long press.
    LongPress,
    /// Operator committed a new value.
    ValueChanged,
    /// Screen became visible (screens only).
    Show,
    /// Screen was hidden (screens only).
    Hide,
}

/// Event handler: an ordered list of actions.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EventHandler {
    /// Triggering event.
    pub event: EventKind,
    /// Optional operator confirmation prompt shown before running actions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirm: Option<LocalizedText>,
    /// Actions, executed in order.
    pub actions: Vec<Action>,
}

/// Declarative action contract. Execution is out of scope for this crate.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum Action {
    /// Navigate to a page.
    #[serde(rename_all = "camelCase")]
    Navigate {
        /// Target page.
        screen: ScreenId,
    },
    /// Navigate back.
    #[serde(rename_all = "camelCase")]
    Back {},
    /// Open a popup screen.
    #[serde(rename_all = "camelCase")]
    OpenPopup {
        /// Target popup.
        screen: ScreenId,
    },
    /// Close the topmost popup.
    #[serde(rename_all = "camelCase")]
    ClosePopup {},
    /// Write a constant value.
    #[serde(rename_all = "camelCase")]
    WriteTag {
        /// Target tag.
        target: TagRef,
        /// Member path.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        member: Option<String>,
        /// Value to write.
        value: serde_json::Value,
    },
    /// Invert a boolean.
    #[serde(rename_all = "camelCase")]
    ToggleTag {
        /// Target tag.
        target: TagRef,
        /// Member path.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        member: Option<String>,
    },
    /// Add `step` to a numeric value, clamped to `[min, max]` if given.
    #[serde(rename_all = "camelCase")]
    StepTag {
        /// Target tag.
        target: TagRef,
        /// Member path.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        member: Option<String>,
        /// Increment (negative to decrement).
        step: f64,
        /// Lower clamp.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<f64>,
        /// Upper clamp.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<f64>,
    },
    /// Set a boolean true for `durationMs`, then false.
    #[serde(rename_all = "camelCase")]
    PulseTag {
        /// Target tag.
        target: TagRef,
        /// Member path.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        member: Option<String>,
        /// Pulse duration in milliseconds (> 0).
        duration_ms: u32,
    },
    /// Acknowledge alarms (one alarm, one group, or all when both are unset).
    #[serde(rename_all = "camelCase")]
    AcknowledgeAlarms {
        /// Single alarm.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        alarm: Option<AlarmId>,
        /// Alarm group.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        group: Option<String>,
    },
    /// Download a recipe set to its tags.
    #[serde(rename_all = "camelCase")]
    LoadRecipe {
        /// Recipe.
        recipe: RecipeId,
        /// Set; operator chooses when unset.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        set: Option<RecipeSetId>,
    },
    /// Upload current tag values into a recipe set.
    #[serde(rename_all = "camelCase")]
    SaveRecipe {
        /// Recipe.
        recipe: RecipeId,
        /// Set; operator chooses when unset.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        set: Option<RecipeSetId>,
    },
    /// Change the UI language.
    #[serde(rename_all = "camelCase")]
    SetLocale {
        /// Declared locale.
        locale: String,
    },
}

impl Action {
    /// Stable action name used in diagnostics.
    pub fn type_name(&self) -> &'static str {
        match self {
            Action::Navigate { .. } => "navigate",
            Action::Back {} => "back",
            Action::OpenPopup { .. } => "openPopup",
            Action::ClosePopup {} => "closePopup",
            Action::WriteTag { .. } => "writeTag",
            Action::ToggleTag { .. } => "toggleTag",
            Action::StepTag { .. } => "stepTag",
            Action::PulseTag { .. } => "pulseTag",
            Action::AcknowledgeAlarms { .. } => "acknowledgeAlarms",
            Action::LoadRecipe { .. } => "loadRecipe",
            Action::SaveRecipe { .. } => "saveRecipe",
            Action::SetLocale { .. } => "setLocale",
        }
    }
}
