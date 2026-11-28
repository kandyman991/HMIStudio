//! Deterministic compiler from frozen canonical schema v2 (`hmi-project-model`)
//! to legacy HMI Forge `HmiProject` JSON (schema versions 1..=10) for format-1
//! and format-2 `.hmipkg` runtime packages.
//!
//! ## Mandatory Invariants
//! - **Canonical validation first:** Every project must pass
//!   `hmi_project_model::validation::validate` with zero errors.
//! - **Quarantine isolation:** Quarantined legacy-import provenance is
//!   unconditionally refused in [`CompileMode::ProductionLive`], and refused by
//!   default even in [`CompileMode::DevelopmentSimulation`] unless explicitly
//!   opted in for isolated simulation.
//! - **B5-07 raster gate:** Every PNG/JPEG resource MUST pass full bounded pixel
//!   decoding via `hmi_legacy_import::raster::decode_bounded_raster_checked`
//!   before its bytes are Base64-encoded into `project.json`.
//! - **Fail-closed refusal:** Any canonical schema v2 construct without an exact
//!   legacy representation returns an explicit [`CompileError`] instead of
//!   silently dropping or approximating semantics.

use std::collections::{BTreeMap, BTreeSet};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use hmi_legacy_import::raster::{
    decode_bounded_raster_checked, RasterError, RasterFormat, RasterLimits,
};
use hmi_project_model::model::v2::{CommandKind, DriverKindV2, WriteLimits};
use hmi_project_model::model::v2_components::{
    CandidateAction, ComponentContract, DefinitionKind, EventOrigin, IndustrialSymbolKind,
    NewAction,
};
use hmi_project_model::model::{
    Action, Arrangement, BindingMode, BindingSource, ComponentKind, CrossAlignment, DataType,
    DriverKind, Element, EventKind, LocalizedText, Node, Project, Resource, ResourceKind, Screen,
    ScreenKind, Size, Tag, TagAccess, TagRef, Track, Visibility,
};
use hmi_project_model::validation::{validate, Severity};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

const RESERVED_QUARANTINE_EXTENSIONS: [&str; 3] = [
    "hmistudio.importQuarantine",
    "hmiforge.idMap",
    "hmiforge.standardCatalog",
];

/// Pinned HMI Forge `WriteRules.kt` / `WriteCommands.kt` momentary pulse duration bounds (ms).
const MIN_MOMENTARY_PULSE_MS: u32 = 100;
const MAX_MOMENTARY_PULSE_MS: u32 = 5_000;

/// Returns `true` iff the signed 64-bit integer `v` is exactly representable
/// in IEEE 754 binary64 (`f64` / Kotlin `Double`) with zero rounding error.
///
/// Uses bit-width inspection on `v.unsigned_abs()` rather than `(v as f64) as i64`,
/// avoiding Rust's saturating float-to-int cast trap for `i64::MAX` (`2^63 - 1`),
/// which rounds up to `2^63` in `f64` and saturates back to `i64::MAX`.
fn is_exact_binary64_i64(v: i64) -> bool {
    let mag = v.unsigned_abs();
    mag == 0 || (mag >> mag.trailing_zeros()) < (1u64 << 53)
}

/// Validate a canonical setpoint engineering limit (`min`, `max`, or `step`)
/// against Kotlin `WriteCommand`'s IEEE 754 binary64 (`Double`) target without
/// silently clamping, widening, or narrowing the value.
fn validate_setpoint_limit(
    num: &serde_json::Number,
    data_type: &DataType,
) -> Result<f64, CompileError> {
    match data_type {
        DataType::Int32 => {
            let v = num
                .as_i64()
                .and_then(|n| i32::try_from(n).ok())
                .ok_or(CompileError::UnsupportedFeature("inexactSetpointLimit"))?;
            Ok(f64::from(v))
        }
        DataType::Int64 => {
            let v = num
                .as_i64()
                .ok_or(CompileError::UnsupportedFeature("inexactSetpointLimit"))?;
            if !is_exact_binary64_i64(v) {
                return Err(CompileError::UnsupportedFeature("inexactSetpointLimit"));
            }
            Ok(v as f64)
        }
        DataType::Float32 => {
            let f = num
                .as_f64()
                .filter(|x| x.is_finite() && f64::from(*x as f32) == *x)
                .ok_or(CompileError::UnsupportedFeature("inexactSetpointLimit"))?;
            Ok(f)
        }
        DataType::Float64 => {
            let f = num
                .as_f64()
                .filter(|x| x.is_finite())
                .ok_or(CompileError::UnsupportedFeature("inexactSetpointLimit"))?;
            Ok(f)
        }
        _ => Err(CompileError::UnsupportedFeature("dataType")),
    }
}

/// Target execution posture for compilation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompileMode {
    /// Isolated development or simulation package (`SIMULATED`, no production approval per D-004).
    DevelopmentSimulation,
    /// Production live package (`LIVE`, refuses quarantined provenance and requires external mappings).
    ProductionLive,
}

/// Compiler options controlling mode, quarantine policy, and B5-07 raster bounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompileOptions {
    pub mode: CompileMode,
    /// Explicitly allow compiling a quarantined review project in `DevelopmentSimulation` only.
    /// Ignored (always refused) in `ProductionLive`.
    pub allow_quarantined_simulation: bool,
    /// B5-07 bounded pixel decoding limits applied to all PNG/JPEG resources.
    pub raster_limits: RasterLimits,
}

impl Default for CompileOptions {
    fn default() -> Self {
        Self {
            mode: CompileMode::DevelopmentSimulation,
            allow_quarantined_simulation: false,
            raster_limits: RasterLimits::DEFAULT,
        }
    }
}

/// Output of compiling a canonical schema v2 [`Project`] to legacy `HmiProject` JSON.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledProject {
    mode: CompileMode,
    project_json: Vec<u8>,
    schema_version: u32,
    project_name: String,
    startup_screen: String,
    has_live_mapping: bool,
}

impl CompiledProject {
    pub(crate) fn mode(&self) -> CompileMode {
        self.mode
    }

    pub fn project_json(&self) -> &[u8] {
        &self.project_json
    }

    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }

    pub fn project_name(&self) -> &str {
        &self.project_name
    }

    pub fn startup_screen(&self) -> &str {
        &self.startup_screen
    }

    pub fn has_live_mapping(&self) -> bool {
        self.has_live_mapping
    }
}

/// Explicit, payload-free compiler refusal reasons.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CompileError {
    #[error("canonical project failed model validation")]
    InvalidProject,
    #[error("quarantined legacy-import project is refused in this compile mode")]
    QuarantinedProjectRefused,
    #[error("production live compilation requires at least one valid external source mapping")]
    LiveMappingRequired,
    #[error("canonical feature is unsupported by format-2 legacy runtime: {0}")]
    UnsupportedFeature(&'static str),
    #[error("missing resource bytes for declared project resource")]
    MissingResource,
    #[error("declared resource integrity (SHA-256) check failed")]
    ResourceIntegrityMismatch,
    #[error("B5-07 raster consumer gate refused resource ({0:?}): {0}")]
    RasterRefused(#[from] RasterError),
    #[error("SVG resource failed restricted static SVG validation")]
    InvalidSvgResource,
    #[error("legacy project JSON serialization failed or exceeded size limit")]
    SerializationFailed,
}

fn has_quarantine_provenance(project: &Project) -> bool {
    project.id.as_str().starts_with("legacy-project-")
        || project
            .extensions
            .keys()
            .any(|k| RESERVED_QUARANTINE_EXTENSIONS.contains(&k.as_str()))
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn literal_text(text: &LocalizedText) -> Result<&str, CompileError> {
    match text {
        LocalizedText::Literal(s) => Ok(s.as_str()),
        LocalizedText::Key(_) => Err(CompileError::UnsupportedFeature("localizedTextKey")),
    }
}

fn is_hex_color(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

// Strict static SVG check matching the legacy-import restricted SVG grammar.
fn verify_restricted_svg(bytes: &[u8]) -> Result<(u32, u32), CompileError> {
    let xml = std::str::from_utf8(bytes)
        .map_err(|_| CompileError::InvalidSvgResource)?
        .trim();
    if !xml.is_ascii() || xml.len() > 256 * 1024 {
        return Err(CompileError::InvalidSvgResource);
    }
    let root = xml
        .strip_prefix("<svg")
        .ok_or(CompileError::InvalidSvgResource)?;
    let (header, body) = root
        .split_once('>')
        .ok_or(CompileError::InvalidSvgResource)?;
    let mut attrs = parse_svg_attrs(header).ok_or(CompileError::InvalidSvgResource)?;
    if attrs.remove("xmlns") != Some("http://www.w3.org/2000/svg") {
        return Err(CompileError::InvalidSvgResource);
    }
    let w = parse_svg_u32(
        attrs
            .remove("width")
            .ok_or(CompileError::InvalidSvgResource)?,
    )
    .ok_or(CompileError::InvalidSvgResource)?;
    let h = parse_svg_u32(
        attrs
            .remove("height")
            .ok_or(CompileError::InvalidSvgResource)?,
    )
    .ok_or(CompileError::InvalidSvgResource)?;
    if w == 0 || h == 0 || u64::from(w) * u64::from(h) > 16_000_000 {
        return Err(CompileError::InvalidSvgResource);
    }
    if let Some(vb) = attrs.remove("viewBox") {
        if vb != format!("0 0 {w} {h}") {
            return Err(CompileError::InvalidSvgResource);
        }
    }
    if !attrs.is_empty() {
        return Err(CompileError::InvalidSvgResource);
    }
    let mut body = body
        .strip_suffix("</svg>")
        .ok_or(CompileError::InvalidSvgResource)?
        .trim();
    let mut count = 0usize;
    while !body.is_empty() {
        let (element, tail) = body
            .split_once("/>")
            .ok_or(CompileError::InvalidSvgResource)?;
        let mut a = parse_svg_attrs(
            element
                .strip_prefix("<rect")
                .ok_or(CompileError::InvalidSvgResource)?,
        )
        .ok_or(CompileError::InvalidSvgResource)?;
        let x = parse_svg_u32(a.remove("x").ok_or(CompileError::InvalidSvgResource)?)
            .ok_or(CompileError::InvalidSvgResource)?;
        let y = parse_svg_u32(a.remove("y").ok_or(CompileError::InvalidSvgResource)?)
            .ok_or(CompileError::InvalidSvgResource)?;
        let rw = parse_svg_u32(a.remove("width").ok_or(CompileError::InvalidSvgResource)?)
            .ok_or(CompileError::InvalidSvgResource)?;
        let rh = parse_svg_u32(a.remove("height").ok_or(CompileError::InvalidSvgResource)?)
            .ok_or(CompileError::InvalidSvgResource)?;
        let fill = a.remove("fill").ok_or(CompileError::InvalidSvgResource)?;
        if rw == 0 || rh == 0 || x + rw > w || y + rh > h || !is_hex_color(fill) || !a.is_empty() {
            return Err(CompileError::InvalidSvgResource);
        }
        count += 1;
        if count > 1024 {
            return Err(CompileError::InvalidSvgResource);
        }
        body = tail.trim();
    }
    if count == 0 {
        return Err(CompileError::InvalidSvgResource);
    }
    Ok((w, h))
}

fn parse_svg_attrs(mut s: &str) -> Option<BTreeMap<&str, &str>> {
    let mut attrs = BTreeMap::new();
    while !s.is_empty() {
        if !s.starts_with(' ') {
            return None;
        }
        s = s.trim_start_matches(' ');
        if s.is_empty() {
            break;
        }
        let (key, tail) = s.split_once("=\"")?;
        if key.is_empty() || !key.bytes().all(|b| b.is_ascii_alphabetic()) {
            return None;
        }
        let (val, tail) = tail.split_once('"')?;
        if val.contains(['<', '>', '&']) || attrs.insert(key, val).is_some() {
            return None;
        }
        s = tail;
    }
    Some(attrs)
}

fn parse_svg_u32(s: &str) -> Option<u32> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n = s.parse::<u32>().ok()?;
    (n <= 4096).then_some(n)
}

fn legacy_data_type(ty: &DataType) -> Result<(&'static str, &'static str), CompileError> {
    match ty {
        DataType::Bool => Ok(("BOOL", "bool")),
        DataType::Int32 => Ok(("INT", "int")),
        DataType::Int64 => Ok(("LONG", "long")),
        DataType::Float32 => Ok(("FLOAT", "float")),
        DataType::Float64 => Ok(("DOUBLE", "double")),
        DataType::String => Ok(("STRING", "string")),
        DataType::Int8
        | DataType::Int16
        | DataType::UInt8
        | DataType::UInt16
        | DataType::UInt32
        | DataType::UInt64
        | DataType::DateTime
        | DataType::Array { .. }
        | DataType::Struct(_) => Err(CompileError::UnsupportedFeature("dataType")),
    }
}

fn compile_resource(
    resource: &Resource,
    resource_bytes: &BTreeMap<String, Vec<u8>>,
    raster_limits: &RasterLimits,
) -> Result<(String, Value), CompileError> {
    let bytes = resource_bytes
        .get(&resource.path)
        .ok_or(CompileError::MissingResource)?;
    let actual_sha = sha256_hex(bytes);
    if let Some(expected_sha) = &resource.sha256 {
        if *expected_sha != actual_sha {
            return Err(CompileError::ResourceIntegrityMismatch);
        }
    }
    let media = resource
        .media_type
        .as_deref()
        .ok_or(CompileError::UnsupportedFeature("resourceMediaType"))?;
    let expected_dims = match (
        resource.extensions.get("legacyWidthPx"),
        resource.extensions.get("legacyHeightPx"),
    ) {
        (None, None) => None,
        (Some(w), Some(h)) => {
            let w = w
                .as_u64()
                .and_then(|v| u32::try_from(v).ok())
                .filter(|&v| v > 0)
                .ok_or(CompileError::RasterRefused(RasterError::DimensionMismatch))?;
            let h = h
                .as_u64()
                .and_then(|v| u32::try_from(v).ok())
                .filter(|&v| v > 0)
                .ok_or(CompileError::RasterRefused(RasterError::DimensionMismatch))?;
            Some((w, h))
        }
        _ => return Err(CompileError::RasterRefused(RasterError::DimensionMismatch)),
    };

    let (kind, w, h, data_str) = match (resource.kind, media) {
        (ResourceKind::Image, "image/png" | "image/jpeg") => {
            // B5-07 mandatory consumer gate: full bounded pixel decoding before packaging.
            let decoded =
                decode_bounded_raster_checked(media, bytes, expected_dims, raster_limits)?;
            let kind_str = match decoded.format() {
                RasterFormat::Png => "PNG",
                RasterFormat::Jpeg => "JPEG",
            };
            (
                kind_str,
                decoded.width(),
                decoded.height(),
                STANDARD.encode(bytes),
            )
        }
        (ResourceKind::Vector, "image/svg+xml") => {
            let (w, h) = verify_restricted_svg(bytes)?;
            if let Some((ew, eh)) = expected_dims {
                if (w, h) != (ew, eh) {
                    return Err(CompileError::InvalidSvgResource);
                }
            }
            let text = std::str::from_utf8(bytes)
                .map_err(|_| CompileError::InvalidSvgResource)?
                .to_owned();
            ("SVG", w, h, text)
        }
        _ => return Err(CompileError::UnsupportedFeature("resourceKind")),
    };

    let name = resource
        .extensions
        .get("legacyName")
        .and_then(Value::as_str)
        .unwrap_or(resource.id.as_str());

    let asset = json!({
        "id": resource.id.as_str(),
        "name": name,
        "kind": kind,
        "sha256": actual_sha,
        "sizeBytes": bytes.len(),
        "widthPx": w,
        "heightPx": h,
        "data": data_str,
    });
    Ok((resource.id.as_str().to_owned(), asset))
}

fn compile_default_value(ty: &DataType, val: &Value) -> Result<Value, CompileError> {
    let (_, value_type) = legacy_data_type(ty)?;
    let valid = match ty {
        DataType::Bool => val.is_boolean(),
        DataType::Int32 => val.as_i64().is_some_and(|n| i32::try_from(n).is_ok()),
        DataType::Int64 => val.is_i64(),
        DataType::Float32 => val.as_f64().is_some_and(f64::is_finite),
        DataType::Float64 => val.as_f64().is_some_and(f64::is_finite),
        DataType::String => val.is_string(),
        _ => false,
    };
    if !valid {
        return Err(CompileError::InvalidProject);
    }
    Ok(json!({
        "valueType": value_type,
        "value": val,
    }))
}

fn compile_action(action: &Action, tag_map: &BTreeMap<&str, &Tag>) -> Result<Value, CompileError> {
    match action {
        Action::Navigate { screen } => Ok(json!({
            "action": "navigate.screen",
            "screenId": screen.as_str(),
        })),
        Action::Back {} => Ok(json!({ "action": "navigate.back" })),
        Action::ClosePopup {} => Ok(json!({ "action": "popup.close" })),
        Action::WriteTag {
            target,
            member,
            value,
        } => {
            if member.is_some() {
                return Err(CompileError::UnsupportedFeature("memberPath"));
            }
            let tag_id = match target {
                TagRef::Tag(id) => id.as_str(),
                TagRef::TagParameter(_) => {
                    return Err(CompileError::UnsupportedFeature("writableTagParameter"));
                }
            };
            let tag = tag_map.get(tag_id).ok_or(CompileError::InvalidProject)?;
            let typed = compile_default_value(&tag.data_type, value)?;
            Ok(json!({
                "action": "value.set",
                "variableId": tag_id,
                "value": typed,
            }))
        }
        Action::ToggleTag { target, member } => {
            if member.is_some() {
                return Err(CompileError::UnsupportedFeature("memberPath"));
            }
            let tag_id = match target {
                TagRef::Tag(id) => id.as_str(),
                TagRef::TagParameter(_) => {
                    return Err(CompileError::UnsupportedFeature("writableTagParameter"));
                }
            };
            Ok(json!({
                "action": "value.toggle",
                "variableId": tag_id,
            }))
        }
        Action::StepTag {
            target,
            member,
            step,
            min,
            max,
        } => {
            if member.is_some() {
                return Err(CompileError::UnsupportedFeature("memberPath"));
            }
            let tag_id = match target {
                TagRef::Tag(id) => id.as_str(),
                TagRef::TagParameter(_) => {
                    return Err(CompileError::UnsupportedFeature("writableTagParameter"));
                }
            };
            let mut obj = Map::new();
            obj.insert("action".into(), json!("value.step"));
            obj.insert("variableId".into(), json!(tag_id));
            obj.insert("step".into(), json!(step));
            if let Some(m) = min {
                obj.insert("min".into(), json!(m));
            }
            if let Some(m) = max {
                obj.insert("max".into(), json!(m));
            }
            Ok(Value::Object(obj))
        }
        Action::PulseTag {
            target,
            member,
            duration_ms,
        } => {
            if member.is_some() {
                return Err(CompileError::UnsupportedFeature("memberPath"));
            }
            let tag_id = match target {
                TagRef::Tag(id) => id.as_str(),
                TagRef::TagParameter(_) => {
                    return Err(CompileError::UnsupportedFeature("writableTagParameter"));
                }
            };
            Ok(json!({
                "action": "value.pulse",
                "variableId": tag_id,
                "durationMs": duration_ms,
            }))
        }
        Action::OpenPopup { .. } => Err(CompileError::UnsupportedFeature("openPopupAction")),
        Action::AcknowledgeAlarms { .. } => {
            Err(CompileError::UnsupportedFeature("acknowledgeAlarmsAction"))
        }
        Action::LoadRecipe { .. } | Action::SaveRecipe { .. } => {
            Err(CompileError::UnsupportedFeature("recipeAction"))
        }
        Action::SetLocale { .. } => Err(CompileError::UnsupportedFeature("setLocaleAction")),
    }
}

fn compile_candidate_action(
    action: &CandidateAction,
    tag_map: &BTreeMap<&str, &Tag>,
) -> Result<Value, CompileError> {
    match action {
        CandidateAction::Existing(a) => compile_action(a, tag_map),
        CandidateAction::New(NewAction::NavigateStartup {}) => {
            Ok(json!({ "action": "navigate.startup" }))
        }
        CandidateAction::New(NewAction::OpenFaceplate {
            definition,
            parameters,
        }) => {
            let mut obj = Map::new();
            obj.insert("action".into(), json!("open.faceplate"));
            obj.insert("definitionId".into(), json!(definition.as_str()));
            if !parameters.is_empty() {
                obj.insert("parameters".into(), json!(parameters));
            }
            Ok(Value::Object(obj))
        }
    }
}

fn legacy_event_name(event: EventKind) -> Result<&'static str, CompileError> {
    match event {
        EventKind::Press | EventKind::Click => Ok("press"),
        EventKind::Release => Ok("release"),
        EventKind::LongPress | EventKind::ValueChanged | EventKind::Show | EventKind::Hide => {
            Err(CompileError::UnsupportedFeature("eventKind"))
        }
    }
}

struct CompileContext<'a> {
    tags: BTreeMap<&'a str, &'a Tag>,
    contracts: BTreeMap<&'a str, &'a ComponentContract>,
    expressions: BTreeMap<&'a str, Vec<(&'a str, &'a str)>>,
    node_permissions: BTreeMap<&'a str, &'a str>,
    candidate_handlers:
        BTreeMap<&'a str, Vec<&'a hmi_project_model::model::v2_components::CandidateHandler>>,
    symbol_defs: BTreeMap<&'a str, IndustrialSymbolKind>,
    resource_media: BTreeMap<&'a str, &'a str>,
    min_version: u32,
}

impl<'a> CompileContext<'a> {
    fn bump(&mut self, v: u32) {
        if v > self.min_version {
            self.min_version = v;
        }
    }

    fn check_common_node_constraints(&self, node: &Node) -> Result<(), CompileError> {
        if !node.responsive.is_empty() {
            return Err(CompileError::UnsupportedFeature("responsiveLayout"));
        }
        if node.layout.z_index != 0 {
            return Err(CompileError::UnsupportedFeature("zIndex"));
        }
        if node.layout.aspect_ratio.is_some() {
            return Err(CompileError::UnsupportedFeature("aspectRatio"));
        }
        if node.layout.clip {
            return Err(CompileError::UnsupportedFeature("clip"));
        }
        if node.layout.align.is_some() {
            return Err(CompileError::UnsupportedFeature("boxAlignment"));
        }
        if node.layout.visibility == Visibility::Collapsed {
            return Err(CompileError::UnsupportedFeature("collapsedVisibility"));
        }
        Ok(())
    }

    fn compile_leaf_component(
        &mut self,
        node: &Node,
        in_adaptive: bool,
    ) -> Result<(String, Value), CompileError> {
        self.check_common_node_constraints(node)?;
        if in_adaptive && node.layout.visibility != Visibility::Visible {
            return Err(CompileError::UnsupportedFeature("hiddenAdaptiveComponent"));
        }

        let (x, y) = match &node.layout.position {
            Some(pos) => (pos.x, pos.y),
            None => (0.0, 0.0),
        };
        let width = match node.layout.width {
            Size::Fixed { value } => value,
            Size::Auto {} | Size::Fill { .. } => 120.0,
        };
        let height = match node.layout.height {
            Size::Fixed { value } => value,
            Size::Auto {} | Size::Fill { .. } => 48.0,
        };

        let mut comp = Map::new();
        comp.insert("id".into(), json!(node.id.as_str()));

        let mut props = Map::new();
        let mut bindings = Map::new();

        match &node.element {
            Element::Component { kind } => {
                let legacy_type = match kind {
                    ComponentKind::Text => "label",
                    ComponentKind::Button => "button",
                    ComponentKind::Rectangle => "rectangle",
                    ComponentKind::Ellipse => "ellipse",
                    ComponentKind::NumericDisplay => "numericDisplay",
                    ComponentKind::NumericInput => {
                        self.bump(10);
                        "numericInput"
                    }
                    ComponentKind::Indicator => "indicator",
                    ComponentKind::Gauge => "gauge",
                    ComponentKind::Slider => {
                        self.bump(10);
                        "slider"
                    }
                    ComponentKind::Switch => {
                        self.bump(10);
                        "switch"
                    }
                    ComponentKind::ProgressBar => "progressBar",
                    ComponentKind::Image => {
                        let asset_id = node
                            .properties
                            .get("source")
                            .or_else(|| node.properties.get("assetId"))
                            .and_then(Value::as_str);
                        if asset_id.and_then(|id| self.resource_media.get(id)).copied()
                            == Some("image/svg+xml")
                        {
                            "svgImage"
                        } else {
                            "image"
                        }
                    }
                    ComponentKind::Line => "line",
                    ComponentKind::Arrow => "arrow",
                    ComponentKind::Polygon => "polygon",
                    ComponentKind::DropDown => {
                        self.bump(10);
                        "dropDown"
                    }
                    ComponentKind::TextInput => {
                        self.bump(10);
                        "textInput"
                    }
                    ComponentKind::Container => {
                        return Err(CompileError::UnsupportedFeature("containerComponent"));
                    }
                };
                comp.insert("type".into(), json!(legacy_type));

                if let Some(contract) = self.contracts.get(node.id.as_str()) {
                    if in_adaptive
                        && matches!(
                            contract,
                            ComponentContract::Line { .. }
                                | ComponentContract::Arrow { .. }
                                | ComponentContract::Polygon { .. }
                        )
                    {
                        return Err(CompileError::UnsupportedFeature("adaptiveGraphicPrimitive"));
                    }
                    self.apply_v2_contract(contract, width, height, &mut props, &mut bindings)?;
                } else if kind.requires_v2_contract() {
                    return Err(CompileError::InvalidProject);
                }

                for (k, v) in &node.properties {
                    if k == "source" && *kind == ComponentKind::Image {
                        props.insert("assetId".into(), v.clone());
                        continue;
                    }
                    if let Some(obj) = v.as_object() {
                        if obj.contains_key("textKey") {
                            return Err(CompileError::UnsupportedFeature("localizedTextKey"));
                        }
                        if obj.contains_key("token") {
                            return Err(CompileError::UnsupportedFeature("themeColorToken"));
                        }
                    }
                    props.insert(k.clone(), v.clone());
                }
            }
            Element::Instance {
                definition,
                parameters,
            } => {
                if let Some(symbol_kind) = self.symbol_defs.get(definition.as_str()).copied() {
                    let sym_type = match symbol_kind {
                        IndustrialSymbolKind::Motor => "motor",
                        IndustrialSymbolKind::Valve => "valve",
                        IndustrialSymbolKind::Conveyor => "conveyor",
                        IndustrialSymbolKind::Pump => "pump",
                        IndustrialSymbolKind::Tank => "tank",
                        IndustrialSymbolKind::Pipe => "pipe",
                        IndustrialSymbolKind::ThermalIndicator => "thermalIndicator",
                        IndustrialSymbolKind::Sensor | IndustrialSymbolKind::FlowIndicator => {
                            return Err(CompileError::UnsupportedFeature("industrialSymbolKind"));
                        }
                    };
                    comp.insert("type".into(), json!(sym_type));
                    for (slot, val) in parameters {
                        let tag_id = val.as_str().ok_or(CompileError::InvalidProject)?;
                        bindings.insert(slot.clone(), json!(tag_id));
                    }
                } else {
                    self.bump(6);
                    comp.insert("type".into(), json!(definition.as_str()));
                    if !parameters.is_empty() {
                        for (k, v) in parameters {
                            props.insert(k.clone(), v.clone());
                        }
                    }
                }
            }
            _ => return Err(CompileError::UnsupportedFeature("nestedContainerAsLeaf")),
        }

        for b in &node.bindings {
            if b.member.is_some() {
                return Err(CompileError::UnsupportedFeature("bindingMemberPath"));
            }
            if b.transform.is_some() {
                return Err(CompileError::UnsupportedFeature("bindingTransform"));
            }
            if b.mode == BindingMode::Write {
                return Err(CompileError::UnsupportedFeature("writeOnlyBinding"));
            }
            let src = match &b.source {
                BindingSource::Tag(id) => id.as_str(),
                BindingSource::TagParameter(name) => name.as_str(),
                BindingSource::Parameter(_) => {
                    return Err(CompileError::UnsupportedFeature("valueParameterBinding"));
                }
            };
            bindings.insert(b.property.clone(), json!(src));
        }

        if let Some(name) = &node.name {
            comp.insert("name".into(), json!(name));
        }
        if !in_adaptive {
            comp.insert(
                "geometry".into(),
                json!({ "x": x, "y": y, "width": width, "height": height }),
            );
            if node.layout.visibility == Visibility::Hidden {
                comp.insert("visible".into(), json!(false));
            }
        }
        if !props.is_empty() {
            comp.insert("properties".into(), Value::Object(props));
        }
        if !bindings.is_empty() {
            comp.insert("bindings".into(), Value::Object(bindings));
        }

        if let Some(exprs) = self.expressions.get(node.id.as_str()).cloned() {
            self.bump(4);
            let mut map = Map::new();
            for (prop, expr) in exprs {
                map.insert(prop.to_owned(), json!(expr));
            }
            comp.insert("expressions".into(), Value::Object(map));
        }

        // Compile events (both node.events and v2.components.handlers).
        let mut actions_by_event: BTreeMap<&'static str, Vec<Value>> = BTreeMap::new();
        for handler in &node.events {
            if handler.confirm.is_some() {
                return Err(CompileError::UnsupportedFeature("eventConfirmationPrompt"));
            }
            let ev = legacy_event_name(handler.event)?;
            for act in &handler.actions {
                actions_by_event
                    .entry(ev)
                    .or_default()
                    .push(compile_action(act, &self.tags)?);
            }
        }
        if let Some(v2_handlers) = self.candidate_handlers.get(node.id.as_str()).cloned() {
            for handler in v2_handlers {
                if handler.confirm.is_some() {
                    return Err(CompileError::UnsupportedFeature("eventConfirmationPrompt"));
                }
                let ev = legacy_event_name(handler.event)?;
                for act in &handler.actions {
                    actions_by_event
                        .entry(ev)
                        .or_default()
                        .push(compile_candidate_action(act, &self.tags)?);
                }
            }
        }
        if !actions_by_event.is_empty() {
            let mut action_obj = Map::new();
            for (ev, list) in actions_by_event {
                if ev != "press" || list.len() != 1 {
                    self.bump(10);
                    action_obj.insert(ev.to_owned(), Value::Array(list));
                } else {
                    let single = &list[0];
                    match single.get("action").and_then(Value::as_str) {
                        Some("navigate.screen" | "navigate.back" | "navigate.startup") => {
                            self.bump(5);
                        }
                        Some("open.faceplate") => {
                            if single.get("parameters").is_some() {
                                self.bump(10);
                            } else {
                                self.bump(6);
                            }
                        }
                        _ => self.bump(10),
                    }
                    action_obj.insert(ev.to_owned(), single.clone());
                }
            }
            comp.insert("actions".into(), Value::Object(action_obj));
        }

        if let Some(perm) = self.node_permissions.get(node.id.as_str()).copied() {
            self.bump(8);
            comp.insert("requiredPermission".into(), json!(perm));
        }

        Ok((node.id.as_str().to_owned(), Value::Object(comp)))
    }

    fn apply_v2_contract(
        &mut self,
        contract: &ComponentContract,
        width: f64,
        height: f64,
        props: &mut Map<String, Value>,
        bindings: &mut Map<String, Value>,
    ) -> Result<(), CompileError> {
        let to_pct = |coord: f64, span: f64, sw: f64| -> Result<f64, CompileError> {
            let inset = sw / 2.0;
            let usable = span - 2.0 * inset;
            if usable <= 0.0 {
                return Err(CompileError::InvalidProject);
            }
            let pct = ((coord - inset) / usable) * 100.0;
            if !(-0.01..=100.01).contains(&pct) {
                return Err(CompileError::UnsupportedFeature("primitiveOutOfBounds"));
            }
            Ok(pct.clamp(0.0, 100.0))
        };
        match contract {
            ComponentContract::Line {
                start,
                end,
                stroke_color,
                stroke_width,
            } => {
                let p0x = to_pct(start.x, width, *stroke_width)?;
                let p0y = to_pct(start.y, height, *stroke_width)?;
                let p1x = to_pct(end.x, width, *stroke_width)?;
                let p1y = to_pct(end.y, height, *stroke_width)?;
                props.insert("strokeColor".into(), json!(stroke_color));
                props.insert("strokeWidth".into(), json!(format!("{stroke_width}")));
                props.insert("strokeStyle".into(), json!("SOLID"));
                props.insert(
                    "points".into(),
                    json!(format!("{p0x:.2},{p0y:.2} {p1x:.2},{p1y:.2}")),
                );
            }
            ComponentContract::Polygon {
                points,
                fill,
                stroke_color,
                stroke_width,
            } => {
                let mut parts = Vec::with_capacity(points.len());
                for pt in points {
                    let px = to_pct(pt.x, width, *stroke_width)?;
                    let py = to_pct(pt.y, height, *stroke_width)?;
                    parts.push(format!("{px:.2},{py:.2}"));
                }
                props.insert("fillColor".into(), json!(fill));
                props.insert("strokeColor".into(), json!(stroke_color));
                props.insert("strokeWidth".into(), json!(format!("{stroke_width}")));
                props.insert("strokeStyle".into(), json!("SOLID"));
                props.insert("points".into(), json!(parts.join(" ")));
            }
            ComponentContract::Arrow {
                start,
                end,
                stroke_color,
                stroke_width,
                ..
            } => {
                let dx = end.x - start.x;
                let dy = end.y - start.y;
                let dir = if dx.abs() > dy.abs() {
                    if dx >= 0.0 {
                        "RIGHT"
                    } else {
                        "LEFT"
                    }
                } else if dy >= 0.0 {
                    "DOWN"
                } else {
                    "UP"
                };
                props.insert("direction".into(), json!(dir));
                props.insert("color".into(), json!(stroke_color));
                props.insert("strokeWidth".into(), json!(format!("{stroke_width}")));
            }
            ComponentContract::DropDown {
                tag,
                enumeration,
                label,
            } => {
                self.bump(10);
                props.insert("enumeration".into(), json!(enumeration.as_str()));
                if let Some(lbl) = label {
                    props.insert("label".into(), json!(literal_text(lbl)?));
                }
                bindings.insert("value".into(), json!(tag.as_str()));
            }
            ComponentContract::TextInput {
                tag,
                max_length,
                label,
            } => {
                self.bump(10);
                props.insert("maxLength".into(), json!(max_length));
                if let Some(lbl) = label {
                    props.insert("label".into(), json!(literal_text(lbl)?));
                }
                bindings.insert("value".into(), json!(tag.as_str()));
            }
        }
        Ok(())
    }

    fn compile_adaptive_node(
        &mut self,
        node: &Node,
        components: &mut Map<String, Value>,
        depth: usize,
    ) -> Result<Value, CompileError> {
        self.check_common_node_constraints(node)?;
        if depth > 32 {
            return Err(CompileError::UnsupportedFeature("layoutDepthLimit"));
        }
        let pad = node.layout.padding.top;
        if node.layout.padding.end != pad
            || node.layout.padding.bottom != pad
            || node.layout.padding.start != pad
        {
            return Err(CompileError::UnsupportedFeature("asymmetricPadding"));
        }
        match &node.element {
            Element::Row {
                arrangement,
                alignment,
                spacing,
                wrap,
            } => {
                if *wrap {
                    return Err(CompileError::UnsupportedFeature("wrappedRowLayout"));
                }
                if *arrangement != Arrangement::Start || *alignment != CrossAlignment::Start {
                    return Err(CompileError::UnsupportedFeature("nonDefaultFlowAlignment"));
                }
                self.bump(2);
                let mut children = Vec::with_capacity(node.children.len());
                for ch in &node.children {
                    children.push(self.compile_adaptive_node(ch, components, depth + 1)?);
                }
                Ok(json!({
                    "node": "stack",
                    "id": node.id.as_str(),
                    "direction": "ROW",
                    "gapDp": spacing,
                    "paddingDp": pad,
                    "children": children,
                }))
            }
            Element::Column {
                arrangement,
                alignment,
                spacing,
                wrap,
            } => {
                if *wrap {
                    return Err(CompileError::UnsupportedFeature("wrappedColumnLayout"));
                }
                if *arrangement != Arrangement::Start || *alignment != CrossAlignment::Start {
                    return Err(CompileError::UnsupportedFeature("nonDefaultFlowAlignment"));
                }
                self.bump(2);
                let mut children = Vec::with_capacity(node.children.len());
                for ch in &node.children {
                    children.push(self.compile_adaptive_node(ch, components, depth + 1)?);
                }
                Ok(json!({
                    "node": "stack",
                    "id": node.id.as_str(),
                    "direction": "COLUMN",
                    "gapDp": spacing,
                    "paddingDp": pad,
                    "children": children,
                }))
            }
            Element::Grid {
                columns,
                rows,
                column_gap,
                row_gap,
            } => {
                if depth > 0 {
                    return Err(CompileError::UnsupportedFeature("nestedGridLayout"));
                }
                if (column_gap - row_gap).abs() > f64::EPSILON {
                    return Err(CompileError::UnsupportedFeature("asymmetricGridGaps"));
                }
                if columns.is_empty()
                    || rows.is_empty()
                    || !columns
                        .iter()
                        .all(|t| matches!(t, Track::Fraction { weight } if (*weight - 1.0).abs() <= f64::EPSILON))
                    || !rows.iter().all(|t| matches!(t, Track::Auto {}))
                {
                    return Err(CompileError::UnsupportedFeature("nonUniformGridTracks"));
                }
                self.bump(3);
                let mut children = Vec::with_capacity(node.children.len());
                for ch in &node.children {
                    if let Some(cell) = &ch.layout.grid_cell {
                        if cell.column_span != 1 || cell.row_span != 1 {
                            return Err(CompileError::UnsupportedFeature("gridCellSpan"));
                        }
                    }
                    children.push(self.compile_adaptive_node(ch, components, depth + 1)?);
                }
                Ok(json!({
                    "node": "stack",
                    "id": node.id.as_str(),
                    "direction": "GRID",
                    "columns": columns.len(),
                    "gapDp": column_gap,
                    "paddingDp": pad,
                    "children": children,
                }))
            }
            Element::Component { .. } | Element::Instance { .. } => {
                let (id, comp) = self.compile_leaf_component(node, true)?;
                components.insert(id.clone(), comp);
                let mut slot = Map::new();
                slot.insert("node".into(), json!("component"));
                slot.insert("componentId".into(), json!(id));
                if let Some(w) = node.layout.weight {
                    slot.insert("weight".into(), json!(w));
                }
                if let Some(mh) = node.layout.max_height {
                    self.bump(3);
                    slot.insert("maxHeightDp".into(), json!(mh));
                }
                Ok(Value::Object(slot))
            }
            Element::Box { .. } => Err(CompileError::UnsupportedFeature("boxLayout")),
            Element::Scroll { .. } => Err(CompileError::UnsupportedFeature("scrollLayout")),
            Element::Absolute {} => Err(CompileError::UnsupportedFeature("nestedAbsoluteLayout")),
        }
    }

    fn compile_screen(
        &mut self,
        screen: &Screen,
        screen_perms: &BTreeMap<&str, &str>,
    ) -> Result<(String, Value), CompileError> {
        if screen.kind == ScreenKind::Popup {
            return Err(CompileError::UnsupportedFeature("popupScreen"));
        }
        if !screen.events.is_empty() {
            return Err(CompileError::UnsupportedFeature("screenLifecycleEvents"));
        }
        let title = match &screen.title {
            Some(t) => literal_text(t)?,
            None => screen.name.as_str(),
        };
        let width_dp = match screen.root.layout.width {
            Size::Fixed { value } if value > 0.0 => value,
            _ => 1280.0,
        };
        let height_dp = match screen.root.layout.height {
            Size::Fixed { value } if value > 0.0 => value,
            _ => 720.0,
        };

        let mut components = Map::new();
        let mut layout_val = None;

        match &screen.root.element {
            Element::Absolute {} => {
                self.check_common_node_constraints(&screen.root)?;
                for child in &screen.root.children {
                    let (id, comp) = self.compile_leaf_component(child, false)?;
                    components.insert(id, comp);
                }
            }
            Element::Row { .. } | Element::Column { .. } | Element::Grid { .. } => {
                let root_layout = self.compile_adaptive_node(&screen.root, &mut components, 0)?;
                layout_val = Some(json!({ "root": root_layout }));
            }
            Element::Box { .. } => return Err(CompileError::UnsupportedFeature("boxLayout")),
            Element::Scroll { .. } => return Err(CompileError::UnsupportedFeature("scrollLayout")),
            Element::Component { .. } | Element::Instance { .. } => {
                return Err(CompileError::UnsupportedFeature("nonContainerScreenRoot"));
            }
        }

        let mut obj = Map::new();
        obj.insert("id".into(), json!(screen.id.as_str()));
        obj.insert("title".into(), json!(title));
        obj.insert("widthDp".into(), json!(width_dp));
        obj.insert("heightDp".into(), json!(height_dp));
        obj.insert("components".into(), Value::Object(components));
        if let Some(layout) = layout_val {
            obj.insert("layout".into(), layout);
        }
        if let Some(perm) = screen_perms.get(screen.id.as_str()) {
            self.bump(8);
            obj.insert("requiredPermission".into(), json!(perm));
        }

        Ok((screen.id.as_str().to_owned(), Value::Object(obj)))
    }
}

/// Compile a frozen canonical schema v2 [`Project`] and its resource byte map
/// into deterministic legacy `HmiProject` JSON bytes.
pub fn compile_project(
    project: &Project,
    resource_bytes: &BTreeMap<String, Vec<u8>>,
    options: CompileOptions,
) -> Result<CompiledProject, CompileError> {
    // 1. Validate canonical project model.
    let report = validate(project);
    if report
        .diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error)
    {
        return Err(CompileError::InvalidProject);
    }

    // 2. Enforce quarantine / production admission boundary.
    if has_quarantine_provenance(project) {
        match options.mode {
            CompileMode::ProductionLive => return Err(CompileError::QuarantinedProjectRefused),
            CompileMode::DevelopmentSimulation if !options.allow_quarantined_simulation => {
                return Err(CompileError::QuarantinedProjectRefused);
            }
            CompileMode::DevelopmentSimulation => {}
        }
    }

    // 3. Explicit top-level fail-closed refusals for unsupported canonical v2 features.
    if !project.alarms.is_empty() {
        return Err(CompileError::UnsupportedFeature("alarms"));
    }
    if !project.recipes.is_empty() {
        return Err(CompileError::UnsupportedFeature("recipes"));
    }
    if !project.data_types.is_empty() {
        return Err(CompileError::UnsupportedFeature("structDataTypes"));
    }
    if !project.breakpoints.is_empty() {
        return Err(CompileError::UnsupportedFeature("breakpoints"));
    }
    if !project.device_profiles.is_empty() {
        return Err(CompileError::UnsupportedFeature("deviceProfiles"));
    }
    if !project.styles.is_empty() {
        return Err(CompileError::UnsupportedFeature("namedStyles"));
    }
    if project.localization.locales.len() > 1 || !project.localization.strings.is_empty() {
        return Err(CompileError::UnsupportedFeature("multiLocaleLocalization"));
    }

    let mut ctx = CompileContext {
        tags: project.tags.iter().map(|t| (t.id.as_str(), t)).collect(),
        contracts: project
            .v2
            .components
            .components
            .iter()
            .map(|c| (c.id.as_str(), &c.contract))
            .collect(),
        expressions: {
            let mut map: BTreeMap<&str, Vec<(&str, &str)>> = BTreeMap::new();
            for e in &project.v2.bindings.expressions {
                if e.contract_version != 1 {
                    return Err(CompileError::UnsupportedFeature(
                        "expressionContractVersion",
                    ));
                }
                map.entry(e.node.as_str())
                    .or_default()
                    .push((e.property.as_str(), e.expression.as_str()));
            }
            map
        },
        node_permissions: project
            .v2
            .industrial
            .nodes
            .iter()
            .map(|np| (np.node.as_str(), np.required_permission.as_str()))
            .collect(),
        candidate_handlers: {
            let mut map: BTreeMap<
                &str,
                Vec<&hmi_project_model::model::v2_components::CandidateHandler>,
            > = BTreeMap::new();
            for h in &project.v2.components.handlers {
                match &h.origin {
                    EventOrigin::Node(id) => map.entry(id.as_str()).or_default().push(h),
                    EventOrigin::Screen(_) => {
                        return Err(CompileError::UnsupportedFeature("screenLifecycleEvents"));
                    }
                }
            }
            map
        },
        symbol_defs: project
            .v2
            .components
            .definitions
            .iter()
            .filter_map(|d| match d.classification {
                DefinitionKind::IndustrialSymbol { symbol } => {
                    Some((d.definition.as_str(), symbol))
                }
                DefinitionKind::Faceplate {} => None,
            })
            .collect(),
        resource_media: project
            .resources
            .iter()
            .filter_map(|r| r.media_type.as_deref().map(|m| (r.id.as_str(), m)))
            .collect(),
        min_version: 1,
    };

    // 4. B5-07 gated asset compilation.
    let mut assets = Map::new();
    for res in &project.resources {
        let (id, val) = compile_resource(res, resource_bytes, &options.raster_limits)?;
        assets.insert(id, val);
    }
    if !assets.is_empty() {
        ctx.bump(7);
    }

    // 5. Connections -> externalSources.
    let conn_policies: BTreeMap<&str, _> = project
        .v2
        .industrial
        .connections
        .iter()
        .map(|p| (p.connection.as_str(), p))
        .collect();
    let mut external_sources = Map::new();
    let mut active_source_ids = BTreeSet::new();
    for conn in &project.connections {
        if conn.credential_ref.is_some() {
            return Err(CompileError::UnsupportedFeature("connectionCredentialRef"));
        }
        let policy = conn_policies.get(conn.id.as_str()).copied();
        let driver_v2 = policy
            .map(|p| p.driver)
            .unwrap_or_else(|| conn.driver.into());
        let commissioning = policy.is_some_and(|p| p.commissioning_allowed);
        let (legacy_driver, config) = match driver_v2 {
            DriverKindV2::SiemensS7 => {
                let ep = conn
                    .endpoint
                    .as_deref()
                    .ok_or(CompileError::InvalidProject)?;
                let (host, port) = ep
                    .rsplit_once(':')
                    .ok_or(CompileError::UnsupportedFeature("s7EndpointFormat"))?;
                let mut cfg = Map::new();
                cfg.insert("host".into(), json!(host));
                cfg.insert("port".into(), json!(port));
                for k in ["rack", "slot", "pollIntervalMs", "timeoutMs"] {
                    if let Some(v) = conn.parameters.get(k) {
                        cfg.insert(k.into(), json!(v.to_string().trim_matches('"')));
                    }
                }
                ("s7", cfg)
            }
            DriverKindV2::OpcUa => {
                let ep = conn
                    .endpoint
                    .as_deref()
                    .ok_or(CompileError::InvalidProject)?;
                let mut cfg = Map::new();
                cfg.insert("endpoint".into(), json!(ep));
                let sec_pol = conn
                    .parameters
                    .get("securityPolicy")
                    .and_then(Value::as_str)
                    .unwrap_or("None");
                let sec_mode = conn
                    .parameters
                    .get("securityMode")
                    .and_then(Value::as_str)
                    .unwrap_or("None");
                cfg.insert("securityPolicy".into(), json!(sec_pol));
                cfg.insert("securityMode".into(), json!(sec_mode));
                if let Some(v) = conn.parameters.get("publishingIntervalMs") {
                    cfg.insert(
                        "publishingIntervalMs".into(),
                        json!(v.to_string().trim_matches('"')),
                    );
                }
                ("opcua", cfg)
            }
            DriverKindV2::Loopback => {
                if commissioning {
                    return Err(CompileError::InvalidProject);
                }
                ("loopback", Map::new())
            }
            DriverKindV2::Simulator => {
                // Pure in-editor simulator connections without loopback mapping are omitted.
                continue;
            }
            DriverKindV2::ModbusTcp | DriverKindV2::Custom => {
                return Err(CompileError::UnsupportedFeature("driverKind"));
            }
        };
        if commissioning {
            ctx.bump(9);
        }
        let mut src = Map::new();
        src.insert("id".into(), json!(conn.id.as_str()));
        src.insert("displayName".into(), json!(conn.name));
        src.insert("driver".into(), json!(legacy_driver));
        src.insert("configuration".into(), Value::Object(config));
        if commissioning {
            src.insert("commissioningAllowed".into(), json!(true));
        }
        active_source_ids.insert(conn.id.as_str().to_owned());
        external_sources.insert(conn.id.as_str().to_owned(), Value::Object(src));
    }

    // 6. Tags -> variables & externalMappings.
    let unit_symbols: BTreeMap<&str, &str> = project
        .units
        .iter()
        .map(|u| (u.id.as_str(), u.symbol.as_str()))
        .collect();
    let write_policies: BTreeMap<&str, _> = project
        .v2
        .industrial
        .tags
        .iter()
        .map(|p| (p.tag.as_str(), p))
        .collect();
    let mut variables = Map::new();
    let mut external_mappings = Map::new();
    for tag in &project.tags {
        let (legacy_ty, _) = legacy_data_type(&tag.data_type)?;
        let write_policy = write_policies.get(tag.id.as_str()).copied();
        let legacy_var_access = match tag.access {
            TagAccess::Read => "READ_ONLY",
            TagAccess::ReadWrite => "READ_WRITE",
            TagAccess::Write => return Err(CompileError::UnsupportedFeature("writeOnlyTag")),
        };
        let mut var = Map::new();
        var.insert("id".into(), json!(tag.id.as_str()));
        var.insert("path".into(), json!(tag.name));
        var.insert("type".into(), json!(legacy_ty));
        var.insert("access".into(), json!(legacy_var_access));
        if let Some(desc) = &tag.description {
            var.insert("description".into(), json!(desc));
        }
        if let Some(init) = &tag.initial_value {
            var.insert(
                "defaultValue".into(),
                compile_default_value(&tag.data_type, init)?,
            );
        }
        if let Some(unit_id) = &tag.unit {
            if let Some(sym) = unit_symbols.get(unit_id.as_str()) {
                var.insert("engineeringUnit".into(), json!(sym));
            }
        }
        variables.insert(tag.id.as_str().to_owned(), Value::Object(var));

        if let Some(conn_id) = &tag.connection {
            if !active_source_ids.contains(conn_id.as_str()) {
                // Simulator-backed tag stays internal unless backed by a legacy external source.
                if project
                    .connections
                    .iter()
                    .any(|c| c.id == *conn_id && c.driver == DriverKind::Simulator)
                {
                    continue;
                }
                return Err(CompileError::InvalidProject);
            }
            let addr = tag.address.as_deref().ok_or(CompileError::InvalidProject)?;
            let map_id = format!("map-{}", tag.id.as_str());
            let mut m = Map::new();
            m.insert("id".into(), json!(map_id));
            m.insert("internalVariableId".into(), json!(tag.id.as_str()));
            m.insert("sourceId".into(), json!(conn_id.as_str()));
            m.insert("externalAddress".into(), json!(addr));
            if tag.access == TagAccess::ReadWrite {
                let policy = write_policy
                    .ok_or(CompileError::UnsupportedFeature("unpolicyWritableMapping"))?;
                if !policy.write_confirmed {
                    return Err(CompileError::UnsupportedFeature(
                        "unconfirmedWritableMapping",
                    ));
                }
                ctx.bump(9);
                m.insert("access".into(), json!("READ_WRITE"));
                m.insert("writeConfirmed".into(), json!(true));
                let mut cmd = Map::new();
                cmd.insert(
                    "kind".into(),
                    json!(match policy.command {
                        CommandKind::Momentary => "MOMENTARY",
                        CommandKind::Maintained => "MAINTAINED",
                        CommandKind::Setpoint => "SETPOINT",
                    }),
                );
                if policy.critical {
                    cmd.insert("critical".into(), json!(true));
                }
                match policy.command {
                    CommandKind::Momentary => {
                        if policy.confirmation_required {
                            cmd.insert("confirm".into(), json!(true));
                        }
                        if policy.limits != WriteLimits::default() {
                            return Err(CompileError::UnsupportedFeature("unexpectedWriteLimits"));
                        }
                        let Some(pulse) = policy.pulse_ms else {
                            return Err(CompileError::UnsupportedFeature("missingMomentaryPulse"));
                        };
                        if !(MIN_MOMENTARY_PULSE_MS..=MAX_MOMENTARY_PULSE_MS).contains(&pulse) {
                            return Err(CompileError::UnsupportedFeature(
                                "momentaryPulseOutOfRange",
                            ));
                        }
                        cmd.insert("pulseMs".into(), json!(pulse));
                    }
                    CommandKind::Maintained => {
                        // HMI Forge treats MAINTAINED commands as always confirmed
                        // and rejects an explicit `confirm: true` flag.
                        if !policy.confirmation_required {
                            return Err(CompileError::UnsupportedFeature(
                                "unconfirmedSetpointOrMaintained",
                            ));
                        }
                        if policy.pulse_ms.is_some() {
                            return Err(CompileError::UnsupportedFeature(
                                "unexpectedPulseDuration",
                            ));
                        }
                        if policy.limits != WriteLimits::default() {
                            return Err(CompileError::UnsupportedFeature("unexpectedWriteLimits"));
                        }
                    }
                    CommandKind::Setpoint => {
                        // HMI Forge treats SETPOINT commands as always confirmed
                        // and rejects an explicit `confirm: true` flag.
                        if !policy.confirmation_required {
                            return Err(CompileError::UnsupportedFeature(
                                "unconfirmedSetpointOrMaintained",
                            ));
                        }
                        if policy.pulse_ms.is_some() {
                            return Err(CompileError::UnsupportedFeature(
                                "unexpectedPulseDuration",
                            ));
                        }
                        let (Some(min_num), Some(max_num)) =
                            (&policy.limits.min, &policy.limits.max)
                        else {
                            return Err(CompileError::UnsupportedFeature("missingSetpointLimits"));
                        };
                        let min_f = validate_setpoint_limit(min_num, &tag.data_type)?;
                        let max_f = validate_setpoint_limit(max_num, &tag.data_type)?;
                        if min_f.partial_cmp(&max_f) != Some(std::cmp::Ordering::Less) {
                            return Err(CompileError::UnsupportedFeature("invalidSetpointRange"));
                        }
                        cmd.insert("min".into(), Value::Number(min_num.clone()));
                        cmd.insert("max".into(), Value::Number(max_num.clone()));
                        if let Some(step_num) = &policy.limits.step {
                            let step_f = validate_setpoint_limit(step_num, &tag.data_type)?;
                            if step_f.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
                                return Err(CompileError::UnsupportedFeature(
                                    "invalidSetpointStep",
                                ));
                            }
                            cmd.insert("step".into(), Value::Number(step_num.clone()));
                        }
                    }
                }
                m.insert("command".into(), Value::Object(cmd));
            } else {
                m.insert("access".into(), json!("READ"));
            }
            external_mappings.insert(map_id, Value::Object(m));
        }
    }

    let has_live_mapping = !external_mappings.is_empty();
    if options.mode == CompileMode::ProductionLive && !has_live_mapping {
        return Err(CompileError::LiveMappingRequired);
    }

    // 7. Screens & screenOrder.
    let screen_perms: BTreeMap<&str, &str> = project
        .v2
        .industrial
        .screens
        .iter()
        .map(|sp| (sp.screen.as_str(), sp.required_permission.as_str()))
        .collect();
    let mut screens_obj = Map::new();
    let mut screen_order = Vec::with_capacity(project.screens.len());
    for screen in &project.screens {
        let (id, val) = ctx.compile_screen(screen, &screen_perms)?;
        screen_order.push(id.clone());
        screens_obj.insert(id, val);
    }
    let emit_screen_order = screen_order.len() > 1 || !project.navigation.items.is_empty();
    if emit_screen_order {
        ctx.bump(5);
    }

    // 8. Reusable non-symbol ComponentDefinitions (v6).
    let faceplate_ids: BTreeSet<&str> = project
        .v2
        .components
        .definitions
        .iter()
        .filter_map(|d| match d.classification {
            DefinitionKind::Faceplate {} => Some(d.definition.as_str()),
            DefinitionKind::IndustrialSymbol { .. } => None,
        })
        .collect();
    let mut comp_defs = Map::new();
    for def in &project.components {
        if ctx.symbol_defs.contains_key(def.id.as_str()) {
            // Industrial symbols compile directly to legacy symbol components when instantiated.
            continue;
        }
        ctx.bump(6);
        let mut elements = Map::new();
        match &def.root.element {
            Element::Absolute {} => {
                for ch in &def.root.children {
                    let (id, val) = ctx.compile_leaf_component(ch, false)?;
                    elements.insert(id, val);
                }
            }
            _ => {
                return Err(CompileError::UnsupportedFeature(
                    "nonAbsoluteDefinitionRoot",
                ))
            }
        }
        let mut d = Map::new();
        d.insert("id".into(), json!(def.id.as_str()));
        d.insert("displayName".into(), json!(def.name));
        d.insert("type".into(), json!("composite"));
        if faceplate_ids.contains(def.id.as_str()) {
            d.insert("kind".into(), json!("FACEPLATE"));
        }
        if let Some(desc) = &def.description {
            d.insert("description".into(), json!(desc));
        }
        if let Some(cat) = &def.category {
            d.insert("category".into(), json!(cat));
        }
        d.insert("composition".into(), json!({ "elements": elements }));
        comp_defs.insert(def.id.as_str().to_owned(), Value::Object(d));
    }

    // 9. Theme (v7, optional single theme).
    let theme_val = if project.themes.len() > 1 {
        return Err(CompileError::UnsupportedFeature("multipleThemes"));
    } else if let Some(theme) = project.themes.first() {
        ctx.bump(7);
        Some(json!({
            "id": theme.id.as_str(),
            "name": theme.name,
            "colors": theme.colors,
        }))
    } else {
        None
    };

    // 10. Security policy (v8).
    const BUILTIN_PERMISSIONS: [&str; 6] = [
        "operate",
        "setpoints",
        "alarms.acknowledge",
        "project.edit",
        "users.manage",
        "runtime.maintain",
    ];
    let sec = &project.v2.industrial.security;
    let security_val = if !sec.permissions.is_empty()
        || !sec.roles.is_empty()
        || sec.anonymous_role.is_some()
        || sec.auto_logout_seconds.is_some()
    {
        ctx.bump(8);
        let mut s = Map::new();
        let mut perms_map = Map::new();
        for p in &sec.permissions {
            if BUILTIN_PERMISSIONS.contains(&p.id.as_str()) {
                continue;
            }
            perms_map.insert(
                p.id.as_str().to_owned(),
                json!({ "id": p.id.as_str(), "displayName": p.name }),
            );
        }
        s.insert("permissions".into(), Value::Object(perms_map));
        let mut roles_map = Map::new();
        for r in &sec.roles {
            roles_map.insert(
                r.id.as_str().to_owned(),
                json!({
                    "id": r.id.as_str(),
                    "displayName": r.name,
                    "permissions": r.permissions.iter().map(|p| p.as_str()).collect::<Vec<_>>()
                }),
            );
        }
        s.insert("roles".into(), Value::Object(roles_map));
        if let Some(anon) = &sec.anonymous_role {
            s.insert("anonymousRoleId".into(), json!(anon.as_str()));
        }
        if let Some(secs) = sec.auto_logout_seconds {
            if secs == 0 || secs % 60 != 0 {
                return Err(CompileError::UnsupportedFeature("subMinuteAutoLogout"));
            }
            s.insert("autoLogoutMinutes".into(), json!(secs / 60));
        }
        Some(Value::Object(s))
    } else {
        None
    };

    // 11. Enumerations (v10).
    let mut enumerations = Map::new();
    for en in &project.v2.bindings.enumerations {
        ctx.bump(10);
        if !matches!(en.data_type, DataType::Int32 | DataType::Int64) {
            return Err(CompileError::UnsupportedFeature("nonIntegerEnumeration"));
        }
        let mut entries = Vec::with_capacity(en.entries.len());
        for entry in &en.entries {
            let v = entry
                .value
                .as_i64()
                .ok_or(CompileError::UnsupportedFeature("nonIntegerEnumeration"))?;
            let text = literal_text(&entry.label)?;
            let mut eobj = Map::new();
            eobj.insert("value".into(), json!(v));
            eobj.insert("text".into(), json!(text));
            if let Some(c) = &entry.color {
                eobj.insert("color".into(), json!(c));
            }
            entries.push(Value::Object(eobj));
        }
        enumerations.insert(
            en.id.as_str().to_owned(),
            json!({
                "id": en.id.as_str(),
                "name": en.name,
                "entries": entries,
            }),
        );
    }

    // 12. Assemble root HmiProject JSON deterministically.
    let schema_version = ctx.min_version;
    let startup_screen = project.navigation.start_screen.as_str().to_owned();
    let mut root = Map::new();
    root.insert("schemaVersion".into(), json!(schema_version));
    root.insert(
        "metadata".into(),
        json!({
            "name": project.metadata.name,
            "defaultScreenId": startup_screen,
            "targetProfiles": [],
        }),
    );
    if emit_screen_order {
        root.insert("screenOrder".into(), json!(screen_order));
    }
    root.insert("screens".into(), Value::Object(screens_obj));
    root.insert("variables".into(), Value::Object(variables));
    root.insert("externalSources".into(), Value::Object(external_sources));
    root.insert("externalMappings".into(), Value::Object(external_mappings));
    if !comp_defs.is_empty() {
        root.insert("componentDefinitions".into(), Value::Object(comp_defs));
    }
    if !assets.is_empty() {
        root.insert("assets".into(), Value::Object(assets));
    }
    if let Some(t) = theme_val {
        root.insert("theme".into(), t);
    }
    if let Some(s) = security_val {
        root.insert("security".into(), s);
    }
    if !enumerations.is_empty() {
        root.insert("enumerations".into(), Value::Object(enumerations));
    }

    let project_json =
        serde_json::to_vec(&Value::Object(root)).map_err(|_| CompileError::SerializationFailed)?;
    if project_json.len() > hmi_legacy_import::MAX_INPUT_BYTES {
        return Err(CompileError::SerializationFailed);
    }

    Ok(CompiledProject {
        mode: options.mode,
        project_json,
        schema_version,
        project_name: project.metadata.name.clone(),
        startup_screen,
        has_live_mapping,
    })
}
