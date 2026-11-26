//! Conservative mapping of ordered navigation declarations. No execution.
use super::{path, Mapping};
use serde_json::{json, Value};

fn handlers(
    source: &Value,
    p: &str,
    origin: Value,
    doc: &Value,
    version: u64,
    out: &mut Vec<Value>,
    m: &mut Mapping,
) -> Option<()> {
    let Some(actions) = source.get("actions") else {
        return Some(());
    };
    let actions = actions.as_object()?;
    let ap = path(p, "actions");
    if actions.is_empty() {
        m.mark(&ap, "/v2/components/handlers");
        return Some(());
    }
    if version < 5 {
        m.fail(&ap, "actions-version-mismatch");
        return None;
    }
    let screen = origin.get("screen").is_some();
    for (event, values) in actions {
        let ep = path(&ap, event);
        let kind = match (screen, event.as_str()) {
            (true, "open") if version >= 10 => "show",
            (true, "close") if version >= 10 => "hide",
            (false, "press") if source["type"] == "button" => "press",
            (false, "release") if version >= 10 && source["type"] == "button" => "release",
            _ => {
                m.fail(&ep, "action-event-not-representable");
                return None;
            }
        };
        let list = match values {
            Value::Array(a) if version >= 10 => a.iter().collect::<Vec<_>>(),
            Value::Object(_) => vec![values],
            _ => return None,
        };
        if list.is_empty() || list.len() > 8 {
            m.fail(&ep, "invalid-action-count");
            return None;
        }
        let t = format!("/v2/components/handlers/{}", out.len());
        let mut mapped = Vec::new();
        for (i, a) in list.into_iter().enumerate() {
            let sp = if values.is_array() {
                format!("{ep}/{i}")
            } else {
                ep.clone()
            };
            let tp = format!("{t}/actions/{i}");
            let action = match a.get("action")?.as_str()? {
                "navigate.screen" if !screen => {
                    let old = a.get("screenId")?.as_str()?;
                    let id = m.ids.get(&path(&path("/screens", old), "id"))?.clone();
                    m.mark(&path(&sp, "screenId"), &path(&tp, "screen"));
                    json!({"type":"navigate","screen":id})
                }
                "navigate.back" if !screen => json!({"type":"back"}),
                "navigate.startup" if !screen => json!({"type":"navigateStartup"}),
                "popup.close" if version >= 10 => json!({"type":"closePopup"}),
                "value.set" | "value.toggle" | "value.pulse" | "value.step"
                    if version >= 10 =>
                {
                    typed_action(a, &sp, &tp, doc, m)?
                }
                _ => {
                    m.fail(&path(&sp, "action"), "action-kind-not-representable");
                    return None;
                }
            };
            m.mark(&path(&sp, "action"), &path(&tp, "type"));
            mapped.push(action);
        }
        out.push(json!({"origin":origin,"event":kind,"actions":mapped}));
    }
    Some(())
}

// Declarations only. An admitted write policy must agree with the action.
fn typed_action(a: &Value, p: &str, t: &str, doc: &Value, m: &mut Mapping) -> Option<Value> {
    let old = a.get("variableId")?.as_str()?;
    let id = m.ids.get(&path(&path("/variables", old), "id"))?.clone();
    let tag = doc["tags"].as_array()?.iter().find(|x| x["id"] == id)?;
    let policy = doc["v2"]["industrial"]["tags"]
        .as_array()?
        .iter()
        .find(|x| x["tag"] == id)?;
    if tag["access"] != "readWrite" || policy["writeConfirmed"] != true {
        return None;
    }
    let target = json!({"tag":id});
    let action = match a["action"].as_str()? {
        "value.set" if tag["dataType"] == "bool" && policy["command"] == "maintained" => {
            let val = a.get("value")?;
            if val["valueType"] != "bool" {
                return None;
            }
            let value = val.get("value")?.as_bool()?;
            m.mark(&format!("{p}/value/valueType"), &path(t, "value"));
            m.mark(&format!("{p}/value/value"), &path(t, "value"));
            json!({"type":"writeTag","target":target,"value":value})
        }
        "value.set" if policy["command"] == "setpoint" => {
            let value = numeric_constant(a.get("value")?, tag["dataType"].as_str()?, policy)?;
            m.mark(&format!("{p}/value/valueType"), &path(t, "value"));
            m.mark(&format!("{p}/value/value"), &path(t, "value"));
            json!({"type":"writeTag","target":target,"value":value})
        }
        "value.step" if tag["dataType"] == "int32" && policy["command"] == "setpoint" => {
            let action = integer_step(a, &target, policy)?;
            for (src, dest) in [("by", "step"), ("min", "min"), ("max", "max")] {
                m.mark(&path(p, src), &path(t, dest));
            }
            action
        }
        "value.toggle" if tag["dataType"] == "bool" && policy["command"] == "maintained" => {
            json!({"type":"toggleTag","target":target})
        }
        "value.pulse" if tag["dataType"] == "bool" && policy["command"] == "momentary" => {
            let ms = a.get("ms").map(Value::as_u64).unwrap_or(Some(500))?;
            if !(100..=5000).contains(&ms) || policy["pulseMs"].as_u64() != Some(ms) {
                m.fail(&path(p, "ms"), "pulse-action-policy-mismatch");
                return None;
            }
            m.mark(&path(p, "ms"), &path(t, "durationMs"));
            json!({"type":"pulseTag","target":target,"durationMs":ms})
        }
        _ => return None,
    };
    m.mark(&path(p, "variableId"), &format!("{t}/target/tag"));
    Some(action)
}

// Int32 arithmetic (including the addition before clamping) is exact in
// binary64. Require both explicit clamps and a compatible write policy.
// Runtime no-op, quality and confirmation behavior still require acceptance.
fn integer_step(a: &Value, target: &Value, policy: &Value) -> Option<Value> {
    fn exact_i32(v: &Value) -> Option<f64> {
        let n = v.as_f64()?;
        (n.fract() == 0. && (f64::from(i32::MIN)..=f64::from(i32::MAX)).contains(&n))
            .then_some(n)
    }
    let step = exact_i32(a.get("by")?)?;
    let min = exact_i32(a.get("min")?)?;
    let max = exact_i32(a.get("max")?)?;
    let limits = &policy["limits"];
    if step == 0.
        || min >= max
        || !limits["step"].is_null()
        || min < exact_i32(limits.get("min")?)?
        || max > exact_i32(limits.get("max")?)?
        || policy["confirmationRequired"] != true
    {
        return None;
    }
    Some(json!({"type":"stepTag","target":target,"step":step,"min":min,"max":max}))
}

// Admit only exact values in the policy range. Do not guess the alignment
// of a step-constrained setpoint or round a legacy value into range.
fn numeric_constant(value: &Value, ty: &str, policy: &Value) -> Option<Value> {
    let kind = value.get("valueType")?.as_str()?;
    let raw = value.get("value")?;
    let converted = match (ty, kind) {
        ("int32", "int") => json!(i32::try_from(raw.as_i64()?).ok()?),
        ("int64", "long") => json!(raw.as_i64()?),
        ("float32", "float") => {
            let n = raw.as_f64()?;
            if !n.is_finite() || f64::from(n as f32) != n {
                return None;
            }
            json!(n)
        }
        ("float64", "double") => json!(raw.as_f64()?),
        _ => return None,
    };
    let limits = &policy["limits"];
    if !limits["step"].is_null() {
        return None;
    }
    let valid = if matches!(ty, "int32" | "int64") {
        let n = converted.as_i64()?;
        n >= limits["min"].as_i64()? && n <= limits["max"].as_i64()?
    } else {
        let n = converted.as_f64()?;
        n.is_finite() && n >= limits["min"].as_f64()? && n <= limits["max"].as_f64()?
    };
    valid.then_some(converted)
}

pub(super) fn map(v: &Value, doc: &mut Value, m: &mut Mapping) -> Option<()> {
    let version = v["schemaVersion"].as_u64()?;
    let mut out = Vec::new();
    for (key, s) in v["screens"].as_object()? {
        let p = path("/screens", key);
        let id = m.ids.get(&path(&p, "id"))?.clone();
        handlers(s, &p, json!({"screen":id}), doc, version, &mut out, m)?;
        if let Some(cs) = s.get("components") {
            for (key, c) in cs.as_object()? {
                let cp = path(&path(&p, "components"), key);
                let id = m.ids.get(&path(&cp, "id"))?.clone();
                handlers(c, &cp, json!({"node":id}), doc, version, &mut out, m)?;
            }
        }
    }
    if !out.is_empty() {
        doc["v2"]["components"]["handlers"] = json!(out);
    }
    Some(())
}
