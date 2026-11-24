//! Themes, named styles and resources.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::common::{is_false, Extensions};
use super::component::ComponentKind;
use super::{ResourceId, StyleId, ThemeId};

/// A theme: named design tokens. Components refer to colors with
/// `{ "token": "<name>" }` so a project can switch between, e.g., day and
/// night themes without touching screens.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Theme {
    /// Stable identifier.
    pub id: ThemeId,
    /// Display name.
    pub name: String,
    /// Dark theme hint for platform chrome.
    #[serde(default, skip_serializing_if = "is_false")]
    pub dark: bool,
    /// Color tokens: name -> `#RRGGBB` / `#RRGGBBAA`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub colors: BTreeMap<String, String>,
    /// Typography tokens.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub typography: BTreeMap<String, TextStyle>,
    /// Numeric tokens (corner radii, spacing scale, stroke widths) in dp.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metrics: BTreeMap<String, f64>,
}

/// A typography token.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextStyle {
    /// Platform font family name (fallback when `font` is unset/unavailable).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_family: Option<String>,
    /// Bundled font resource.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font: Option<ResourceId>,
    /// Size in sp (> 0).
    pub size: f64,
    /// Weight, 100..=900.
    #[serde(default = "default_weight", skip_serializing_if = "is_default_weight")]
    pub weight: u16,
    /// Italic.
    #[serde(default, skip_serializing_if = "is_false")]
    pub italic: bool,
    /// Line height in sp.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_height: Option<f64>,
}

fn default_weight() -> u16 {
    400
}

fn is_default_weight(v: &u16) -> bool {
    *v == 400
}

/// A reusable set of property values.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Style {
    /// Stable identifier.
    pub id: StyleId,
    /// Display name.
    pub name: String,
    /// Component kind this style is for. Unset = common properties only,
    /// usable on any node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<ComponentKind>,
    /// Property values (validated like node properties).
    #[serde(default)]
    pub properties: BTreeMap<String, serde_json::Value>,
}

/// Resource kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum ResourceKind {
    /// Raster image (PNG, JPEG, WebP).
    Image,
    /// Vector image (SVG subset / Android vector drawable).
    Vector,
    /// Font file.
    Font,
    /// Arbitrary data file.
    Data,
}

/// A project asset. Bytes are stored next to the project document (or in
/// the bundle) at `path`; the model only references them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Resource {
    /// Stable identifier.
    pub id: ResourceId,
    /// Resource kind.
    pub kind: ResourceKind,
    /// Relative, forward-slash path inside the project package
    /// (no `..`, no absolute paths, no URL schemes).
    pub path: String,
    /// IANA media type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    /// Lowercase hex SHA-256 of the bytes, for integrity checks on publish.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    /// Extensible metadata.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: Extensions,
}
