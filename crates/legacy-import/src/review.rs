//! Local, inactive engineering evidence. Never serialize declarations into
//! the canonical project, and never copy arbitrary unknown source fields.
use super::{path, Mapping};
use serde_json::{json, Value};

#[derive(Clone, PartialEq, Eq)]
pub struct ReviewOnlyContract {
    pub source_path: String,
    pub kind: &'static str,
    pub declaration: Value,
}

impl std::fmt::Debug for ReviewOnlyContract {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReviewOnlyContract")
            .field("source_path", &self.source_path)
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

fn faceplate(action: &Value, p: &str, v: &Value) -> Option<ReviewOnlyContract> {
    if action["action"] != "open.faceplate"
        || action.as_object()?.keys().any(|k| {
            !matches!(
                k.as_str(),
                "action" | "definitionId" | "modal" | "parameters"
            )
        })
    {
        return None;
    }
    let id = action["definitionId"].as_str()?;
    if v["componentDefinitions"][id]["kind"] != "FACEPLATE" {
        return None;
    }
    if let Some(modal) = action.get("modal") {
        modal.as_bool()?;
    }
    if let Some(parameters) = action.get("parameters") {
        if !parameters
            .as_object()?
            .values()
            .all(|x| x.as_str().is_some())
        {
            return None;
        }
    }
    Some(ReviewOnlyContract {
        source_path: p.into(),
        kind: "faceplate-forwarding",
        declaration: action.clone(),
    })
}

fn text_input(c: &Value, p: &str, v: &Value) -> Option<ReviewOnlyContract> {
    if c["type"] != "textInput" {
        return None;
    }
    let props = c.get("properties")?.as_object()?;
    let bindings = c.get("bindings")?.as_object()?;
    if bindings.len() != 1
        || props
            .keys()
            .any(|k| !matches!(k.as_str(), "label" | "maxLength"))
        || !props.values().all(Value::is_string)
    {
        return None;
    }
    let variable = bindings.get("value")?.as_str()?;
    if v["variables"][variable]["type"] != "STRING" {
        return None;
    }
    if let Some(max) = props.get("maxLength") {
        if !(1..=1000).contains(&max.as_str()?.parse::<u32>().ok()?) {
            return None;
        }
    }
    Some(ReviewOnlyContract {
        source_path: p.into(),
        kind: "text-input-write-policy",
        declaration: json!({"type":"textInput","properties":props,"bindings":bindings}),
    })
}

pub(super) fn collect(v: &Value, m: &mut Mapping) -> Vec<ReviewOnlyContract> {
    let mut out = Vec::new();
    if v["schemaVersion"].as_u64() != Some(10) {
        return out;
    }
    let Some(screens) = v["screens"].as_object() else {
        return out;
    };
    for (screen, s) in screens {
        let Some(components) = s["components"].as_object() else {
            continue;
        };
        for (component, c) in components {
            let p = path(&path(&path("/screens", screen), "components"), component);
            if let Some(review) = text_input(c, &p, v) {
                m.fail(&p, "text-input-write-policy-not-representable");
                out.push(review);
            }
            let Some(actions) = c["actions"].as_object() else {
                continue;
            };
            for (event, action) in actions {
                let ep = path(&path(&p, "actions"), event);
                if let Some(list) = action.as_array() {
                    for (i, item) in list.iter().enumerate() {
                        let ap = format!("{ep}/{i}");
                        if let Some(review) = faceplate(item, &ap, v) {
                            m.fail(&ap, "faceplate-forwarding-not-representable");
                            out.push(review);
                        }
                    }
                } else if let Some(review) = faceplate(action, &ep, v) {
                    m.fail(&ep, "faceplate-forwarding-not-representable");
                    out.push(review);
                }
            }
        }
    }
    out
}
