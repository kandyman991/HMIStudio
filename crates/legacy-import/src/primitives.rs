//! Fixed-frame graphic declarations and conservative passive symbol conversion.
use super::{path, Mapping};
use hmi_project_model::{
    id::IdAllocator, model::v2_components::IndustrialSymbolKind as K, symbols::build_symbol,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn color(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

pub(super) fn shape(
    v: &Value,
    p: &str,
    id: &str,
    width: f32,
    height: f32,
    m: &mut Mapping,
) -> Option<()> {
    let props = v.get("properties")?.as_object()?;
    let kind = v["type"].as_str()?;
    let stroke = props.get("strokeColor")?.as_str()?;
    let sw = props.get("strokeWidth")?.as_str()?.parse::<f32>().ok()?;
    if !color(stroke) || !(1.0..=12.0).contains(&sw) || width <= sw || height <= sw {
        return None;
    }
    if props.get("strokeStyle").and_then(Value::as_str) != Some("SOLID") {
        return None;
    }
    let raw = props.get("points")?.as_str()?;
    let mut points = Vec::new();
    let inset = sw / 2.0;
    for pair in raw.split_ascii_whitespace() {
        let (x, y) = pair.split_once(',')?;
        let x = x.parse::<f32>().ok()?;
        let y = y.parse::<f32>().ok()?;
        if !(0.0..=100.0).contains(&x) || !(0.0..=100.0).contains(&y) || points.len() >= 1024 {
            return None;
        }
        points.push(json!({
            "x": inset + x / 100.0 * (width - 2.0 * inset),
            "y": inset + y / 100.0 * (height - 2.0 * inset)
        }));
    }
    let mut contract = json!({"kind":kind,"strokeColor":stroke,"strokeWidth":sw});
    let t = format!("/v2/components/components/{}/contract", m.contracts.len());
    if kind == "line" {
        if points.len() != 2 {
            m.fail(
                &path(&path(p, "properties"), "points"),
                "polyline-not-representable",
            );
            return None;
        }
        contract["start"] = points[0].clone();
        contract["end"] = points[1].clone();
    } else {
        if points.len() < 3 {
            return None;
        }
        let fill = props.get("fillColor")?.as_str()?;
        if !color(fill) {
            return None;
        }
        contract["fill"] = json!(fill);
        contract["points"] = json!(points);
        m.mark(&path(&path(p, "properties"), "fillColor"), &path(&t, "fill"));
    }
    for (key, target) in [
        ("strokeColor", "strokeColor"),
        ("strokeWidth", "strokeWidth"),
        ("strokeStyle", "kind"),
        ("points", if kind == "line" { "start" } else { "points" }),
    ] {
        m.mark(&path(&path(p, "properties"), key), &path(&t, target));
    }
    if let Some(f) = m.findings.get_mut(&path(p, "type")) {
        f.code = "fixed-frame-primitive-parity-pending";
        f.severity = "warning";
        f.message = "Fixed-frame geometry mapped; renderer and resize parity not established.";
    }
    m.contracts.push(json!({"id":id,"contract":contract}));
    Some(())
}

pub(super) fn symbol_kind(kind: &str) -> Option<K> {
    match kind {
        "motor" => Some(K::Motor),
        "conveyor" => Some(K::Conveyor),
        "valve" => Some(K::Valve),
        "pump" => Some(K::Pump),
        _ => None,
    }
}

pub(super) fn symbol(
    source: &Value,
    node: &mut Value,
    p: &str,
    t: &str,
    kind: K,
    ids: &BTreeMap<String, String>,
    m: &mut Mapping,
) -> Option<(Value, Value)> {
    // Only explicitly supplied passive status slots; any extra properties,
    // writable controls, or expressions remain unaccounted and refuse import.
    let bindings = source.get("bindings")?.as_object()?;
    if bindings.len() != kind.slots().len() {
        m.fail(&path(p, "bindings"), "industrial-symbol-slot-mismatch");
        return None;
    }
    let mut parameters = json!({});
    for (slot, _) in kind.slots() {
        let legacy = bindings.get(*slot)?.as_str()?;
        parameters[*slot] = json!(ids.get(legacy)?);
        m.mark(
            &path(&path(p, "bindings"), slot),
            &path(&format!("{t}/element/parameters"), slot),
        );
    }
    let mut alloc = IdAllocator::new(&m.used);
    let definition = build_symbol(kind, &mut alloc).ok()?;
    let id = definition.id.as_str().to_owned();
    m.used.insert(id.clone());
    definition.root.walk(&mut |n| {
        m.used.insert(n.id.as_str().to_owned());
    });
    m.ids
        .insert(path(p, "@generated-symbol-definition"), id.clone());
    node["element"] = json!({"type":"instance","definition":id,"parameters":parameters});
    if let Some(f) = m.findings.get_mut(&path(p, "type")) {
        f.code = "industrial-symbol-visual-approximation";
        f.severity = "warning";
        f.target = Some(format!("{t}/element/definition"));
        f.message = "Passive status slots mapped to an original canonical symbol; legacy appearance and interactions are not established.";
    }
    let class = json!({"definition":id,"classification":{"kind":"industrialSymbol","symbol":kind}});
    Some((serde_json::to_value(definition).ok()?, class))
}
