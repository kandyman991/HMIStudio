//! Conservative mapping of ordered navigation declarations. No execution.
use super::{path, Mapping};
use serde_json::{json, Value};

fn handlers(
    source: &Value,
    p: &str,
    origin: Value,
    screen: bool,
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
                "navigate.screen" => {
                    let old = a.get("screenId")?.as_str()?;
                    let id = m.ids.get(&path(&path("/screens", old), "id"))?.clone();
                    m.mark(&path(&sp, "screenId"), &path(&tp, "screen"));
                    json!({"type":"navigate","screen":id})
                }
                "navigate.back" => json!({"type":"back"}),
                "navigate.startup" => json!({"type":"navigateStartup"}),
                "popup.close" if version >= 10 => json!({"type":"closePopup"}),
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

pub(super) fn map(v: &Value, doc: &mut Value, m: &mut Mapping) -> Option<()> {
    let version = v["schemaVersion"].as_u64()?;
    let mut out = Vec::new();
    for (key, s) in v["screens"].as_object()? {
        let p = path("/screens", key);
        let id = m.ids.get(&path(&p, "id"))?.clone();
        handlers(s, &p, json!({"screen":id}), true, version, &mut out, m)?;
        if let Some(cs) = s.get("components") {
            for (key, c) in cs.as_object()? {
                let cp = path(&path(&p, "components"), key);
                let id = m.ids.get(&path(&cp, "id"))?.clone();
                handlers(c, &cp, json!({"node":id}), false, version, &mut out, m)?;
            }
        }
    }
    if !out.is_empty() {
        doc["v2"]["components"]["handlers"] = json!(out);
    }
    Some(())
}
