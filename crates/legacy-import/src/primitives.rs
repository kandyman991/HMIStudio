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
        m.mark(
            &path(&path(p, "properties"), "fillColor"),
            &path(&t, "fill"),
        );
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
        "tank" => Some(K::Tank),
        "pipe" => Some(K::Pipe),
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
        let legacy_slot = if kind == K::Pipe { "flow" } else { slot };
        let legacy = bindings.get(legacy_slot)?.as_str()?;
        parameters[*slot] = json!(ids.get(legacy)?);
        m.mark(
            &path(&path(p, "bindings"), legacy_slot),
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

// A legacy arrow has a filled triangular head, whereas the canonical
// arrow is a line with a head. Only same-color cardinal arrows are admitted.
pub(super) fn arrow(
    v: &Value,
    p: &str,
    id: &str,
    width: f64,
    height: f64,
    m: &mut Mapping,
) -> Option<()> {
    let props = v.get("properties")?.as_object()?;
    let stroke = props.get("strokeColor")?.as_str()?;
    let fill = props.get("fillColor")?.as_str()?;
    let sw = props.get("strokeWidth")?.as_str()?.parse::<f32>().ok()?;
    if !color(stroke) || !stroke.eq_ignore_ascii_case(fill) || !(1.0..=8.0).contains(&sw) {
        return None;
    }
    let direction = props.get("direction")?.as_str()?;
    let (dx, dy) = match direction {
        "right" => (1., 0.),
        "left" => (-1., 0.),
        "up" => (0., -1.),
        "down" => (0., 1.),
        _ => return None,
    };
    if let Some(r) = props.get("rotation") {
        if r.as_str()?.parse::<f32>().ok()? != 0. {
            return None;
        }
    }
    let scale = (width / 60.).min(height / 32.);
    let x = width / 2.;
    let y = height / 2.;
    let stroke_width = f64::from(sw) * scale;
    if scale <= 0. || !stroke_width.is_finite() {
        return None;
    }
    let t = format!("/v2/components/components/{}/contract", m.contracts.len());
    let contract = json!({"kind":"arrow","start":{"x":x-26.*scale*dx,"y":y-26.*scale*dy},"end":{"x":x+26.*scale*dx,"y":y+26.*scale*dy},"headLength":16.*scale,"strokeColor":stroke,"strokeWidth":stroke_width});
    for (key, target) in [
        ("strokeColor", "strokeColor"),
        ("fillColor", "strokeColor"),
        ("strokeWidth", "strokeWidth"),
        ("direction", "end"),
        ("rotation", "end"),
    ] {
        m.mark(&path(&path(p, "properties"), key), &path(&t, target));
    }
    if let Some(f) = m.findings.get_mut(&path(p, "type")) {
        f.code = "arrow-renderer-approximation";
        f.severity = "warning";
        f.message =
            "Cardinal arrow geometry mapped; filled-head and resize parity not established.";
    }
    m.contracts.push(json!({"id":id,"contract":contract}));
    Some(())
}

// Input contracts refer to already validated canonical tag and enumeration
// declarations. No direct write path is introduced by the importer.
pub(super) fn dropdowns(v: &Value, doc: &Value, m: &mut Mapping) -> Option<()> {
    for (screen, s) in v["screens"].as_object()? {
        let Some(cs) = s.get("components") else {
            continue;
        };
        for (key, c) in cs.as_object()? {
            if c["type"] != "dropDown" {
                continue;
            }
            let p = path(&path(&path("/screens", screen), "components"), key);
            let id = m.ids.get(&path(&p, "id"))?.clone();
            let bindings = c.get("bindings")?.as_object()?;
            if bindings.len() != 1 {
                return None;
            }
            let old = bindings.get("value")?.as_str()?;
            let tag_id = m.ids.get(&path(&path("/variables", old), "id"))?.clone();
            let props = c.get("properties")?.as_object()?;
            let old_enum = props.get("enumeration")?.as_str()?;
            let enum_id = m
                .ids
                .get(&path(&path("/enumerations", old_enum), "id"))?
                .clone();
            let tag = doc["tags"].as_array()?.iter().find(|x| x["id"] == tag_id)?;
            let en = doc["v2"]["bindings"]["enumerations"]
                .as_array()?
                .iter()
                .find(|x| x["id"] == enum_id)?;
            let policy = doc["v2"]["industrial"]["tags"]
                .as_array()?
                .iter()
                .find(|x| x["tag"] == tag_id)?;
            if tag["dataType"] != "int64"
                || tag["access"] != "readWrite"
                || en["dataType"] != "int64"
                || policy["command"] != "setpoint"
                || policy["writeConfirmed"] != true
                || !policy["limits"]["step"].is_null()
            {
                return None;
            }
            let min = policy["limits"]["min"].as_i64()?;
            let max = policy["limits"]["max"].as_i64()?;
            for e in en["entries"].as_array()? {
                let n = e["value"].as_i64()?;
                if n < min || n > max || !(-9007199254740992..=9007199254740992).contains(&n) {
                    return None;
                }
            }
            let label = props.get("label")?.as_str()?;
            if label.contains("{{") {
                return None;
            }
            let t = format!("/v2/components/components/{}/contract", m.contracts.len());
            m.mark(&format!("{p}/bindings/value"), &path(&t, "tag"));
            m.mark(
                &format!("{p}/properties/enumeration"),
                &path(&t, "enumeration"),
            );
            m.mark(&format!("{p}/properties/label"), &path(&t, "label"));
            if let Some(f) = m.findings.get_mut(&path(&p, "type")) {
                f.code = "dropdown-runtime-parity-pending";
                f.severity = "warning";
                f.message = "Typed choices mapped; runtime input and write policy parity pending.";
            }
            m.contracts.push(
                json!({"id":id,"contract":{"kind":"dropDown","tag":tag_id,"enumeration":enum_id,"label":label}}),
            );
        }
    }
    Some(())
}
