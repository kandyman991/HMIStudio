//! Conservative graphical declaration migration. Unsupported semantics are refused.
use super::{dim, fixed, path, Mapping};
use hmi_project_model::expression::{compile, Type, CONTRACT_VERSION};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

fn refuse<T>(m: &mut Mapping, p: &str, code: &'static str) -> Option<T> {
    m.fail(p, code);
    None
}
fn mark(m: &mut Mapping, p: &str, k: &str, t: &str, target: &str) {
    m.mark(&path(p, k), &path(t, target));
}
fn color(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s[1..].bytes().all(|b| b.is_ascii_hexdigit())
}
fn number(s: &str) -> Option<Value> {
    let n = s.parse::<f64>().ok()?;
    n.is_finite().then(|| json!(n))
}

pub(super) fn component(v: &Value, key: &str, p: &str, t: &str, m: &mut Mapping) -> Option<Value> {
    if v.get("id")?.as_str() != Some(key) || key.trim().is_empty() {
        return None;
    }
    let kind = v.get("type")?.as_str()?;
    let canonical = match kind {
        "label" => "text",
        "button" => "button",
        "rectangle" => "rectangle",
        "ellipse" => "ellipse",
        "numericField" => "numericDisplay",
        "indicator" => "indicator",
        "composite" => "instance",
        "line" => "line",
        "arrow" => "arrow",
        "image" => "image",
        "dropDown" => "dropDown",
        "polygon" => "polygon",
        "motor" | "conveyor" | "valve" | "pump" | "tank" | "pipe" => "instance",
        _ => return refuse(m, &path(p, "type"), "component-kind-not-mapped"),
    };
    let id = m.id(&path(p, "id"), key, "node");
    mark(m, p, "id", t, "id");
    m.mark(&path(p, "type"), &format!("{t}/element"));
    if matches!(kind, "numericField" | "indicator" | "button") {
        if let Some(f) = m.findings.get_mut(&path(p, "type")) {
            f.severity = "warning";
            f.code = "component-renderer-parity-pending";
            f.message="Component declaration mapped; legacy renderer behavior requires interoperability validation.";
        }
    }
    let name = match v.get("displayName") {
        Some(n) => m.text(n, &path(p, "displayName"), &path(t, "name"))?,
        None => key.into(),
    };
    let g = v.get("geometry");
    if g.is_some_and(|x| !x.is_object()) {
        return None;
    }
    let mut geo = Vec::new();
    for (k, default, dest) in [
        ("x", 0., "position/x"),
        ("y", 0., "position/y"),
        ("width", 120., "width/value"),
        ("height", 60., "height/value"),
    ] {
        let n = dim(g.and_then(|x| x.get(k)), default)?;
        if matches!(k, "width" | "height") && n <= 0. {
            return None;
        }
        geo.push(n);
        m.mark(&format!("{p}/geometry/{k}"), &format!("{t}/layout/{dest}"));
    }
    if g.is_some_and(|x| x.as_object().is_some_and(|o| o.is_empty())) {
        mark(m, p, "geometry", t, "layout");
    }
    let mut props = json!({});
    if kind == "arrow" {
        super::primitives::arrow(v, p, &id, geo[2], geo[3], m)?;
    }
    if matches!(kind, "line" | "polygon") {
        super::primitives::shape(v, p, &id, geo[2] as f32, geo[3] as f32, m)?;
    }
    if let Some(x) = v.get("properties") {
        let o = x.as_object()?;
        if o.is_empty() {
            mark(m, p, "properties", t, "properties");
        }
        for (k, x) in o {
            let raw = x.as_str()?;
            let target = match (kind, k.as_str()) {
                ("label" | "button", "text") => "text",
                ("rectangle" | "ellipse", "fillColor") => "fill",
                ("rectangle" | "ellipse", "strokeColor") => "strokeColor",
                ("rectangle" | "ellipse", "strokeWidth") => "strokeWidth",
                ("rectangle", "cornerRadius") => "cornerRadius",
                ("indicator", "label") => "label",
                ("indicator", "onColor") => "activeColor",
                ("indicator", "offColor") => "inactiveColor",
                ("numericField", "decimals") => "decimals",
                _ => continue,
            };
            let value = match target {
                "text" | "label" => {
                    if raw.contains("{{") {
                        return refuse(
                            m,
                            &format!("{p}/properties/{k}"),
                            "parameter-template-not-mapped",
                        );
                    }
                    json!(raw)
                }
                "fill" | "strokeColor" | "activeColor" | "inactiveColor" => {
                    if color(raw) {
                        json!(raw)
                    } else if let Some(token) = raw.strip_prefix('@') {
                        if !m.theme_colors.contains_key(token) {
                            return refuse(
                                m,
                                &format!("{p}/properties/{k}"),
                                "unknown-explicit-theme-token",
                            );
                        }
                        json!({"token":token})
                    } else {
                        return refuse(
                            m,
                            &format!("{p}/properties/{k}"),
                            "theme-or-color-not-mapped",
                        );
                    }
                }
                "decimals" => json!(raw.parse::<u32>().ok()?),
                _ => number(raw)?,
            };
            props[target] = value;
            m.mark(
                &path(&path(p, "properties"), k),
                &format!("{t}/properties/{target}"),
            );
        }
    }
    // Avoid relying on differing renderer defaults.
    match kind {
        "label" | "button" => {
            props.get("text")?;
        }
        "rectangle" => {
            for k in ["fill", "strokeColor", "strokeWidth", "cornerRadius"] {
                props.get(k)?;
            }
        }
        "ellipse" => {
            for k in ["fill", "strokeColor", "strokeWidth"] {
                props.get(k)?;
            }
        }
        "indicator" => {
            for k in ["label", "activeColor", "inactiveColor"] {
                props.get(k)?;
            }
        }
        "numericField" => {
            props.get("decimals")?;
        }
        _ => {}
    }
    if let Some(z) = v.get("zIndex") {
        i32::try_from(z.as_i64()?).ok()?;
        m.mark(&path(p, "zIndex"), t);
    }
    let visible = match v.get("visible") {
        None => true,
        Some(x) => {
            let b = x.as_bool()?;
            m.mark(&path(p, "visible"), &format!("{t}/layout/visibility"));
            b
        }
    };
    for k in [
        "componentDefinitionId",
        "parentId",
        "bindings",
        "expressions",
        "actions",
        "requiredPermission",
    ] {
        if let Some(x) = v.get(k) {
            m.empty(x, &path(p, k), t);
        }
    }
    // Instance definition is resolved in the second pass.
    let element = if kind == "composite" {
        json!({"type":"instance","definition":v.get("componentDefinitionId")?.as_str()?})
    } else if super::primitives::symbol_kind(kind).is_some() {
        json!({"type":"instance","definition":"pending-symbol"})
    } else {
        json!({"type":"component","kind":canonical})
    };
    Some(
        json!({"id":id,"name":name,"element":element,"layout":{"position":{"x":geo[0],"y":geo[1]},"width":fixed(geo[2]),"height":fixed(geo[3]),"visibility":if visible{"visible"}else{"hidden"}},"properties":props}),
    )
}

// Source geometry remains available for the design editor; the adaptive layout
// supplies placement instead. Report preservation explicitly, not as geometry parity.
fn preserve_geometry(m: &mut Mapping, p: &str, t: &str, n: &mut Value) {
    let geometry = n["layout"].clone();
    n["extensions"]["hmiforge.legacyAbsoluteLayout"] = geometry;
    for (k, f) in &mut m.findings {
        if f.disposition == "mapped"
            && (k == &format!("{p}/geometry") || k.starts_with(&format!("{p}/geometry/")))
        {
            f.disposition = "preserved";
            f.code = "legacy-geometry-preserved";
            f.severity = "info";
            f.target = Some(format!("{t}/extensions/hmiforge.legacyAbsoluteLayout"));
            f.message = "Decoded absolute geometry preserved as inactive design metadata.";
        }
    }
}
fn approximate(m: &mut Mapping, p: &str, t: &str) {
    m.mark(p, t);
    if let Some(f) = m.findings.get_mut(p) {
        f.code = "adaptive-layout-approximation";
        f.severity = "warning";
        f.message="Semantic layout intent mapped; exact legacy renderer reflow and sizing are not established.";
    }
}
// The design-frame preview assumes font scale 1. Dynamic local measurement
// is not expressible in the frozen model; never claim runtime equivalence.
fn minimum_width(v: &Value) -> Option<f64> {
    match v.get("node")?.as_str()? {
        "component" => {
            let width = dim(v.get("minWidthDp"), 160.)?;
            (width >= 0.).then_some(width)
        }
        "stack" => {
            let pad = dim(v.get("paddingDp"), 0.)?;
            if pad < 0. {
                return None;
            }
            let mut widest: f64 = 0.;
            for child in v.get("children")?.as_array()? {
                widest = widest.max(minimum_width(child)?);
            }
            Some(widest + 2. * pad)
        }
        _ => None,
    }
}

struct Layout<'a> {
    source: &'a serde_json::Map<String, Value>,
    nodes: BTreeMap<String, Value>,
    used_stack: BTreeSet<String>,
    p: String,
    ver: u64,
    design_width: f64,
    design_height: f64,
}
impl Layout<'_> {
    fn node(
        &mut self,
        v: &Value,
        p: &str,
        t: &str,
        m: &mut Mapping,
        depth: usize,
        parent_row: bool,
    ) -> Option<Value> {
        if depth > 48 {
            return refuse(m, p, "layout-depth-limit");
        }
        let weight = dim(v.get("weight"), 1.)?;
        if weight <= 0. {
            return None;
        }
        match v.get("node")?.as_str()? {
            "stack" => {
                let key = v.get("id")?.as_str()?;
                if key.trim().is_empty() || !self.used_stack.insert(key.into()) {
                    return refuse(m, p, "duplicate-layout-id");
                }
                let id = m.id(&path(p, "id"), key, "layout");
                mark(m, p, "id", t, "id");
                let dir = v.get("direction")?.as_str()?;
                let gap = dim(v.get("gapDp"), 16.)?;
                let pad = dim(v.get("paddingDp"), 0.)?;
                if gap < 0. || pad < 0. {
                    return None;
                }
                let children = v.get("children")?.as_array()?;
                let cap = v.get("columns").map(Value::as_u64).unwrap_or(Some(0))?;
                if dir != "GRID" && cap != 0 {
                    return None;
                }
                let mut columns = 1;
                let row = match dir {
                    "ROW" => true,
                    "COLUMN" => false,
                    "ADAPTIVE" if depth == 0 => {
                        let mut needed = gap * children.len().saturating_sub(1) as f64;
                        for child in children {
                            needed += minimum_width(child)?;
                        }
                        self.design_width >= self.design_height
                            && needed <= self.design_width - 2. * pad
                    }
                    "GRID" if self.ver >= 3 && cap == 1 => false,
                    "GRID" if self.ver >= 3 && depth == 0 => {
                        let mut widest: f64 = 1.;
                        for child in children {
                            widest = widest.max(minimum_width(child)?);
                        }
                        let inner = self.design_width - 2. * pad;
                        let fitting = ((inner + gap) / (widest + gap)).floor().max(1.);
                        let limit = if cap == 0 { u64::MAX } else { cap };
                        columns = (fitting as u64).min(limit).min(children.len() as u64);
                        columns = columns.max(1);
                        if columns > 64 || children.len() > 4096 {
                            return refuse(m, p, "grid-track-limit");
                        }
                        false
                    }
                    _ => {
                        return refuse(
                            m,
                            &path(p, "direction"),
                            "nested-local-adaptive-direction-not-representable",
                        )
                    }
                };
                approximate(m, &path(p, "direction"), &format!("{t}/element/type"));
                mark(m, p, "node", t, "element");
                let gap_target = if columns > 1 { "columnGap" } else { "spacing" };
                mark(m, p, "gapDp", &format!("{t}/element"), gap_target);
                mark(m, p, "paddingDp", &format!("{t}/layout"), "padding");
                approximate(m, &path(p, "columns"), &format!("{t}/element"));
                let mut out = Vec::new();
                for (i, c) in children.iter().enumerate() {
                    out.push(self.node(
                        c,
                        &format!("{p}/children/{i}"),
                        &format!("{t}/children/{i}"),
                        m,
                        depth + 1,
                        row,
                    )?);
                }
                if children.is_empty() {
                    mark(m, p, "children", t, "children");
                }
                let mut layout = json!({"width":{"mode":"fill"},"padding":{"top":pad,"bottom":pad,"start":pad,"end":pad}});
                if parent_row {
                    layout["weight"] = json!(weight);
                }
                approximate(m, &path(p, "weight"), &format!("{t}/layout"));
                if columns > 1 {
                    for (i, child) in out.iter_mut().enumerate() {
                        let i = i as u64;
                        child["layout"]["gridCell"] =
                            json!({"column":i % columns,"row":i / columns});
                    }
                    let rows = (out.len() as u64).div_ceil(columns);
                    let cols = vec![json!({"mode":"fraction","weight":1}); columns as usize];
                    let rows = vec![json!({"mode":"auto"}); rows as usize];
                    return Some(
                        json!({"id":id,"element":{"type":"grid","columns":cols,"rows":rows,"columnGap":gap,"rowGap":gap},"layout":layout,"children":out}),
                    );
                }
                Some(json!({"id":id,"element":{"type":if row{"row"}else{"column"},"spacing":gap,"wrap":row},"layout":layout,"children":out}))
                    .map(|mut n|{if !row{n["element"].as_object_mut().map(|o|o.remove("wrap"));}n})
            }
            "component" => {
                let key = v.get("componentId")?.as_str()?;
                let mut n = self.nodes.remove(key).or_else(|| {
                    m.fail(&path(p, "componentId"), "unknown-or-duplicate-placement");
                    None
                })?;
                let source = self.source.get(key)?;
                let np = path(&self.p, key);
                if matches!(source["type"].as_str(), Some("line" | "polygon" | "arrow")) {
                    return refuse(m, &np, "relative-primitive-layout-not-representable");
                }
                if source.get("visible").and_then(Value::as_bool) == Some(false)
                    || source
                        .get("bindings")
                        .is_some_and(|x| x.get("visible").is_some())
                    || source
                        .get("expressions")
                        .is_some_and(|x| x.get("visible").is_some())
                {
                    return refuse(
                        m,
                        &path(&np, "visible"),
                        "adaptive-hidden-layout-not-mapped",
                    );
                }
                preserve_geometry(m, &np, t, &mut n);
                if let Some(z) = source.get("zIndex") {
                    n["extensions"]["hmiforge.legacyZIndex"] = z.clone();
                    if let Some(f) = m.findings.get_mut(&path(&np, "zIndex")) {
                        f.disposition = "preserved";
                        f.code = "legacy-z-index-preserved";
                        f.target = Some(format!("{t}/extensions/hmiforge.legacyZIndex"));
                        f.message = "Original z-index preserved as inactive design metadata.";
                    }
                }
                let minw = dim(v.get("minWidthDp"), 160.)?;
                let minh = dim(v.get("minHeightDp"), 64.)?;
                if minw < 0. || minh < 0. {
                    return None;
                }
                let mut layout = json!({"width":{"mode":"fill"},"minWidth":minw,"minHeight":minh});
                if parent_row {
                    layout["weight"] = json!(weight);
                }
                for (k, canonical, min) in [
                    ("maxWidthDp", "maxWidth", minw),
                    ("maxHeightDp", "maxHeight", minh),
                ] {
                    if let Some(x) = v.get(k) {
                        if !x.is_null() {
                            if k == "maxHeightDp" && self.ver < 3 {
                                return None;
                            }
                            let val = dim(Some(x), 0.)?;
                            if val < min {
                                return None;
                            }
                            layout[canonical] = json!(val);
                        }
                        mark(m, p, k, &format!("{t}/layout"), canonical);
                    }
                }
                for k in ["alignH", "alignV"] {
                    if let Some(x) = v.get(k) {
                        if !x.is_null() {
                            return refuse(m, &path(p, k), "slot-alignment-not-mapped");
                        }
                        mark(m, p, k, t, "layout");
                    }
                }
                n["layout"] = layout;
                for (k, dest) in [
                    ("node", "element"),
                    ("componentId", "id"),
                    ("weight", "layout/weight"),
                    ("minWidthDp", "layout/minWidth"),
                    ("minHeightDp", "layout/minHeight"),
                ] {
                    approximate(m, &path(p, k), &format!("{t}/{dest}"));
                }
                // Update previous component findings to the final nested target.
                for (k, f) in &mut m.findings {
                    if k.starts_with(&format!("{np}/")) && f.disposition == "mapped" {
                        if let Some(old) = &f.target {
                            if let Some((_, suffix)) = old.split_once("/root/children/") {
                                if let Some((_, tail)) = suffix.split_once('/') {
                                    f.target = Some(format!("{t}/{tail}"));
                                } else {
                                    f.target = Some(t.into());
                                }
                            }
                        }
                    }
                }
                Some(n)
            }
            "scaleGroup" => refuse(
                m,
                p,
                "scale-group-uniform-scale-and-reflow-not-representable",
            ),
            _ => refuse(m, &path(p, "node"), "layout-node-not-representable"),
        }
    }
}
fn adaptive(
    source: &Value,
    root: &mut Value,
    p: &str,
    t: &str,
    m: &mut Mapping,
    ver: u64,
) -> Option<()> {
    let Some(v) = source.get("layout") else {
        return Some(());
    };
    if v.is_null() {
        m.mark(&path(p, "layout"), t);
        return Some(());
    }
    if ver < 2 {
        return refuse(m, &path(p, "layout"), "layout-requires-v2");
    }
    let lp = path(p, "layout");
    if v.get("version").map(Value::as_u64).unwrap_or(Some(1)) != Some(1) {
        return None;
    }
    m.mark(&path(&lp, "version"), t);
    let empty = serde_json::Map::new();
    let comps = source
        .get("components")
        .or_else(|| source.get("elements"))
        .map(Value::as_object)
        .unwrap_or(Some(&empty))?;
    let prefix = if source.get("components").is_some() {
        "components"
    } else {
        "elements"
    };
    let cp = path(p, prefix);
    let mut nodes = BTreeMap::new();
    for (key, _) in comps {
        let id = m.ids.get(&path(&path(&cp, key), "id"))?;
        let node = root["children"]
            .as_array()?
            .iter()
            .find(|n| n["id"].as_str() == Some(id))?;
        nodes.insert(key.clone(), node.clone());
    }
    let mut l = Layout {
        source: comps,
        nodes,
        used_stack: BTreeSet::new(),
        p: cp,
        ver,
        design_width: root["layout"]["width"]["value"].as_f64()?,
        design_height: root["layout"]["height"]["value"].as_f64()?,
    };
    let mut n = l.node(v.get("root")?, &path(&lp, "root"), t, m, 0, false)?;
    if !l.nodes.is_empty() {
        return refuse(m, &lp, "unplaced-components");
    }
    // Keep the design dimensions from the prior root. Actual reflow remains a warning.
    n["layout"]["width"] = root["layout"]["width"].clone();
    n["layout"]["height"] = root["layout"]["height"].clone();
    m.ids.remove(&path(p, "@generated-root"));
    *root = n;
    Some(())
}

// Lexer-aware remapping: validate the original and rewritten expression using
// the pinned contract. Never replace substrings, quoted literals or function names.
fn rewrite(
    s: &str,
    types: &BTreeMap<String, Type>,
    ids: &BTreeMap<String, String>,
) -> Option<String> {
    let old = compile(s, types).ok()?;
    let refs: BTreeSet<_> = old.references().iter().map(String::as_str).collect();
    let c: Vec<char> = s.chars().collect();
    let mut i = 0;
    let mut out = String::new();
    while i < c.len() {
        let a = c[i];
        let start = i;
        if matches!(a, '"' | '\'') {
            i += 1;
            while i < c.len() {
                let ch = c[i];
                i += 1;
                if ch == '\\' {
                    i += 1;
                } else if ch == a {
                    break;
                }
            }
            out.extend(c.get(start..i)?.iter());
        } else if a == '`' {
            i += 1;
            while i < c.len() && c[i] != '`' {
                i += 1;
            }
            let name: String = c.get(start + 1..i)?.iter().collect();
            i += 1;
            out.push('`');
            out.push_str(ids.get(&name)?);
            out.push('`');
        } else if a.is_ascii_digit() || (a == '.' && c.get(i + 1).is_some_and(char::is_ascii_digit))
        {
            i += 1;
            while i < c.len() && (c[i].is_ascii_digit() || c[i] == '.') {
                i += 1;
            }
            if matches!(c.get(i), Some('e' | 'E')) {
                i += 1;
                if matches!(c.get(i), Some('+' | '-')) {
                    i += 1;
                }
                while i < c.len() && c[i].is_ascii_digit() {
                    i += 1;
                }
            }
            out.extend(c[start..i].iter());
        } else if a.is_alphabetic() || a == '_' {
            i += 1;
            while i < c.len()
                && (c[i].is_alphanumeric()
                    || c[i] == '_'
                    || (c[i] == '.'
                        && c.get(i + 1).is_some_and(|x| x.is_alphabetic() || *x == '_')))
            {
                i += 1;
            }
            let name: String = c[start..i].iter().collect();
            let call = c[i..].iter().find(|x| !x.is_whitespace()) == Some(&'(');
            if !call && name != "true" && name != "false" && refs.contains(name.as_str()) {
                out.push('`');
                out.push_str(ids.get(&name)?);
                out.push('`');
            } else {
                out.push_str(&name);
            }
        } else {
            out.push(a);
            i += 1;
        }
    }
    let new_types = types
        .iter()
        .map(|(k, v)| Some((ids.get(k)?.clone(), *v)))
        .collect::<Option<BTreeMap<_, _>>>()?;
    let new = compile(&out, &new_types).ok()?;
    let expected = old
        .references()
        .iter()
        .map(|k| ids.get(k).cloned())
        .collect::<Option<Vec<_>>>()?;
    (new.result_type() == old.result_type() && new.references() == expected).then_some(out)
}
fn target(kind: &str, slot: &str) -> Option<&'static str> {
    match (kind, slot) {
        (_, "visible") => Some("visible"),
        (_, "opacity") => Some("opacity"),
        ("label" | "button", "text") => Some("text"),
        ("numericField", "value") => Some("value"),
        ("indicator", "state") => Some("active"),
        ("rectangle" | "ellipse", "fillColor") => Some("fill"),
        ("rectangle" | "ellipse", "strokeColor") => Some("strokeColor"),
        _ => None,
    }
}
fn node_mut<'a>(n: &'a mut Value, id: &str, t: &str) -> Option<(&'a mut Value, String)> {
    if n["id"].as_str() == Some(id) {
        return Some((n, t.into()));
    }
    for (i, c) in n
        .get_mut("children")?
        .as_array_mut()?
        .iter_mut()
        .enumerate()
    {
        if let Some(r) = node_mut(c, id, &format!("{t}/children/{i}")) {
            return Some(r);
        }
    }
    None
}
struct BindingContext<'a> {
    types: &'a BTreeMap<String, Type>,
    ids: &'a BTreeMap<String, String>,
    writable: &'a BTreeSet<String>,
    expr: &'a mut Vec<Value>,
    ver: u64,
}
fn bindings(
    v: &Value,
    n: &mut Value,
    p: &str,
    t: &str,
    m: &mut Mapping,
    ctx: &mut BindingContext<'_>,
) -> Option<()> {
    let kind = v["type"].as_str()?;
    let mut bound = BTreeSet::new();
    let mut bs = Vec::new();
    if let Some(b) = v.get("bindings") {
        let b = b.as_object()?;
        if b.is_empty() {
            mark(m, p, "bindings", t, "bindings");
        }
        for (slot, x) in b {
            let sp = path(&path(p, "bindings"), slot);
            let property = target(kind, slot).or_else(|| {
                m.fail(&sp, "unsupported-binding-slot");
                None
            })?;
            let var = x.as_str()?;
            let ty = *ctx.types.get(var)?;
            if kind == "numericField" && ctx.writable.contains(var) {
                return refuse(m, &sp, "writable-numeric-field-not-mapped");
            }
            if !bound.insert(property) {
                return None;
            }
            // Do not coerce number/bool to text, or strings to theme colors.
            if !matches!(
                (property, ty),
                ("visible" | "active", Type::Bool)
                    | ("opacity" | "value", Type::Number)
                    | ("text", Type::Text)
            ) {
                return refuse(m, &sp, "binding-type-not-lossless");
            }
            bs.push(json!({"property":property,"source":{"tag":ctx.ids.get(var)?},"mode":"read"}));
            m.mark(&sp, &format!("{t}/bindings/{}/source/tag", bs.len() - 1));
        }
    }
    if let Some(e) = v.get("expressions") {
        let e = e.as_object()?;
        if e.is_empty() {
            mark(m, p, "expressions", t, "bindings");
        }
        if !e.is_empty() && ctx.ver < 4 {
            return None;
        }
        for (slot, x) in e {
            let sp = path(&path(p, "expressions"), slot);
            let property = target(kind, slot)?;
            if !bound.insert(property) {
                return refuse(m, &sp, "duplicate-binding-target");
            }
            let text = x.as_str()?;
            let rewritten = rewrite(text, ctx.types, ctx.ids).or_else(|| {
                m.fail(&sp, "invalid-or-unresolved-expression");
                None
            })?;
            let et = format!("/v2/bindings/expressions/{}", ctx.expr.len());
            ctx.expr.push(json!({"node":n["id"],"property":property,"contractVersion":CONTRACT_VERSION,"expression":rewritten}));
            m.mark(&sp, &format!("{et}/expression"));
        }
    }
    n["bindings"] = json!(bs);
    Some(())
}
pub(super) fn finish(v: &Value, doc: &mut Value, m: &mut Mapping) -> Option<()> {
    let ver = v["schemaVersion"].as_u64()?;
    let mut types = BTreeMap::new();
    let mut writable = BTreeSet::new();
    let mut ids = BTreeMap::new();
    if let Some(vars) = v.get("variables") {
        for (k, x) in vars.as_object()? {
            let ty = match x["type"].as_str()? {
                "BOOL" => Type::Bool,
                "STRING" => Type::Text,
                _ => Type::Number,
            };
            types.insert(k.clone(), ty);
            if x.get("access")
                .and_then(Value::as_str)
                .is_some_and(|a| a != "READ_ONLY")
            {
                writable.insert(k.clone());
            }
            ids.insert(
                k.clone(),
                m.ids.get(&path(&path("/variables", k), "id"))?.clone(),
            );
        }
    }
    let mut expr = Vec::new();
    let mut definitions = Vec::new();
    let mut classifications = Vec::new();
    let mut def_ids = BTreeMap::new();
    if let Some(ds) = v.get("componentDefinitions") {
        let ds = ds.as_object()?;
        if ds.is_empty() {
            m.mark("/componentDefinitions", "/components");
        }
        // Allocate all reusable IDs before placing instances.
        for (k, x) in ds {
            let p = path("/componentDefinitions", k);
            if x["id"].as_str() != Some(k) || x["type"].as_str() != Some("composite") || ver < 6 {
                return refuse(m, &p, "standard-or-unsupported-definition");
            }
            let id = m.id(&path(&p, "id"), k, "definition");
            def_ids.insert(k.clone(), id);
        }
        for (k, x) in ds {
            let p = path("/componentDefinitions", k);
            let t = format!("/components/{}", definitions.len());
            let id = def_ids.get(k)?;
            mark(m, &p, "id", &t, "id");
            mark(m, &p, "type", &t, "root");
            if let Some(kind) = x.get("kind") {
                match kind.as_str()? {
                    "COMPONENT" => mark(m, &p, "kind", &t, "root"),
                    "FACEPLATE" => {
                        let ct = format!("/v2/components/definitions/{}", classifications.len());
                        classifications
                            .push(json!({"definition":id,"classification":{"kind":"faceplate"}}));
                        mark(m, &p, "kind", &ct, "classification");
                    }
                    _ => return refuse(m, &path(&p, "kind"), "unknown-definition-kind"),
                }
            }
            for key in ["bindingSlots", "defaultProperties"] {
                if let Some(v) = x.get(key) {
                    m.empty(v, &path(&p, key), &t);
                }
            }
            let name = m.text(
                x.get("displayName")?,
                &path(&p, "displayName"),
                &path(&t, "name"),
            )?;
            let (params, param_types) = parameters(x, &p, &t, m)?;
            let c = x.get("composition")?;
            let cp = path(&p, "composition");
            let mut root = json!({"id":m.id(&path(&cp,"@generated-root"),&format!("legacy-definition-root-{}",definitions.len()),"root"),"element":{"type":"absolute"},"layout":{"width":fixed(dim(c.get("widthDp"),360.)?),"height":fixed(dim(c.get("heightDp"),200.)?)},"children":[]});
            for (k, target) in [("widthDp", "width"), ("heightDp", "height")] {
                m.mark(&path(&cp, k), &format!("{t}/root/layout/{target}/value"));
            }
            if let Some(es) = c.get("elements") {
                let es = es.as_object()?;
                if es.is_empty() {
                    m.mark(&path(&cp, "elements"), &format!("{t}/root/children"));
                }
                let mut es: Vec<_> = es.iter().collect();
                es.sort_by_key(|(k, x)| (x.get("zIndex").and_then(Value::as_i64).unwrap_or(0), *k));
                let mut z = BTreeSet::new();
                for (j, (key, e)) in es.into_iter().enumerate() {
                    if c.get("layout").is_none_or(Value::is_null)
                        && !z.insert(e.get("zIndex").and_then(Value::as_i64).unwrap_or(0))
                    {
                        return None;
                    }
                    let ep = path(&path(&cp, "elements"), key);
                    let et = format!("{t}/root/children/{j}");
                    // Composition bindings refer to local typed parameters.
                    if e["type"] == "image" {
                        return refuse(m, &ep, "nested-image-not-mapped");
                    }
                    if e["type"] == "dropDown" {
                        return refuse(m, &ep, "nested-dropdown-not-mapped");
                    }
                    if super::primitives::symbol_kind(e["type"].as_str()?).is_some() {
                        return refuse(m, &ep, "nested-industrial-symbol-not-mapped");
                    }
                    if e["type"] == "composite"
                        && v["componentDefinitions"][e["componentDefinitionId"].as_str()?]["kind"]
                            == "FACEPLATE"
                    {
                        return refuse(m, &ep, "faceplate-direct-placement");
                    }
                    let mut n = component(e, key, &ep, &et, m)?;
                    resolve_instance(e, &mut n, &ep, &et, m, &def_ids)?;
                    parameter_bindings(e, &mut n, &ep, &et, m, &param_types)?;
                    root["children"].as_array_mut()?.push(n);
                }
            }
            adaptive(c, &mut root, &cp, &format!("{t}/root"), m, ver)?;
            let mut def = json!({"id":id,"name":name,"root":root,"parameters":params});
            for key in ["category", "description"] {
                if let Some(x) = x.get(key) {
                    if x.is_null() {
                        mark(m, &p, key, &t, key);
                    } else {
                        def[key] = json!(m.text(x, &path(&p, key), &path(&t, key))?);
                    }
                }
            }
            definitions.push(def);
        }
    }
    let def_params: BTreeMap<String, Value> = definitions
        .iter()
        .map(|d| Some((d["id"].as_str()?.to_owned(), d["parameters"].clone())))
        .collect::<Option<_>>()?;
    let mut symbol_defs = Vec::new();
    for (i, s) in doc["screens"].as_array_mut()?.iter_mut().enumerate() {
        let (key, source) = v["screens"].as_object()?.iter().find(|(k, _)| {
            m.ids
                .get(&path(&path("/screens", k), "id"))
                .is_some_and(|id| s["id"].as_str() == Some(id))
        })?;
        let p = path("/screens", key);
        let t = format!("/screens/{i}/root");
        adaptive(source, &mut s["root"], &p, &t, m, ver)?;
        if let Some(cs) = source.get("components") {
            for (k, c) in cs.as_object()? {
                let cp = path(&path(&p, "components"), k);
                let id = m.ids.get(&path(&cp, "id"))?.clone();
                let (n, nt) = node_mut(&mut s["root"], &id, &t)?;
                resolve_instance(c, n, &cp, &nt, m, &def_ids)?;
                if c["type"].as_str() == Some("composite") {
                    let key = c["componentDefinitionId"].as_str()?;
                    if v["componentDefinitions"][key]["kind"] == "FACEPLATE" {
                        return refuse(m, &cp, "faceplate-direct-placement");
                    }
                    instance_parameters(
                        c,
                        n,
                        &cp,
                        &nt,
                        m,
                        def_params.get(n["element"]["definition"].as_str()?)?,
                        &ids,
                    )?;
                } else if let Some(kind) = super::primitives::symbol_kind(c["type"].as_str()?) {
                    let (def, class) = super::primitives::symbol(c, n, &cp, &nt, kind, &ids, m)?;
                    symbol_defs.push(def);
                    classifications.push(class);
                } else if c["type"] == "image" {
                    super::resources::image(c, n, &cp, &nt, m, ver)?;
                } else if c["type"] == "dropDown" {
                    if ver < 10 {
                        return refuse(m, &cp, "dropdown-version-mismatch");
                    }
                } else {
                    bindings(
                        c,
                        n,
                        &cp,
                        &nt,
                        m,
                        &mut BindingContext {
                            types: &types,
                            ids: &ids,
                            writable: &writable,
                            expr: &mut expr,
                            ver,
                        },
                    )?;
                }
            }
        }
    }
    let mut enums = Vec::new();
    if let Some(es) = v.get("enumerations") {
        let es = es.as_object()?;
        if es.is_empty() {
            m.mark("/enumerations", "/v2/bindings/enumerations");
        }
        if !es.is_empty() && ver < 10 {
            return None;
        }
        for (k, x) in es {
            let p = path("/enumerations", k);
            let t = format!("/v2/bindings/enumerations/{}", enums.len());
            if x["id"].as_str() != Some(k) {
                return None;
            }
            let id = m.id(&path(&p, "id"), k, "enum");
            mark(m, &p, "id", &t, "id");
            let name = m.text(x.get("name")?, &path(&p, "name"), &path(&t, "name"))?;
            let mut entries = Vec::new();
            for (j, e) in x.get("entries")?.as_array()?.iter().enumerate() {
                let ep = format!("{p}/entries/{j}");
                let et = format!("{t}/entries/{j}");
                let value = e.get("value")?.as_i64()?;
                mark(m, &ep, "value", &et, "value");
                let text = m.text(e.get("text")?, &path(&ep, "text"), &path(&et, "label"))?;
                let mut entry = json!({"value":value,"label":text});
                if let Some(c) = e.get("color") {
                    if !c.is_null() {
                        let c = c.as_str()?;
                        if !color(c) {
                            return refuse(m, &path(&ep, "color"), "theme-color-not-mapped");
                        }
                        entry["color"] = json!(c);
                    }
                    mark(m, &ep, "color", &et, "color");
                }
                entries.push(entry);
            }
            enums.push(json!({"id":id,"name":name,"dataType":"int64","entries":entries}));
        }
    }
    definitions.extend(symbol_defs);
    doc["components"] = json!(definitions);
    if !classifications.is_empty() {
        doc["v2"]["components"]["definitions"] = json!(classifications);
    }
    doc["v2"]["bindings"] = json!({"expressions":expr,"enumerations":enums});
    super::primitives::dropdowns(v, doc, m)?;
    Some(())
}
fn resolve_instance(
    c: &Value,
    n: &mut Value,
    p: &str,
    t: &str,
    m: &mut Mapping,
    ids: &BTreeMap<String, String>,
) -> Option<()> {
    if c["type"].as_str() == Some("composite") {
        let key = c.get("componentDefinitionId")?.as_str()?;
        let id = ids.get(key).or_else(|| {
            m.fail(&path(p, "componentDefinitionId"), "unknown-definition");
            None
        })?;
        n["element"]["definition"] = json!(id);
        m.mark(
            &path(p, "componentDefinitionId"),
            &format!("{t}/element/definition"),
        );
    }
    Some(())
}

fn parameters(
    x: &Value,
    p: &str,
    t: &str,
    m: &mut Mapping,
) -> Option<(Vec<Value>, BTreeMap<String, Type>)> {
    let mut params = Vec::new();
    let mut types = BTreeMap::new();
    if let Some(ps) = x.get("parameters") {
        let ps = ps.as_object()?;
        if ps.is_empty() {
            mark(m, p, "parameters", t, "parameters");
        }
        for (k, x) in ps {
            let sp = path(&path(p, "parameters"), k);
            let tp = format!("{t}/parameters/{}", params.len());
            let (ty, dt) = match x.get("type")?.as_str()? {
                "BOOL_VARIABLE" => (Type::Bool, "bool"),
                "TEXT_VARIABLE" => (Type::Text, "string"),
                _ => return refuse(m, &path(&sp, "type"), "reusable-parameter-type-not-mapped"),
            };
            if x.get("writable")
                .is_some_and(|v| v.as_bool() != Some(false))
            {
                return refuse(
                    m,
                    &path(&sp, "writable"),
                    "writable-reusable-parameter-not-mapped",
                );
            }
            if x.get("default").is_some_and(|v| !v.is_null())
                || x.get("overridable")
                    .is_some_and(|v| v.as_bool() != Some(true))
            {
                return refuse(m, &sp, "reusable-parameter-policy-not-mapped");
            }
            let mut param = json!({"name":k,"type":{"kind":"tag","dataType":dt,"writable":false}});
            for (key, dest) in [
                ("type", "type"),
                ("writable", "type/writable"),
                ("default", "type"),
                ("overridable", "type"),
            ] {
                m.mark(&path(&sp, key), &format!("{tp}/{dest}"));
            }
            if let Some(d) = x.get("description") {
                if !d.is_null() {
                    param["description"] =
                        json!(m.text(d, &path(&sp, "description"), &path(&tp, "description"))?);
                } else {
                    mark(m, &sp, "description", &tp, "description");
                }
            }
            types.insert(k.clone(), ty);
            params.push(param);
        }
    }
    Some((params, types))
}
fn parameter_bindings(
    v: &Value,
    n: &mut Value,
    p: &str,
    t: &str,
    m: &mut Mapping,
    types: &BTreeMap<String, Type>,
) -> Option<()> {
    if v.get("expressions")
        .is_some_and(|x| x.as_object().is_none_or(|o| !o.is_empty()))
    {
        return refuse(
            m,
            &path(p, "expressions"),
            "reusable-parameter-expression-not-mapped",
        );
    }
    let kind = v["type"].as_str()?;
    let mut out = Vec::new();
    if let Some(b) = v.get("bindings") {
        for (k, x) in b.as_object()? {
            let sp = path(&path(p, "bindings"), k);
            let name = x.as_str()?;
            let ty = *types.get(name)?;
            let prop = target(kind, k)?;
            if kind == "composite"
                || !matches!(
                    (prop, ty),
                    ("visible" | "active", Type::Bool) | ("text", Type::Text)
                )
            {
                return refuse(m, &sp, "reusable-binding-type-not-mapped");
            }
            out.push(json!({"property":prop,"source":{"tagParameter":name},"mode":"read"}));
            m.mark(
                &sp,
                &format!("{t}/bindings/{}/source/tagParameter", out.len() - 1),
            );
        }
    }
    n["bindings"] = json!(out);
    Some(())
}
fn instance_parameters(
    v: &Value,
    n: &mut Value,
    p: &str,
    t: &str,
    m: &mut Mapping,
    params: &Value,
    ids: &BTreeMap<String, String>,
) -> Option<()> {
    let mut values = json!({});
    if v.get("expressions")
        .is_some_and(|x| x.as_object().is_none_or(|o| !o.is_empty()))
    {
        return refuse(
            m,
            &path(p, "expressions"),
            "instance-parameter-expression-not-mapped",
        );
    }
    if let Some(bs) = v.get("bindings") {
        for (k, x) in bs.as_object()? {
            let sp = path(&path(p, "bindings"), k);
            if !params
                .as_array()?
                .iter()
                .any(|p| p["name"].as_str() == Some(k))
            {
                return refuse(m, &sp, "unknown-instance-parameter");
            }
            values[k] = json!(ids.get(x.as_str()?)?);
            m.mark(&sp, &path(&format!("{t}/element/parameters"), k));
        }
    }
    n["element"]["parameters"] = values;
    Some(())
}
