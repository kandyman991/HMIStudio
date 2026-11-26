//! Read-only portable legacy import, initial conservative mapping slice.
//! No filesystem writes, archive extraction, I/O, or runtime authorization.
use hmi_project_model::{model::Project, serialization::load_project};
mod graphics;
mod industrial;
mod navigation;
mod primitives;
mod resources;
use serde::{de, Deserialize, Deserializer, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

/// Maximum admitted portable JSON bytes.
pub const MAX_INPUT_BYTES: usize = 4 * 1024 * 1024;
const MAX_LEAVES: usize = 20_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub path: String,
    pub code: &'static str,
    pub severity: &'static str,
    pub disposition: &'static str,
    pub target: Option<String>,
    pub message: &'static str,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationReport {
    pub source_format: &'static str,
    pub source_schema_version: Option<u64>,
    pub source_sha256: String,
    pub importer_version: &'static str,
    pub findings: Vec<Finding>,
    pub id_map: BTreeMap<String, String>,
}
#[derive(Debug)]
pub struct ImportOutcome {
    /// None whenever any source field or validation gate is refused.
    pub project: Option<Project>,
    pub report: MigrationReport,
    /// Verified bytes keyed by generated package-relative path; empty on refusal.
    pub resource_bytes: BTreeMap<String, Vec<u8>>,
    /// Review-only declarations; never part of a canonical executable project.
    pub review_only_alarms: Vec<ReviewOnlyAlarm>,
}

/// Legacy alarm declaration with unresolved activation semantics.
/// Kept separately from the value-free migration report. Do not publish it.
#[derive(Clone, PartialEq, Eq)]
pub struct ReviewOnlyAlarm {
    pub source_path: String,
    pub id: String,
    pub source_variable_id: String,
    pub message: String,
    pub severity: String,
}
impl std::fmt::Debug for ReviewOnlyAlarm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReviewOnlyAlarm")
            .field("source_path", &self.source_path)
            .finish_non_exhaustive()
    }
}

// A fresh generic visitor rejects repeated keys at every level before mapping.
// serde_json retains its default recursion limit.
struct Unique(Value);
impl<'de> Deserialize<'de> for Unique {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> de::Visitor<'de> for V {
            type Value = Unique;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("unique-key JSON")
            }
            fn visit_bool<E: de::Error>(self, x: bool) -> Result<Unique, E> {
                Ok(Unique(json!(x)))
            }
            fn visit_i64<E: de::Error>(self, x: i64) -> Result<Unique, E> {
                Ok(Unique(json!(x)))
            }
            fn visit_u64<E: de::Error>(self, x: u64) -> Result<Unique, E> {
                Ok(Unique(json!(x)))
            }
            fn visit_f64<E: de::Error>(self, x: f64) -> Result<Unique, E> {
                Ok(Unique(json!(x)))
            }
            fn visit_str<E: de::Error>(self, x: &str) -> Result<Unique, E> {
                Ok(Unique(json!(x)))
            }
            fn visit_unit<E: de::Error>(self) -> Result<Unique, E> {
                Ok(Unique(Value::Null))
            }
            fn visit_seq<A: de::SeqAccess<'de>>(self, mut a: A) -> Result<Unique, A::Error> {
                let mut v = Vec::new();
                while let Some(Unique(x)) = a.next_element()? {
                    v.push(x);
                }
                Ok(Unique(Value::Array(v)))
            }
            fn visit_map<A: de::MapAccess<'de>>(self, mut a: A) -> Result<Unique, A::Error> {
                let mut v = serde_json::Map::new();
                while let Some((k, Unique(x))) = a.next_entry::<String, Unique>()? {
                    if v.insert(k, x).is_some() {
                        return Err(de::Error::custom("duplicate object key"));
                    }
                }
                Ok(Unique(Value::Object(v)))
            }
        }
        d.deserialize_any(V)
    }
}
fn path(p: &str, k: &str) -> String {
    format!("{p}/{}", k.replace('~', "~0").replace('/', "~1"))
}
fn refused(p: String, code: &'static str) -> Finding {
    Finding {
        path: p,
        code,
        severity: "blocking",
        disposition: "refused",
        target: None,
        message: "Source field or admission gate is unsupported or invalid; no project emitted.",
    }
}
fn leaves(v: &Value, p: &str, out: &mut BTreeMap<String, Finding>) -> bool {
    if p.len() > 1024 {
        return false;
    }
    match v {
        Value::Object(o) if !o.is_empty() => {
            for (k, x) in o {
                if !leaves(x, &path(p, k), out) {
                    return false;
                }
            }
        }
        Value::Array(a) if !a.is_empty() => {
            for (i, x) in a.iter().enumerate() {
                if !leaves(x, &path(p, &i.to_string()), out) {
                    return false;
                }
            }
        }
        _ => {
            if out.len() >= MAX_LEAVES {
                return false;
            }
            let code = if p.starts_with("/externalSources/") || p.starts_with("/externalMappings/")
            {
                "external-io-mapping-not-supported"
            } else if p.starts_with("/security/") || p.ends_with("/requiredPermission") {
                "security-mapping-not-supported"
            } else if p.contains("/actions/") {
                "action-mapping-not-supported"
            } else if p.contains("/expressions/") {
                "expression-mapping-not-supported"
            } else {
                "unsupported-or-invalid-field"
            };
            out.insert(p.into(), refused(p.into(), code));
        }
    }
    true
}
struct Mapping {
    findings: BTreeMap<String, Finding>,
    ids: BTreeMap<String, String>,
    used: BTreeSet<String>,
    contracts: Vec<Value>,
    resource_bytes: BTreeMap<String, Vec<u8>>,
    theme_colors: BTreeMap<String, String>,
    named_styles: BTreeMap<String, Value>,
    default_styles: BTreeMap<String, Value>,
    styles: Vec<Value>,
}
impl Mapping {
    fn mark(&mut self, p: &str, t: &str) {
        if let Some(f) = self.findings.get_mut(p) {
            f.code = "mapped";
            f.severity = "info";
            f.disposition = "mapped";
            f.target = Some(t.into());
            f.message = "Field accounted for by canonical mapping (proposed if import is refused).";
        }
    }
    fn fail(&mut self, p: &str, code: &'static str) {
        self.findings.insert(p.into(), refused(p.into(), code));
    }
    fn id(&mut self, p: &str, old: &str, prefix: &str) -> String {
        let mut id = old.to_owned();
        if hmi_project_model::model::ProjectId::new(&id).is_err() || self.used.contains(&id) {
            let mut i = 1;
            loop {
                id = format!("legacy-{prefix}-{i}");
                if !self.used.contains(&id) {
                    break;
                }
                i += 1;
            }
        }
        self.used.insert(id.clone());
        self.ids.insert(p.into(), id.clone());
        id
    }
    fn text(&mut self, v: &Value, p: &str, t: &str) -> Option<String> {
        let s = v.as_str()?;
        self.mark(p, t);
        Some(s.into())
    }
    fn empty(&mut self, v: &Value, p: &str, t: &str) {
        if match p.rsplit('/').next().unwrap_or("") {
            "screenOrder" | "targetProfiles" => v.as_array().is_some_and(|x| x.is_empty()),
            "layout"
            | "requiredPermission"
            | "componentDefinitionId"
            | "parentId"
            | "theme"
            | "security" => v.is_null(),
            _ => v.as_object().is_some_and(|x| x.is_empty()),
        } {
            self.mark(p, t);
        }
    }
}
fn dim(v: Option<&Value>, default: f64) -> Option<f64> {
    match v {
        None => Some(default),
        Some(v) => {
            let f = v.as_f64()? as f32;
            f.is_finite().then_some(f64::from(f))
        }
    }
}
fn fixed(v: f64) -> Value {
    json!({"mode":"fixed","value":v})
}

/// Detect and map portable legacy JSON. Versions 1–10 are recognized; the
/// initial supported subset is identical across versions. Unsupported features
/// refuse the entire result, and report only paths/codes, not source values. The report may contain source key names; keep it local.
pub fn import_portable(input: &[u8]) -> ImportOutcome {
    let mut report = MigrationReport {
        source_format: "hmiforge",
        source_schema_version: None,
        source_sha256: format!("{:x}", Sha256::digest(input)),
        importer_version: "0.1.0",
        findings: vec![],
        id_map: BTreeMap::new(),
    };
    let stop = |mut r: MigrationReport, code| {
        r.findings.push(refused("".into(), code));
        ImportOutcome {
            project: None,
            report: r,
            resource_bytes: BTreeMap::new(),
            review_only_alarms: Vec::new(),
        }
    };
    if input.len() > MAX_INPUT_BYTES {
        return stop(report, "input-too-large");
    }
    let Ok(Unique(v)) = serde_json::from_slice::<Unique>(input) else {
        return stop(report, "invalid-or-duplicate-json");
    };
    let project_id = format!("legacy-project-{}", &report.source_sha256[..24]);
    let mut m = Mapping {
        findings: BTreeMap::new(),
        ids: BTreeMap::new(),
        used: BTreeSet::from([project_id.clone()]),
        contracts: Vec::new(),
        resource_bytes: BTreeMap::new(),
        theme_colors: BTreeMap::new(),
        named_styles: BTreeMap::new(),
        default_styles: BTreeMap::new(),
        styles: Vec::new(),
    };
    if !leaves(&v, "", &mut m.findings) {
        return stop(report, "leaf-or-path-limit");
    }
    let ver = v.get("schemaVersion").and_then(Value::as_u64);
    report.source_schema_version = ver;
    let mut review_only_alarms = Vec::new();
    let project = if matches!(ver, Some(1..=10)) && v.get("format").is_none() {
        review_only_alarms = review_alarms(&v, &mut m);
        m.mark("/schemaVersion", "/schemaVersion");
        map_project(&v, &mut m, &project_id)
    } else {
        m.fail("/schemaVersion", "unsupported-or-missing-version");
        None
    };
    if project.is_none() {
        m.fail("", "invalid-or-unsupported-project");
    }
    report.id_map = m.ids;
    report.findings = m.findings.into_values().collect();
    let project = if report.findings.iter().any(|f| f.disposition == "refused") {
        None
    } else {
        project
    };
    let resource_bytes = if project.is_some() {
        m.resource_bytes
    } else {
        BTreeMap::new()
    };
    ImportOutcome {
        project,
        report,
        resource_bytes,
        review_only_alarms,
    }
}

// Preserve recognized declarations as review-only data without guessing
// the trigger, acknowledgement, or severity conversion. All leaves remain
// blocking so no incomplete executable project can be emitted.
fn review_alarms(v: &Value, m: &mut Mapping) -> Vec<ReviewOnlyAlarm> {
    let Some(alarms) = v.get("alarms").and_then(Value::as_object) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (id, alarm) in alarms {
        let p = path("/alarms", id);
        let parsed = (|| {
            let fields = alarm.as_object()?;
            if fields.keys().any(|k| {
                !matches!(
                    k.as_str(),
                    "id" | "sourceVariableId" | "message" | "severity"
                )
            }) || id.is_empty()
                || alarm["id"].as_str()? != id
            {
                return None;
            }
            let variable = alarm["sourceVariableId"].as_str()?;
            if v["variables"][variable]["id"].as_str()? != variable {
                return None;
            }
            let message = alarm["message"].as_str()?;
            let severity = alarm
                .get("severity")
                .map(Value::as_str)
                .unwrap_or(Some("WARNING"))?;
            if !matches!(severity, "INFO" | "WARNING" | "HIGH" | "CRITICAL") {
                return None;
            }
            Some(ReviewOnlyAlarm {
                source_path: p.clone(),
                id: id.clone(),
                source_variable_id: variable.into(),
                message: message.into(),
                severity: severity.into(),
            })
        })();
        if let Some(declaration) = parsed {
            for key in ["id", "sourceVariableId", "message", "severity"] {
                let leaf = path(&p, key);
                if m.findings.contains_key(&leaf) {
                    m.fail(&leaf, "legacy-alarm-activation-not-representable");
                }
            }
            out.push(declaration);
        }
    }
    out
}

fn map_project(v: &Value, m: &mut Mapping, project_id: &str) -> Option<Project> {
    let theme = resources::theme(v, m)?;
    let meta = v.get("metadata")?.as_object()?;
    let name = m.text(meta.get("name")?, "/metadata/name", "/metadata/name")?;
    let mut metadata = json!({"name":name});
    if let Some(d) = meta.get("description") {
        if d.is_null() {
            m.mark("/metadata/description", "/metadata/description");
        } else {
            metadata["description"] =
                json!(m.text(d, "/metadata/description", "/metadata/description")?);
        }
    }
    if let Some(t) = meta.get("targetProfiles") {
        m.empty(t, "/metadata/targetProfiles", "/deviceProfiles");
    }
    let screens = v.get("screens")?.as_object()?;
    if screens.is_empty() {
        return None;
    }
    let order = match v.get("screenOrder") {
        Some(Value::Array(a)) if !a.is_empty() => {
            if v.get("schemaVersion")?.as_u64()? < 5 {
                return None;
            }
            let o = a.iter().map(Value::as_str).collect::<Option<Vec<_>>>()?;
            if o.len() != screens.len()
                || o.iter().copied().collect::<BTreeSet<_>>().len() != o.len()
                || o.iter().any(|k| !screens.contains_key(*k))
            {
                return None;
            }
            o
        }
        Some(x) => {
            if screens.len() != 1 {
                return None;
            }
            m.empty(x, "/screenOrder", "/screens");
            screens.keys().map(String::as_str).collect()
        }
        None if screens.len() == 1 => screens.keys().map(String::as_str).collect(),
        None => return None,
    };
    let mut result = Vec::new();
    let mut screen_ids = BTreeMap::new();
    for (i, k) in order.iter().enumerate() {
        let p = path("/screens", k);
        let t = format!("/screens/{i}");
        let s = &screens[*k];
        if s.get("id")?.as_str() != Some(*k) {
            return None;
        }
        let id = m.id(&path(&p, "id"), k, "screen");
        m.mark(&path(&p, "id"), &path(&t, "id"));
        screen_ids.insert(*k, id.clone());
        if v.get("screenOrder")
            .is_some_and(|x| x.as_array().is_some_and(|a| !a.is_empty()))
        {
            m.mark(&format!("/screenOrder/{i}"), &path(&t, "id"));
        }
        let title = m.text(s.get("title")?, &path(&p, "title"), &path(&t, "name"))?;
        let w = dim(s.get("widthDp"), 1280.)?;
        let h = dim(s.get("heightDp"), 720.)?;
        m.mark(
            &path(&p, "widthDp"),
            &format!("{t}/root/layout/width/value"),
        );
        m.mark(
            &path(&p, "heightDp"),
            &format!("{t}/root/layout/height/value"),
        );
        for k in ["requiredPermission", "actions"] {
            if let Some(x) = s.get(k) {
                m.empty(x, &path(&p, k), &t);
            }
        }
        let root = m.id(
            &format!("{p}/@generated-root"),
            &format!("legacy-root-{i}"),
            "root",
        );
        let mut children = Vec::new();
        if let Some(c) = s.get("components") {
            let c = c.as_object()?;
            if c.is_empty() {
                m.mark(&path(&p, "components"), &format!("{t}/root/children"));
            }
            let mut ordered = c.iter().collect::<Vec<_>>();
            let mut z = BTreeSet::new();
            if s.get("layout").is_none_or(Value::is_null)
                && ordered
                    .iter()
                    .any(|(_, x)| !z.insert(x.get("zIndex").and_then(Value::as_i64).unwrap_or(0)))
            {
                return None;
            }
            ordered
                .sort_by_key(|(k, x)| (x.get("zIndex").and_then(Value::as_i64).unwrap_or(0), *k));
            for (j, (key, x)) in ordered.into_iter().enumerate() {
                children.push(graphics::component(
                    x,
                    key,
                    &path(&path(&p, "components"), key),
                    &format!("{t}/root/children/{j}"),
                    m,
                )?);
            }
        }
        result.push(json!({"id":id,"name":title,"title":title,"root":{"id":root,"element":{"type":"absolute"},"layout":{"width":fixed(w),"height":fixed(h)},"children":children}}));
    }
    // Explicit startup is required in this slice; no guessed startup behavior.
    let start = meta.get("defaultScreenId")?.as_str()?;
    let start = screen_ids.get(start)?;
    m.mark("/metadata/defaultScreenId", "/navigation/startScreen");
    let mut tags = Vec::new();
    if let Some(v) = v.get("variables") {
        let o = v.as_object()?;
        if o.is_empty() {
            m.mark("/variables", "/tags");
        }
        for (i, (k, x)) in o.iter().enumerate() {
            let p = path("/variables", k);
            let t = format!("/tags/{i}");
            if x.get("id")?.as_str() != Some(k) {
                return None;
            }
            let id = m.id(&path(&p, "id"), k, "tag");
            m.mark(&path(&p, "id"), &path(&t, "id"));
            let name = m.text(x.get("path")?, &path(&p, "path"), &path(&t, "name"))?;
            let ty = match x.get("type")?.as_str()? {
                "BOOL" => "bool",
                "INT" => "int32",
                "LONG" => "int64",
                "FLOAT" => "float32",
                "DOUBLE" => "float64",
                "STRING" => "string",
                _ => return None,
            };
            m.mark(&path(&p, "type"), &path(&t, "dataType"));
            if let Some(a) = x.get("access") {
                if !matches!(
                    a.as_str(),
                    Some("READ_ONLY" | "READ_WRITE" | "COMMAND" | "SETPOINT")
                ) {
                    return None;
                }
                m.mark(&path(&p, "access"), &path(&t, "access"));
            }
            let mut tag = json!({"id":id,"name":name,"dataType":ty,"access":"read"});
            if let Some(d) = x.get("description") {
                if d.is_null() {
                    m.mark(&path(&p, "description"), &path(&t, "description"));
                } else {
                    tag["description"] =
                        json!(m.text(d, &path(&p, "description"), &path(&t, "description"))?);
                }
            }
            if let Some(x) = x.get("defaultValue") {
                let src = path(&p, "defaultValue");
                let dest = path(&t, "initialValue");
                if x.is_null() {
                    m.mark(&src, &dest);
                } else {
                    let value = x.get("value")?;
                    let kind = x.get("valueType")?.as_str()?;
                    let mapped = match (ty, kind) {
                        ("bool", "bool") => json!(value.as_bool()?),
                        ("int32", "int") => json!(i32::try_from(value.as_i64()?).ok()?),
                        ("int64", "long") => json!(value.as_i64()?),
                        ("float32", "float") => json!(dim(Some(value), 0.)?),
                        ("float64", "double") => json!(value.as_f64()?),
                        ("string", "string") => json!(value.as_str()?),
                        _ => return None,
                    };
                    tag["initialValue"] = mapped;
                    m.mark(&path(&src, "valueType"), &path(&t, "dataType"));
                    m.mark(&path(&src, "value"), &dest);
                }
            }
            if x.get("simulationValue").is_some_and(Value::is_null) {
                m.mark(&path(&p, "simulationValue"), &t);
            }
            tags.push(tag);
        }
    }
    for (k, t) in [
        ("alarms", "/alarms"),
        ("assets", "/resources"),
        ("theme", "/themes"),
    ] {
        if let Some(x) = v.get(k) {
            m.empty(x, &path("", k), t);
        }
    }
    let mut doc = json!({"format":"hmi-studio.project","schemaVersion":2,"id":project_id,"metadata":metadata,"navigation":{"startScreen":start},"localization":{"defaultLocale":"en","locales":["en"]},"screens":result,"tags":tags,"extensions":{"hmiforge.idMap":m.ids}});
    if let Some(theme) = theme {
        doc["settings"]["defaultTheme"] = theme["id"].clone();
        doc["themes"] = json!([theme]);
    }
    if !m.styles.is_empty() {
        doc["styles"] = json!(m.styles);
    }
    industrial::map(v, &mut doc, m)?;
    resources::map(v, &mut doc, m)?;
    graphics::finish(v, &mut doc, m)?;
    navigation::map(v, &mut doc, m)?;
    if !m.contracts.is_empty() {
        doc["v2"]["components"]["components"] = json!(m.contracts);
    }
    doc["extensions"]["hmiforge.idMap"] = json!(m.ids);
    load_project(&doc.to_string()).ok().map(|x| x.project)
}
