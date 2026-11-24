//! Semantic, hierarchical layout tree.
//!
//! A screen (or a reusable component definition) is a tree of [`Node`]s.
//! Every node has an [`Element`] that says *what* it is (a row, a grid, a
//! button, an instance of a reusable component, ...) and [`LayoutParams`]
//! that say how it is sized and placed *by its parent*. The vocabulary is
//! deliberately the intersection of what a Figma-like editor manipulates
//! (auto-layout frames, constraints, absolute frames) and what Jetpack /
//! Compose Multiplatform can express natively (`Row`, `Column`, `Box`,
//! `Modifier.weight`, `Modifier.size`, scroll modifiers, `Layout`).
//!
//! All lengths are density-independent pixels (dp in Compose, CSS px in the
//! HTML editor at a 1:1 scale).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::common::{is_default, is_zero_f64, is_zero_i32, Extensions};
use super::component::{ComponentKind, EventHandler, PropertyBinding};
use super::{BreakpointId, ComponentDefinitionId, DeviceProfileId, NodeId, StyleId};
use crate::model::common::is_false;
use crate::model::project::{Orientation, Platform};
use std::collections::BTreeMap;

/// One element of a layout tree.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Node {
    /// Stable identifier.
    pub id: NodeId,
    /// Optional designer-facing name (layer name). Never used for references.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// What this node is.
    pub element: Element,
    /// How the parent sizes and places this node.
    #[serde(default, skip_serializing_if = "is_default")]
    pub layout: LayoutParams,
    /// Child nodes, in paint order (later children are drawn above earlier
    /// ones unless `layout.zIndex` says otherwise).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Node>,
    /// Named style applied before local properties.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<StyleId>,
    /// Typed properties (validated against the component catalog or the
    /// parameters of a component definition).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub properties: BTreeMap<String, serde_json::Value>,
    /// Live data bindings for properties.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bindings: Vec<PropertyBinding>,
    /// Event handlers.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<EventHandler>,
    /// Responsive overrides, applied in order; later matching rules win.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub responsive: Vec<ResponsiveRule>,
    /// Extensible engineering metadata.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: Extensions,
}

impl Node {
    /// Creates a node with default layout and no children.
    pub fn new(id: NodeId, element: Element) -> Self {
        Self {
            id,
            name: None,
            element,
            layout: LayoutParams::default(),
            children: Vec::new(),
            style: None,
            properties: BTreeMap::new(),
            bindings: Vec::new(),
            events: Vec::new(),
            responsive: Vec::new(),
            extensions: Extensions::new(),
        }
    }

    /// Visits this node and all descendants depth-first, pre-order.
    pub fn walk<'a>(&'a self, f: &mut dyn FnMut(&'a Node)) {
        f(self);
        for child in &self.children {
            child.walk(f);
        }
    }
}

/// The semantic kind of a node.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum Element {
    /// Horizontal flow (Compose `Row`, Figma horizontal auto-layout).
    #[serde(rename_all = "camelCase")]
    Row {
        /// Main-axis arrangement.
        #[serde(default, skip_serializing_if = "is_default")]
        arrangement: Arrangement,
        /// Cross-axis alignment of children.
        #[serde(default, skip_serializing_if = "is_default")]
        alignment: CrossAlignment,
        /// Gap between children (dp). Ignored by space-* arrangements.
        #[serde(default, skip_serializing_if = "is_zero_f64")]
        spacing: f64,
        /// Wrap children onto new lines (Compose `FlowRow`).
        #[serde(default, skip_serializing_if = "is_false")]
        wrap: bool,
    },
    /// Vertical flow (Compose `Column`, Figma vertical auto-layout).
    #[serde(rename_all = "camelCase")]
    Column {
        /// Main-axis arrangement.
        #[serde(default, skip_serializing_if = "is_default")]
        arrangement: Arrangement,
        /// Cross-axis alignment of children.
        #[serde(default, skip_serializing_if = "is_default")]
        alignment: CrossAlignment,
        /// Gap between children (dp).
        #[serde(default, skip_serializing_if = "is_zero_f64")]
        spacing: f64,
        /// Wrap children onto new columns (Compose `FlowColumn`).
        #[serde(default, skip_serializing_if = "is_false")]
        wrap: bool,
    },
    /// Stack / overlay (Compose `Box`). Children are layered.
    #[serde(rename_all = "camelCase")]
    Box {
        /// Default alignment of children inside the box.
        #[serde(default, skip_serializing_if = "is_default")]
        content_alignment: Alignment,
    },
    /// Two-dimensional track grid. Children must declare `layout.gridCell`.
    #[serde(rename_all = "camelCase")]
    Grid {
        /// Column tracks (at least one).
        columns: Vec<Track>,
        /// Row tracks (at least one).
        rows: Vec<Track>,
        /// Gap between columns (dp).
        #[serde(default, skip_serializing_if = "is_zero_f64")]
        column_gap: f64,
        /// Gap between rows (dp).
        #[serde(default, skip_serializing_if = "is_zero_f64")]
        row_gap: f64,
    },
    /// Scrollable viewport with exactly one child.
    #[serde(rename_all = "camelCase")]
    Scroll {
        /// Scroll direction(s).
        #[serde(default, skip_serializing_if = "is_default")]
        direction: ScrollDirection,
    },
    /// Free-form canvas. Children must declare `layout.position`. Use only
    /// where explicit coordinates are required (P&ID mimics, overlays on
    /// machine pictures); prefer flow containers elsewhere.
    #[serde(rename_all = "camelCase")]
    Absolute {},
    /// A built-in component from the catalog.
    #[serde(rename_all = "camelCase")]
    Component {
        /// Component kind.
        kind: ComponentKind,
    },
    /// An instance of a reusable [`crate::model::ComponentDefinition`].
    #[serde(rename_all = "camelCase")]
    Instance {
        /// Referenced definition.
        definition: ComponentDefinitionId,
        /// Parameter values (validated against the definition parameters).
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        parameters: BTreeMap<String, serde_json::Value>,
    },
}

impl Element {
    /// Stable kind name used in diagnostics.
    pub fn type_name(&self) -> &'static str {
        match self {
            Element::Row { .. } => "row",
            Element::Column { .. } => "column",
            Element::Box { .. } => "box",
            Element::Grid { .. } => "grid",
            Element::Scroll { .. } => "scroll",
            Element::Absolute {} => "absolute",
            Element::Component { .. } => "component",
            Element::Instance { .. } => "instance",
        }
    }

    /// Whether this is a pure layout container.
    pub fn is_layout_container(&self) -> bool {
        matches!(
            self,
            Element::Row { .. }
                | Element::Column { .. }
                | Element::Box { .. }
                | Element::Grid { .. }
                | Element::Scroll { .. }
                | Element::Absolute {}
        )
    }
}

/// Main-axis arrangement for rows and columns.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Arrangement {
    /// Pack at start.
    #[default]
    Start,
    /// Pack at center.
    Center,
    /// Pack at end.
    End,
    /// Distribute with space between.
    SpaceBetween,
    /// Distribute with space around.
    SpaceAround,
    /// Distribute evenly.
    SpaceEvenly,
}

/// Cross-axis alignment for rows and columns.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum CrossAlignment {
    /// Top (row) / start (column).
    #[default]
    Start,
    /// Centered.
    Center,
    /// Bottom (row) / end (column).
    End,
}

/// Two-dimensional alignment (Compose `Alignment`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Alignment {
    /// Top start.
    #[default]
    TopStart,
    /// Top center.
    TopCenter,
    /// Top end.
    TopEnd,
    /// Center start.
    CenterStart,
    /// Center.
    Center,
    /// Center end.
    CenterEnd,
    /// Bottom start.
    BottomStart,
    /// Bottom center.
    BottomCenter,
    /// Bottom end.
    BottomEnd,
}

/// Scroll direction.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum ScrollDirection {
    /// Vertical scrolling.
    #[default]
    Vertical,
    /// Horizontal scrolling.
    Horizontal,
    /// Both axes.
    Both,
}

/// Flow axis, used by responsive overrides to switch a row into a column.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Axis {
    /// Horizontal (row).
    Horizontal,
    /// Vertical (column).
    Vertical,
}

/// Size along one axis.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "mode", rename_all = "camelCase", deny_unknown_fields)]
pub enum Size {
    /// Wrap content (Compose `wrapContentSize`, Figma "hug").
    #[serde(rename_all = "camelCase")]
    Auto {},
    /// Fill the available space (Compose `fillMaxWidth(fraction)`,
    /// Figma "fill container").
    #[serde(rename_all = "camelCase")]
    Fill {
        /// Fraction of the available space in `(0, 1]`.
        #[serde(default = "one", skip_serializing_if = "is_one")]
        fraction: f64,
    },
    /// Fixed size in dp.
    #[serde(rename_all = "camelCase")]
    Fixed {
        /// Size in dp (>= 0).
        value: f64,
    },
}

impl Default for Size {
    fn default() -> Self {
        Size::Auto {}
    }
}

impl Size {
    /// `Auto` size.
    pub fn auto() -> Self {
        Size::Auto {}
    }
    /// Full fill.
    pub fn fill() -> Self {
        Size::Fill { fraction: 1.0 }
    }
    /// Fixed size.
    pub fn fixed(value: f64) -> Self {
        Size::Fixed { value }
    }
}

fn one() -> f64 {
    1.0
}

fn is_one(v: &f64) -> bool {
    *v == 1.0
}

/// A grid track.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "mode", rename_all = "camelCase", deny_unknown_fields)]
pub enum Track {
    /// Fixed track size in dp.
    #[serde(rename_all = "camelCase")]
    Fixed {
        /// Size in dp (> 0).
        value: f64,
    },
    /// Size to content.
    #[serde(rename_all = "camelCase")]
    Auto {},
    /// Share of the remaining space (CSS `fr`, Compose weight).
    #[serde(rename_all = "camelCase")]
    Fraction {
        /// Relative weight (> 0).
        weight: f64,
    },
}

/// Padding / insets in dp.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Insets {
    /// Top inset.
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub top: f64,
    /// End (right in LTR) inset.
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub end: f64,
    /// Bottom inset.
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub bottom: f64,
    /// Start (left in LTR) inset.
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub start: f64,
}

impl Insets {
    /// Equal insets on all sides.
    pub fn all(v: f64) -> Self {
        Self {
            top: v,
            end: v,
            bottom: v,
            start: v,
        }
    }
}

/// Explicit coordinates inside an [`Element::Absolute`] parent (dp, from
/// the parent's top-start corner).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Position {
    /// X offset.
    pub x: f64,
    /// Y offset.
    pub y: f64,
}

/// Placement inside an [`Element::Grid`] parent. Indices are zero-based.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GridCell {
    /// Column index.
    pub column: u32,
    /// Row index.
    pub row: u32,
    /// Number of columns spanned (>= 1).
    #[serde(default = "one_u32", skip_serializing_if = "is_one_u32")]
    pub column_span: u32,
    /// Number of rows spanned (>= 1).
    #[serde(default = "one_u32", skip_serializing_if = "is_one_u32")]
    pub row_span: u32,
}

fn one_u32() -> u32 {
    1
}

fn is_one_u32(v: &u32) -> bool {
    *v == 1
}

/// Visibility state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Visibility {
    /// Laid out and drawn.
    #[default]
    Visible,
    /// Laid out (occupies space) but not drawn and not interactive.
    Hidden,
    /// Not laid out at all.
    Collapsed,
}

/// Parameters a parent uses to size and place a child.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LayoutParams {
    /// Width mode.
    #[serde(default, skip_serializing_if = "is_default")]
    pub width: Size,
    /// Height mode.
    #[serde(default, skip_serializing_if = "is_default")]
    pub height: Size,
    /// Minimum width (dp).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_width: Option<f64>,
    /// Maximum width (dp).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_width: Option<f64>,
    /// Minimum height (dp).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_height: Option<f64>,
    /// Maximum height (dp).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_height: Option<f64>,
    /// Width / height ratio (> 0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aspect_ratio: Option<f64>,
    /// Inner padding.
    #[serde(default, skip_serializing_if = "is_default")]
    pub padding: Insets,
    /// Flex weight along the parent row/column main axis (> 0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<f64>,
    /// Cross-axis self alignment inside a row/column parent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cross_align: Option<CrossAlignment>,
    /// Self alignment inside a box parent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<Alignment>,
    /// Coordinates inside an absolute parent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<Position>,
    /// Cell inside a grid parent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grid_cell: Option<GridCell>,
    /// Static visibility (may be overridden by a `visible` binding).
    #[serde(default, skip_serializing_if = "is_default")]
    pub visibility: Visibility,
    /// Clip content to bounds.
    #[serde(default, skip_serializing_if = "is_false")]
    pub clip: bool,
    /// Paint order among siblings; higher draws above. Ties keep child order.
    #[serde(default, skip_serializing_if = "is_zero_i32")]
    pub z_index: i32,
}

/// A responsive override rule.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResponsiveRule {
    /// When the rule applies. All present criteria must match.
    pub when: ResponsiveCondition,
    /// What changes when it applies.
    pub set: LayoutOverride,
}

/// Criteria for a responsive rule. At least one criterion is required.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResponsiveCondition {
    /// Named breakpoint (window width class).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub breakpoint: Option<BreakpointId>,
    /// Minimum available window width (dp, inclusive).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_width: Option<f64>,
    /// Maximum available window width (dp, inclusive).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_width: Option<f64>,
    /// Device orientation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orientation: Option<Orientation>,
    /// Runtime platform.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<Platform>,
    /// Specific device profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_profile: Option<DeviceProfileId>,
}

impl ResponsiveCondition {
    /// Whether no criterion is set.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Overrides applied by a responsive rule. Unset fields are unchanged.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LayoutOverride {
    /// Width.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<Size>,
    /// Height.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<Size>,
    /// Minimum width.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_width: Option<f64>,
    /// Maximum width.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_width: Option<f64>,
    /// Minimum height.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_height: Option<f64>,
    /// Maximum height.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_height: Option<f64>,
    /// Padding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub padding: Option<Insets>,
    /// Weight.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<f64>,
    /// Visibility.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visibility: Option<Visibility>,
    /// Row/column spacing (row and column elements only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spacing: Option<f64>,
    /// Switch a row/column flow axis (row and column elements only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<Axis>,
    /// Replace grid column tracks (grid elements only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub columns: Option<Vec<Track>>,
    /// Replace grid placement (children of grids only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grid_cell: Option<GridCell>,
    /// Replace absolute position (children of absolute parents only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<Position>,
    /// Z-order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub z_index: Option<i32>,
}

impl LayoutOverride {
    /// Whether nothing is overridden.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}
