//! Project root, metadata, device profiles, screens, navigation and
//! localization.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::common::{is_default, Extensions, LocalizedText};
use super::component::{ComponentDefinition, EventHandler};
use super::industrial::{
    AlarmDefinition, Connection, EngineeringUnit, RecipeDefinition, StructType, Tag,
};
use super::layout::Node;
use super::style::{Resource, Style, Theme};
use super::{
    BreakpointId, DeviceProfileId, NavigationItemId, ProjectId, ResourceId, ScreenId, ThemeId,
};
use crate::{CURRENT_SCHEMA_VERSION, FORMAT_ID};

/// The root of an HMI Studio project document.
///
/// Field order is the canonical serialization order.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(title = "HMI Studio project")]
pub struct Project {
    /// Format marker; always [`FORMAT_ID`].
    pub format: String,
    /// Schema version of this document.
    pub schema_version: u32,
    /// Stable project identifier.
    pub id: ProjectId,
    /// Descriptive metadata.
    pub metadata: ProjectMetadata,
    /// Project-wide defaults.
    #[serde(default, skip_serializing_if = "is_default")]
    pub settings: ProjectSettings,
    /// Navigation model.
    pub navigation: Navigation,
    /// Target device / resolution profiles.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub device_profiles: Vec<DeviceProfile>,
    /// Responsive width breakpoints.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub breakpoints: Vec<Breakpoint>,
    /// Themes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub themes: Vec<Theme>,
    /// Named styles.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub styles: Vec<Style>,
    /// Assets.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resources: Vec<Resource>,
    /// Localization tables.
    pub localization: Localization,
    /// Engineering units.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub units: Vec<EngineeringUnit>,
    /// User-defined structured data types.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub data_types: Vec<StructType>,
    /// PLC / data-source connections.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub connections: Vec<Connection>,
    /// Tags.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<Tag>,
    /// Alarm definitions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alarms: Vec<AlarmDefinition>,
    /// Recipe definitions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recipes: Vec<RecipeDefinition>,
    /// Reusable component definitions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<ComponentDefinition>,
    /// Screens and popups.
    pub screens: Vec<Screen>,
    /// Typed version-2 declarations, normalized by stable project identifiers.
    #[serde(default, skip_serializing_if = "is_default")]
    pub v2: super::v2_project::V2Declarations,
    /// Extensible engineering metadata.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: Extensions,
}

impl Project {
    /// Detect v2-only data even in an invalid project with a missing contract.
    pub fn has_v2_features(&self) -> bool {
        if !self.v2.is_empty()
            || self.styles.iter().any(|s| {
                s.target
                    .is_some_and(super::ComponentKind::requires_v2_contract)
            })
        {
            return true;
        }
        let mut found = false;
        for root in self
            .screens
            .iter()
            .map(|s| &s.root)
            .chain(self.components.iter().map(|c| &c.root))
        {
            root.walk(&mut |n| {
                if matches!(n.element, super::Element::Component { kind } if kind.requires_v2_contract()) {
                    found = true;
                }
            });
        }
        found
    }

    /// Creates an empty current-version project with one empty page.
    pub fn new(id: ProjectId, name: impl Into<String>, start_screen: Screen) -> Self {
        Self {
            format: FORMAT_ID.to_owned(),
            schema_version: CURRENT_SCHEMA_VERSION,
            id,
            metadata: ProjectMetadata {
                name: name.into(),
                ..ProjectMetadata::default()
            },
            settings: ProjectSettings::default(),
            navigation: Navigation {
                start_screen: start_screen.id.clone(),
                items: Vec::new(),
            },
            device_profiles: Vec::new(),
            breakpoints: Vec::new(),
            themes: Vec::new(),
            styles: Vec::new(),
            resources: Vec::new(),
            localization: Localization::default(),
            units: Vec::new(),
            data_types: Vec::new(),
            connections: Vec::new(),
            tags: Vec::new(),
            alarms: Vec::new(),
            recipes: Vec::new(),
            components: Vec::new(),
            screens: vec![start_screen],
            v2: super::v2_project::V2Declarations::default(),
            extensions: Extensions::new(),
        }
    }
}

/// Descriptive project metadata. Timestamps are supplied by the caller; the
/// model never stamps them implicitly, which keeps serialization
/// reproducible.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectMetadata {
    /// Project name.
    pub name: String,
    /// Description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Author / organization.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// Engineer-managed project revision (free text, e.g. `1.4.0`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    /// Creation time (RFC 3339).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    /// Last modification time (RFC 3339).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified: Option<String>,
    /// Search keywords.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
}

/// Project-wide defaults.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectSettings {
    /// Theme used at startup.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_theme: Option<ThemeId>,
    /// Profile used by the editor canvas when a screen has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_device_profile: Option<DeviceProfileId>,
}

/// Runtime platform.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Platform {
    /// Any platform.
    #[default]
    Any,
    /// Windows desktop / panel PC.
    Windows,
    /// Linux desktop / embedded panel.
    Linux,
    /// Android tablet / panel.
    Android,
}

/// Screen orientation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Orientation {
    /// Width >= height.
    #[default]
    Landscape,
    /// Height > width.
    Portrait,
}

/// A target device / resolution profile. Used for editor previews,
/// responsive rules and publishing; layouts remain responsive.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeviceProfile {
    /// Stable identifier.
    pub id: DeviceProfileId,
    /// Display name.
    pub name: String,
    /// Platform.
    #[serde(default, skip_serializing_if = "is_default")]
    pub platform: Platform,
    /// Physical resolution width in pixels.
    pub width_px: u32,
    /// Physical resolution height in pixels.
    pub height_px: u32,
    /// Pixels per dp (Android density; 1.0 = 160 dpi).
    pub density: f64,
    /// Orientation.
    #[serde(default, skip_serializing_if = "is_default")]
    pub orientation: Orientation,
    /// Touch input available.
    #[serde(default, skip_serializing_if = "super::common::is_false")]
    pub touch: bool,
}

/// A responsive breakpoint (window width class), in dp.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Breakpoint {
    /// Stable identifier.
    pub id: BreakpointId,
    /// Display name.
    pub name: String,
    /// Minimum width (inclusive).
    pub min_width: f64,
    /// Maximum width (exclusive); unset = unbounded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_width: Option<f64>,
}

/// Screen kind.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum ScreenKind {
    /// Full page, target of `navigate`.
    #[default]
    Page,
    /// Modal popup, target of `openPopup`.
    Popup,
}

/// A screen or popup.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Screen {
    /// Stable identifier.
    pub id: ScreenId,
    /// Engineer-facing name.
    pub name: String,
    /// Page or popup.
    #[serde(default, skip_serializing_if = "is_default")]
    pub kind: ScreenKind,
    /// Operator-facing title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<LocalizedText>,
    /// Profile used for design-time preview.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub design_profile: Option<DeviceProfileId>,
    /// Screen-level handlers (`show`, `hide`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<EventHandler>,
    /// Root of the layout tree.
    pub root: Node,
    /// Extensible metadata.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: Extensions,
}

/// Navigation model.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Navigation {
    /// Page shown at startup.
    pub start_screen: ScreenId,
    /// Main menu entries, in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<NavigationItem>,
}

/// A main-menu entry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NavigationItem {
    /// Stable identifier.
    pub id: NavigationItemId,
    /// Label.
    pub label: LocalizedText,
    /// Target page.
    pub screen: ScreenId,
    /// Icon resource.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<ResourceId>,
}

/// Localization tables.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Localization {
    /// Fallback locale (BCP-47, e.g. `en`, `it-IT`).
    pub default_locale: String,
    /// Declared locales (must include the default).
    pub locales: Vec<String>,
    /// Key -> locale -> text.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub strings: BTreeMap<String, BTreeMap<String, String>>,
}

impl Default for Localization {
    fn default() -> Self {
        Self {
            default_locale: "en".into(),
            locales: vec!["en".into()],
            strings: BTreeMap::new(),
        }
    }
}
