//! Built-in component catalog: typed properties, bindability and events per
//! component kind.
//!
//! The catalog is the single source of truth shared by the validator, the
//! editor inspector and (via `docs/PROJECT_MODEL.md`) the Compose runtime.
//! Adding a property here is a backwards-compatible schema change; removing
//! or retyping one requires a schema migration.

use crate::model::{ComponentKind, EventKind, PropertyType, ResourceKind};

/// Whether and how a property can be bound to live data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bindability {
    /// Static only.
    Static,
    /// May be driven by a read binding.
    Read,
    /// May be driven by a read binding and written back by operator input.
    ReadWrite,
}

/// A property definition.
#[derive(Clone, Debug, PartialEq)]
pub struct PropertySpec {
    /// Property name.
    pub name: &'static str,
    /// Value type.
    pub ty: PropertyType,
    /// Bindability.
    pub bindable: Bindability,
}

fn p(name: &'static str, ty: PropertyType, bindable: Bindability) -> PropertySpec {
    PropertySpec { name, ty, bindable }
}

fn boolean() -> PropertyType {
    PropertyType::Bool {}
}
fn number() -> PropertyType {
    PropertyType::Number {
        min: None,
        max: None,
    }
}
fn number_range(min: Option<f64>, max: Option<f64>) -> PropertyType {
    PropertyType::Number { min, max }
}
fn integer_range(min: Option<i64>, max: Option<i64>) -> PropertyType {
    PropertyType::Integer { min, max }
}
fn string() -> PropertyType {
    PropertyType::String {}
}
fn text() -> PropertyType {
    PropertyType::Text {}
}
fn color() -> PropertyType {
    PropertyType::Color {}
}
fn resource(kinds: &[ResourceKind]) -> PropertyType {
    PropertyType::Resource {
        kinds: kinds.to_vec(),
    }
}
fn enumeration(options: &[&str]) -> PropertyType {
    PropertyType::Enum {
        options: options.iter().map(|s| (*s).to_owned()).collect(),
    }
}

use Bindability::{Read, ReadWrite, Static};

/// Properties available on every node (layout containers, components and
/// instances).
pub fn common_properties() -> Vec<PropertySpec> {
    vec![
        p("visible", boolean(), Read),
        p("enabled", boolean(), Read),
        p("opacity", number_range(Some(0.0), Some(1.0)), Read),
        p("background", color(), Read),
        p("borderColor", color(), Read),
        p("borderWidth", number_range(Some(0.0), None), Static),
        p("cornerRadius", number_range(Some(0.0), None), Static),
        p("tooltip", text(), Read),
        p("accessibilityLabel", text(), Static),
    ]
}

fn text_props() -> Vec<PropertySpec> {
    vec![
        p("textColor", color(), Read),
        p("textStyle", string(), Static),
        p("fontSize", number_range(Some(1.0), None), Static),
        p(
            "fontWeight",
            enumeration(&["normal", "medium", "bold"]),
            Static,
        ),
        p(
            "textAlign",
            enumeration(&["start", "center", "end"]),
            Static,
        ),
    ]
}

/// Kind-specific properties (without the common ones).
pub fn kind_properties(kind: ComponentKind) -> Vec<PropertySpec> {
    let mut v = match kind {
        ComponentKind::Button => vec![
            p("text", text(), Read),
            p(
                "icon",
                resource(&[ResourceKind::Image, ResourceKind::Vector]),
                Read,
            ),
            p(
                "behavior",
                enumeration(&["action", "momentary", "toggle"]),
                Static,
            ),
            p("pressedColor", color(), Static),
        ],
        ComponentKind::Text => vec![
            p("text", text(), Read),
            p("maxLines", integer_range(Some(1), None), Static),
            p("overflow", enumeration(&["clip", "ellipsis"]), Static),
        ],
        ComponentKind::Image => vec![
            p(
                "source",
                resource(&[ResourceKind::Image, ResourceKind::Vector]),
                Read,
            ),
            p(
                "contentScale",
                enumeration(&["fit", "fill", "crop", "none"]),
                Static,
            ),
            p("tint", color(), Read),
        ],
        ComponentKind::Rectangle => vec![
            p("fill", color(), Read),
            p("strokeColor", color(), Read),
            p("strokeWidth", number_range(Some(0.0), None), Static),
        ],
        ComponentKind::Ellipse => vec![
            p("fill", color(), Read),
            p("strokeColor", color(), Read),
            p("strokeWidth", number_range(Some(0.0), None), Static),
        ],
        ComponentKind::NumericDisplay => vec![
            p("value", number(), Read),
            p("decimals", integer_range(Some(0), Some(10)), Static),
            p("showUnit", boolean(), Static),
            p("prefix", text(), Static),
        ],
        ComponentKind::NumericInput => vec![
            p("value", number(), ReadWrite),
            p("min", number(), Static),
            p("max", number(), Static),
            p("step", number_range(Some(0.0), None), Static),
            p("decimals", integer_range(Some(0), Some(10)), Static),
            p("showUnit", boolean(), Static),
        ],
        ComponentKind::Indicator => vec![
            p("active", boolean(), Read),
            p("activeColor", color(), Read),
            p("inactiveColor", color(), Read),
            p(
                "shape",
                enumeration(&["circle", "square", "rounded"]),
                Static,
            ),
            p("label", text(), Read),
            p("blink", boolean(), Read),
        ],
        ComponentKind::Gauge => vec![
            p("value", number(), Read),
            p("min", number(), Static),
            p("max", number(), Static),
            p("lowLimit", number(), Read),
            p("highLimit", number(), Read),
            p("variant", enumeration(&["radial", "linear"]), Static),
            p("majorTicks", integer_range(Some(0), Some(50)), Static),
            p("showUnit", boolean(), Static),
            p("needleColor", color(), Static),
        ],
        ComponentKind::Slider => vec![
            p("value", number(), ReadWrite),
            p("min", number(), Static),
            p("max", number(), Static),
            p("step", number_range(Some(0.0), None), Static),
            p(
                "orientation",
                enumeration(&["horizontal", "vertical"]),
                Static,
            ),
        ],
        ComponentKind::Switch => vec![p("checked", boolean(), ReadWrite), p("label", text(), Read)],
        ComponentKind::ProgressBar => vec![
            p("value", number(), Read),
            p("min", number(), Static),
            p("max", number(), Static),
            p(
                "orientation",
                enumeration(&["horizontal", "vertical"]),
                Static,
            ),
            p("barColor", color(), Read),
        ],
        ComponentKind::Container => vec![p("elevation", number_range(Some(0.0), None), Static)],
        // Geometry and input targets belong to the typed contract, not an untyped duplicate property bag.
        ComponentKind::Line
        | ComponentKind::Arrow
        | ComponentKind::Polygon
        | ComponentKind::TextInput
        | ComponentKind::DropDown => vec![],
    };
    if matches!(
        kind,
        ComponentKind::Button
            | ComponentKind::Text
            | ComponentKind::NumericDisplay
            | ComponentKind::NumericInput
            | ComponentKind::Indicator
            | ComponentKind::Switch
    ) {
        v.extend(text_props());
    }
    v
}

/// All properties of a kind (common + kind-specific), or the common
/// properties for layout containers and instances (`None`).
pub fn properties(kind: Option<ComponentKind>) -> Vec<PropertySpec> {
    let mut v = common_properties();
    if let Some(kind) = kind {
        v.extend(kind_properties(kind));
    }
    v
}

/// Looks up one property.
pub fn find_property(kind: Option<ComponentKind>, name: &str) -> Option<PropertySpec> {
    properties(kind).into_iter().find(|s| s.name == name)
}

/// Events a node of the given kind can raise (`None` = layout container or
/// instance).
pub fn events(kind: Option<ComponentKind>) -> &'static [EventKind] {
    match kind {
        Some(ComponentKind::Button) => &[
            EventKind::Click,
            EventKind::Press,
            EventKind::Release,
            EventKind::LongPress,
        ],
        Some(
            ComponentKind::NumericInput
            | ComponentKind::Slider
            | ComponentKind::Switch
            | ComponentKind::TextInput
            | ComponentKind::DropDown,
        ) => &[EventKind::ValueChanged],
        _ => &[EventKind::Click],
    }
}

/// Events a screen can raise.
pub const SCREEN_EVENTS: &[EventKind] = &[EventKind::Show, EventKind::Hide];

/// The Compose Multiplatform construct a component kind is expected to map
/// to (informative; the generator is a later milestone).
pub fn compose_mapping(kind: ComponentKind) -> &'static str {
    match kind {
        ComponentKind::Button => "Button / OutlinedButton with Text + Icon",
        ComponentKind::Text => "Text",
        ComponentKind::Image => "Image(painterResource / vector)",
        ComponentKind::Rectangle => "Box(Modifier.background(color, RoundedCornerShape).border)",
        ComponentKind::Ellipse => "Box(Modifier.background(color, CircleShape)) / Canvas.drawOval",
        ComponentKind::NumericDisplay => "Text with number formatting",
        ComponentKind::NumericInput => "OutlinedTextField(KeyboardType.Decimal) + keypad dialog",
        ComponentKind::Indicator => "Box(CircleShape) with animated color",
        ComponentKind::Gauge => "Canvas arc/needle (custom composable)",
        ComponentKind::Slider => "Slider",
        ComponentKind::Switch => "Switch",
        ComponentKind::ProgressBar => "LinearProgressIndicator",
        ComponentKind::Container => "Surface / Card",
        ComponentKind::Line | ComponentKind::Arrow | ComponentKind::Polygon => {
            "Typed geometry (renderer pending)"
        }
        ComponentKind::TextInput | ComponentKind::DropDown => "Typed input (renderer pending)",
    }
}
