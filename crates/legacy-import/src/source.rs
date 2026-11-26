//! Source checks independent of canonical model validation.
use super::{path, Mapping};
use serde_json::Value;

fn present(v: &Value) -> bool {
    !v.is_null() && v.as_object().is_none_or(|x| !x.is_empty())
        && v.as_array().is_none_or(|x| !x.is_empty())
}

fn component(c: &Value, p: &str, version: u64, m: &mut Mapping) -> Option<()> {
    for (key, min) in [("expressions", 4), ("actions", 5), ("requiredPermission", 8)] {
        if version < min && c.get(key).is_some_and(present) {
            m.fail(&path(p, key), "source-feature-version-mismatch");
            return None;
        }
    }
    if version < 10
        && matches!(
            c["type"].as_str(),
            Some("switch" | "slider" | "numericInput" | "textInput" | "dropDown")
        )
    {
        m.fail(&path(p, "type"), "source-feature-version-mismatch");
        return None;
    }
    Some(())
}

pub(super) fn validate(v: &Value, version: u64, m: &mut Mapping) -> Option<()> {
    for (key, min) in [
        ("screenOrder", 5),
        ("assets", 7),
        ("theme", 7),
        ("security", 8),
        ("enumerations", 10),
    ] {
        if version < min && v.get(key).is_some_and(present) {
            m.fail(&path("", key), "source-feature-version-mismatch");
            return None;
        }
    }
    // Unlike an empty map, an explicit empty theme or security model
    // is still a versioned feature.
    for (key, min) in [("theme", 7), ("security", 8)] {
        if version < min && v.get(key).is_some_and(|x| !x.is_null()) {
            m.fail(&path("", key), "source-feature-version-mismatch");
            return None;
        }
    }
    for (key, screen) in v["screens"].as_object()? {
        let p = path("/screens", key);
        for (field, min) in [("layout", 2), ("requiredPermission", 8), ("actions", 10)] {
            if version < min && screen.get(field).is_some_and(present) {
                m.fail(&path(&p, field), "source-feature-version-mismatch");
                return None;
            }
        }
        if let Some(cs) = screen.get("components") {
            for (key, c) in cs.as_object()? {
                component(c, &path(&path(&p, "components"), key), version, m)?;
            }
        }
    }
    if let Some(ds) = v.get("componentDefinitions") {
        for (key, d) in ds.as_object()? {
            let p = path("/componentDefinitions", key);
            if version < 6
                && (d.get("composition").is_some_and(|x| !x.is_null())
                    || d.get("parameters").is_some_and(present)
                    || d.get("category").is_some_and(|x| !x.is_null())
                    || d.get("description").is_some_and(|x| !x.is_null())
                    || d.get("kind").is_some_and(|x| x != "COMPONENT"))
            {
                m.fail(&p, "source-feature-version-mismatch");
                return None;
            }
            if let Some(es) = d["composition"].get("elements") {
                for (key, c) in es.as_object()? {
                    component(c, &path(&path(&path(&p, "composition"), "elements"), key), version, m)?;
                }
            }
        }
    }
    Some(())
}
