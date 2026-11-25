//! Original passive industrial symbol library built from the existing
//! semantic layout primitives. No imported art, runtime or device code.
//! The geometry is a new baseline, not a legacy visual-parity claim.
use crate::id::{IdAllocator, IdError};
use crate::model::v2_components::{IndustrialSymbolKind as K, SymbolSlotType};
use crate::model::*;
use serde_json::json;

fn node(alloc: &mut IdAllocator, kind: Element) -> Result<Node, IdError> {
    Ok(Node::new(NodeId::new(alloc.allocate("sym-node")?)?, kind))
}
fn positioned(
    alloc: &mut IdAllocator,
    kind: ComponentKind,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
) -> Result<Node, IdError> {
    let mut n = node(alloc, Element::Component { kind })?;
    n.layout.position = Some(Position { x, y });
    n.layout.width = Size::fixed(w);
    n.layout.height = Size::fixed(h);
    Ok(n)
}
fn shape(alloc: &mut IdAllocator, kind: ComponentKind, rect: [f64; 4]) -> Result<Node, IdError> {
    let [x, y, w, h] = rect;
    let mut n = positioned(alloc, kind, x, y, w, h)?;
    n.properties.insert("fill".into(), json!("#D3DCE4"));
    n.properties.insert("strokeColor".into(), json!("#42566B"));
    n.properties.insert("strokeWidth".into(), json!(2));
    Ok(n)
}
fn text(alloc: &mut IdAllocator, label: &str, rect: [f64; 4]) -> Result<Node, IdError> {
    let [x, y, w, h] = rect;
    let mut n = positioned(alloc, ComponentKind::Text, x, y, w, h)?;
    n.properties.insert("text".into(), json!(label));
    n.properties.insert("textAlign".into(), json!("center"));
    Ok(n)
}
/// Allocates a reusable definition and all node IDs from the caller's shared
/// allocator. All slot parameters are read-only and explicitly typed.
/// The caller must validate the definition as part of a project.
pub fn build_symbol(kind: K, alloc: &mut IdAllocator) -> Result<ComponentDefinition, IdError> {
    use ComponentKind::{Ellipse, Rectangle};
    let id = ComponentDefinitionId::new(alloc.allocate("sym")?)?;
    let mut root = node(alloc, Element::Absolute {})?;
    root.layout.width = Size::fixed(180.0);
    root.layout.height = Size::fixed(116.0);
    let (name, rects, label): (&str, &[(ComponentKind, [f64; 4])], &str) = match kind {
        K::Motor => (
            "Motor",
            &[
                (Ellipse, [58., 8., 64., 60.]),
                (Rectangle, [120., 28., 35., 18.]),
            ],
            "M",
        ),
        K::Valve => (
            "Valve",
            &[
                (Rectangle, [18., 28., 144., 18.]),
                (Ellipse, [64., 12., 52., 50.]),
                (Rectangle, [86., 3., 8., 15.]),
            ],
            "V",
        ),
        K::Conveyor => (
            "Conveyor",
            &[
                (Rectangle, [18., 20., 144., 32.]),
                (Ellipse, [23., 25., 23., 23.]),
                (Ellipse, [134., 25., 23., 23.]),
            ],
            "",
        ),
        K::Sensor => (
            "Sensor",
            &[
                (Rectangle, [52., 16., 76., 42.]),
                (Ellipse, [72., 19., 36., 36.]),
            ],
            "S",
        ),
        K::Pump => (
            "Pump",
            &[
                (Ellipse, [50., 12., 66., 60.]),
                (Rectangle, [112., 28., 45., 16.]),
                (Rectangle, [76., 1., 16., 15.]),
            ],
            "P",
        ),
        K::Tank => (
            "Tank",
            &[
                (Rectangle, [53., 12., 74., 55.]),
                (Ellipse, [53., 4., 74., 16.]),
                (Rectangle, [82., 67., 16., 8.]),
            ],
            "",
        ),
        K::Pipe => (
            "Pipe",
            &[
                (Rectangle, [15., 25., 150., 24.]),
                (Rectangle, [42., 18., 8., 38.]),
                (Rectangle, [130., 18., 8., 38.]),
            ],
            "",
        ),
        K::ThermalIndicator => (
            "Thermal indicator",
            &[
                (Rectangle, [84., 6., 12., 47.]),
                (Ellipse, [75., 43., 30., 30.]),
            ],
            "",
        ),
        K::FlowIndicator => (
            "Flow indicator",
            &[
                (Rectangle, [18., 28., 144., 17.]),
                (Ellipse, [65., 12., 50., 49.]),
            ],
            "F",
        ),
    };
    for (shape_kind, rect) in rects {
        root.children.push(shape(alloc, *shape_kind, *rect)?);
    }
    if !label.is_empty() {
        root.children
            .push(text(alloc, label, [67., 25., 46., 25.])?);
    }
    let mut parameters = Vec::new();
    for (i, (slot, ty)) in kind.slots().iter().enumerate() {
        let (component, property, dt) = match ty {
            SymbolSlotType::Boolean => (ComponentKind::Indicator, "active", DataType::Bool),
            SymbolSlotType::Numeric => (ComponentKind::NumericDisplay, "value", DataType::Float64),
        };
        parameters.push(ParameterDefinition {
            name: (*slot).into(),
            ty: PropertyType::Tag {
                data_type: Some(dt),
                writable: false,
            },
            default: None,
            description: None,
        });
        let x = 8.0 + i as f64 * 84.0;
        root.children.push(text(alloc, slot, [x, 77., 80., 17.])?);
        let mut n = positioned(alloc, component, x + 10., 95., 60., 18.)?;
        n.bindings.push(PropertyBinding {
            property: property.into(),
            source: BindingSource::TagParameter((*slot).into()),
            member: None,
            mode: BindingMode::Read,
            transform: None,
        });
        root.children.push(n);
    }
    Ok(ComponentDefinition {
        id,
        name: name.into(),
        description: Some("Original passive industrial symbol; runtime parity pending".into()),
        category: Some("Industrial".into()),
        parameters,
        root,
        extensions: Extensions::new(),
    })
}
