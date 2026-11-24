//! Project-level checks: header, identifiers, indices, resources, themes,
//! localization, data types, tags, alarms, recipes, navigation and
//! component-definition cycles.

use std::collections::{BTreeMap, BTreeSet};

use super::{Diagnostic, DiagnosticCode as C, Severity};
use crate::id::validate_id;
use crate::model::*;
use crate::{CURRENT_SCHEMA_VERSION, FORMAT_ID};

/// Lookup tables built once per validation run. All maps are ordered so
/// no iteration can introduce nondeterminism.
pub(super) struct Index<'p> {
    pub screens: BTreeMap<&'p str, &'p Screen>,
    pub definitions: BTreeMap<&'p str, &'p ComponentDefinition>,
    pub tags: BTreeMap<&'p str, &'p Tag>,
    pub data_types: BTreeMap<&'p str, &'p StructType>,
    pub themes: BTreeMap<&'p str, &'p Theme>,
    pub styles: BTreeMap<&'p str, &'p Style>,
    pub resources: BTreeMap<&'p str, &'p Resource>,
    pub connections: BTreeMap<&'p str, &'p Connection>,
    pub units: BTreeSet<&'p str>,
    pub alarms: BTreeSet<&'p str>,
    pub alarm_groups: BTreeSet<&'p str>,
    pub recipes: BTreeMap<&'p str, &'p RecipeDefinition>,
    pub breakpoints: BTreeSet<&'p str>,
    pub device_profiles: BTreeSet<&'p str>,
    pub locales: BTreeSet<&'p str>,
    pub text_keys: BTreeSet<&'p str>,
}

pub(super) struct Validator<'p> {
    pub p: &'p Project,
    pub ix: Index<'p>,
    diags: Vec<Diagnostic>,
}

/// Appends a JSON Pointer segment.
pub(super) fn at(base: &str, seg: impl std::fmt::Display) -> String {
    let seg = seg.to_string().replace('~', "~0").replace('/', "~1");
    format!("{base}/{seg}")
}

pub(super) fn is_identifier_name(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn is_locale_tag(s: &str) -> bool {
    let mut parts = s.split('-');
    let Some(lang) = parts.next() else {
        return false;
    };
    (2..=3).contains(&lang.len())
        && lang.chars().all(|c| c.is_ascii_alphabetic())
        && parts.all(|p| (2..=8).contains(&p.len()) && p.chars().all(|c| c.is_ascii_alphanumeric()))
}

fn is_text_key(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphanumeric())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

pub(super) fn is_hex_color(s: &str) -> bool {
    s.strip_prefix('#')
        .is_some_and(|h| (h.len() == 6 || h.len() == 8) && h.chars().all(|c| c.is_ascii_hexdigit()))
}

fn is_safe_relative_path(s: &str) -> bool {
    !s.is_empty()
        && !s.starts_with('/')
        && !s.contains('\\')
        && !s.contains(':')
        && s.split('/')
            .all(|seg| !seg.is_empty() && seg != "." && seg != "..")
}

pub(super) fn finite_non_negative(v: f64) -> bool {
    v.is_finite() && v >= 0.0
}

const SECRET_HINTS: [&str; 7] = [
    "password",
    "passwd",
    "secret",
    "token",
    "apikey",
    "api_key",
    "privatekey",
];

impl<'p> Validator<'p> {
    pub fn new(p: &'p Project) -> Self {
        Self {
            p,
            ix: Index {
                screens: BTreeMap::new(),
                definitions: BTreeMap::new(),
                tags: BTreeMap::new(),
                data_types: BTreeMap::new(),
                themes: BTreeMap::new(),
                styles: BTreeMap::new(),
                resources: BTreeMap::new(),
                connections: BTreeMap::new(),
                units: BTreeSet::new(),
                alarms: BTreeSet::new(),
                alarm_groups: BTreeSet::new(),
                recipes: BTreeMap::new(),
                breakpoints: BTreeSet::new(),
                device_profiles: BTreeSet::new(),
                locales: BTreeSet::new(),
                text_keys: BTreeSet::new(),
            },
            diags: Vec::new(),
        }
    }

    pub fn into_diagnostics(self) -> Vec<Diagnostic> {
        self.diags
    }

    pub fn error(&mut self, code: C, path: impl Into<String>, message: impl Into<String>) {
        self.diags.push(Diagnostic {
            severity: Severity::Error,
            code,
            path: path.into(),
            message: message.into(),
        });
    }

    pub fn warning(&mut self, code: C, path: impl Into<String>, message: impl Into<String>) {
        self.diags.push(Diagnostic {
            severity: Severity::Warning,
            code,
            path: path.into(),
            message: message.into(),
        });
    }

    pub fn run(&mut self) {
        self.check_header();
        self.check_identifiers();
        self.build_index();
        self.check_metadata();
        self.check_settings();
        self.check_device_profiles();
        self.check_breakpoints();
        self.check_localization();
        self.check_resources();
        self.check_themes();
        self.check_styles();
        self.check_units();
        self.check_data_types();
        self.check_connections();
        self.check_tags();
        self.check_alarms();
        self.check_recipes();
        self.check_navigation();
        self.check_definitions();
        self.check_screens();
        self.check_definition_cycles();
    }

    // ----------------------------------------------------------------- header

    fn check_header(&mut self) {
        if self.p.format != FORMAT_ID {
            self.error(
                C::UnsupportedFormat,
                "/format",
                format!("format is {:?}, expected {FORMAT_ID:?}", self.p.format),
            );
        }
        if self.p.schema_version != CURRENT_SCHEMA_VERSION {
            self.error(
                C::UnsupportedSchemaVersion,
                "/schemaVersion",
                format!(
                    "schemaVersion {} is not supported by this validator (expected {}); \
                     migrate the document first",
                    self.p.schema_version, CURRENT_SCHEMA_VERSION
                ),
            );
        }
    }

    // ------------------------------------------------------------ identifiers

    fn collect_ids(&self) -> Vec<(String, String)> {
        let p = self.p;
        let mut ids: Vec<(String, String)> = Vec::new();
        let mut push = |id: &str, path: String| ids.push((id.to_owned(), path));
        push(p.id.as_str(), "/id".into());
        for (i, x) in p.navigation.items.iter().enumerate() {
            push(x.id.as_str(), format!("/navigation/items/{i}/id"));
        }
        for (i, x) in p.device_profiles.iter().enumerate() {
            push(x.id.as_str(), format!("/deviceProfiles/{i}/id"));
        }
        for (i, x) in p.breakpoints.iter().enumerate() {
            push(x.id.as_str(), format!("/breakpoints/{i}/id"));
        }
        for (i, x) in p.themes.iter().enumerate() {
            push(x.id.as_str(), format!("/themes/{i}/id"));
        }
        for (i, x) in p.styles.iter().enumerate() {
            push(x.id.as_str(), format!("/styles/{i}/id"));
        }
        for (i, x) in p.resources.iter().enumerate() {
            push(x.id.as_str(), format!("/resources/{i}/id"));
        }
        for (i, x) in p.units.iter().enumerate() {
            push(x.id.as_str(), format!("/units/{i}/id"));
        }
        for (i, x) in p.data_types.iter().enumerate() {
            push(x.id.as_str(), format!("/dataTypes/{i}/id"));
        }
        for (i, x) in p.connections.iter().enumerate() {
            push(x.id.as_str(), format!("/connections/{i}/id"));
        }
        for (i, x) in p.tags.iter().enumerate() {
            push(x.id.as_str(), format!("/tags/{i}/id"));
        }
        for (i, x) in p.alarms.iter().enumerate() {
            push(x.id.as_str(), format!("/alarms/{i}/id"));
        }
        for (i, r) in p.recipes.iter().enumerate() {
            push(r.id.as_str(), format!("/recipes/{i}/id"));
            for (j, f) in r.fields.iter().enumerate() {
                push(f.id.as_str(), format!("/recipes/{i}/fields/{j}/id"));
            }
            for (j, s) in r.sets.iter().enumerate() {
                push(s.id.as_str(), format!("/recipes/{i}/sets/{j}/id"));
            }
        }
        fn nodes(node: &Node, path: String, out: &mut Vec<(String, String)>, depth: usize) {
            out.push((node.id.as_str().to_owned(), at(&path, "id")));
            if depth >= crate::MAX_NODE_DEPTH {
                return;
            }
            for (i, c) in node.children.iter().enumerate() {
                nodes(c, format!("{path}/children/{i}"), out, depth + 1);
            }
        }
        for (i, d) in p.components.iter().enumerate() {
            ids.push((d.id.as_str().to_owned(), format!("/components/{i}/id")));
            nodes(&d.root, format!("/components/{i}/root"), &mut ids, 1);
        }
        for (i, s) in p.screens.iter().enumerate() {
            ids.push((s.id.as_str().to_owned(), format!("/screens/{i}/id")));
            nodes(&s.root, format!("/screens/{i}/root"), &mut ids, 1);
        }
        ids
    }

    fn check_identifiers(&mut self) {
        let ids = self.collect_ids();
        let mut first: BTreeMap<String, String> = BTreeMap::new();
        for (id, path) in ids {
            if let Err(e) = validate_id(&id) {
                self.error(
                    C::InvalidIdentifier,
                    &path,
                    format!("identifier {id:?}: {e}"),
                );
            }
            match first.get(&id) {
                Some(prev) => {
                    let msg = format!("identifier {id:?} is already used at {prev}");
                    self.error(C::DuplicateIdentifier, &path, msg);
                }
                None => {
                    first.insert(id, path);
                }
            }
        }
    }

    // ------------------------------------------------------------------ index

    fn build_index(&mut self) {
        let p = self.p;
        let ix = &mut self.ix;
        for s in &p.screens {
            ix.screens.entry(s.id.as_str()).or_insert(s);
        }
        for d in &p.components {
            ix.definitions.entry(d.id.as_str()).or_insert(d);
        }
        for t in &p.tags {
            ix.tags.entry(t.id.as_str()).or_insert(t);
        }
        for t in &p.data_types {
            ix.data_types.entry(t.id.as_str()).or_insert(t);
        }
        for t in &p.themes {
            ix.themes.entry(t.id.as_str()).or_insert(t);
        }
        for s in &p.styles {
            ix.styles.entry(s.id.as_str()).or_insert(s);
        }
        for r in &p.resources {
            ix.resources.entry(r.id.as_str()).or_insert(r);
        }
        for c in &p.connections {
            ix.connections.entry(c.id.as_str()).or_insert(c);
        }
        ix.units = p.units.iter().map(|u| u.id.as_str()).collect();
        ix.alarms = p.alarms.iter().map(|a| a.id.as_str()).collect();
        ix.alarm_groups = p.alarms.iter().filter_map(|a| a.group.as_deref()).collect();
        for r in &p.recipes {
            ix.recipes.entry(r.id.as_str()).or_insert(r);
        }
        ix.breakpoints = p.breakpoints.iter().map(|b| b.id.as_str()).collect();
        ix.device_profiles = p.device_profiles.iter().map(|d| d.id.as_str()).collect();
        ix.locales = p.localization.locales.iter().map(String::as_str).collect();
        ix.text_keys = p.localization.strings.keys().map(String::as_str).collect();
    }

    // ------------------------------------------------------- simple sections

    fn check_metadata(&mut self) {
        if self.p.metadata.name.trim().is_empty() {
            self.error(C::InvalidValue, "/metadata/name", "project name is empty");
        }
    }

    fn check_settings(&mut self) {
        if let Some(t) = &self.p.settings.default_theme {
            if !self.ix.themes.contains_key(t.as_str()) {
                self.error(
                    C::UnknownReference,
                    "/settings/defaultTheme",
                    format!("unknown theme {t:?}"),
                );
            }
        }
        if let Some(d) = &self.p.settings.default_device_profile {
            if !self.ix.device_profiles.contains(d.as_str()) {
                self.error(
                    C::UnknownReference,
                    "/settings/defaultDeviceProfile",
                    format!("unknown device profile {d:?}"),
                );
            }
        }
    }

    fn check_device_profiles(&mut self) {
        for (i, d) in self.p.device_profiles.iter().enumerate() {
            let path = format!("/deviceProfiles/{i}");
            if d.width_px == 0 || d.height_px == 0 {
                self.error(C::InvalidValue, &path, "resolution must be non-zero");
            }
            if !(d.density.is_finite() && d.density > 0.0) {
                self.error(C::InvalidValue, at(&path, "density"), "density must be > 0");
            }
            let landscape = d.width_px >= d.height_px;
            if landscape != (d.orientation == Orientation::Landscape) {
                self.warning(
                    C::InvalidValue,
                    at(&path, "orientation"),
                    "orientation does not match the resolution",
                );
            }
        }
    }

    fn check_breakpoints(&mut self) {
        for (i, b) in self.p.breakpoints.iter().enumerate() {
            let path = format!("/breakpoints/{i}");
            if !finite_non_negative(b.min_width) {
                self.error(
                    C::InvalidValue,
                    at(&path, "minWidth"),
                    "minWidth must be >= 0",
                );
            }
            if let Some(max) = b.max_width {
                if !(max.is_finite() && max > b.min_width) {
                    self.error(
                        C::InvalidValue,
                        at(&path, "maxWidth"),
                        "maxWidth must be greater than minWidth",
                    );
                }
            }
        }
    }

    fn check_localization(&mut self) {
        let l = &self.p.localization;
        let mut seen = BTreeSet::new();
        for (i, loc) in l.locales.iter().enumerate() {
            let path = format!("/localization/locales/{i}");
            if !is_locale_tag(loc) {
                self.error(
                    C::InvalidLocalization,
                    &path,
                    format!("invalid locale tag {loc:?}"),
                );
            }
            if !seen.insert(loc.as_str()) {
                self.error(
                    C::InvalidLocalization,
                    &path,
                    format!("duplicate locale {loc:?}"),
                );
            }
        }
        if !l.locales.contains(&l.default_locale) {
            self.error(
                C::InvalidLocalization,
                "/localization/defaultLocale",
                format!(
                    "default locale {:?} is not declared in locales",
                    l.default_locale
                ),
            );
        }
        for (key, translations) in &l.strings {
            let path = at("/localization/strings", key);
            if !is_text_key(key) {
                self.error(
                    C::InvalidLocalization,
                    &path,
                    format!("invalid text key {key:?}"),
                );
            }
            for loc in translations.keys() {
                if !seen.contains(loc.as_str()) {
                    self.error(
                        C::InvalidLocalization,
                        at(&path, loc),
                        format!("translation for undeclared locale {loc:?}"),
                    );
                }
            }
            for loc in &l.locales {
                if translations.contains_key(loc) {
                    continue;
                }
                if *loc == l.default_locale {
                    self.error(
                        C::InvalidLocalization,
                        &path,
                        format!("missing text for the default locale {loc:?}"),
                    );
                } else {
                    self.warning(
                        C::MissingTranslation,
                        &path,
                        format!("missing {loc:?} translation"),
                    );
                }
            }
        }
    }

    fn check_resources(&mut self) {
        for (i, r) in self.p.resources.iter().enumerate() {
            let path = format!("/resources/{i}");
            if !is_safe_relative_path(&r.path) {
                self.error(
                    C::InvalidResource,
                    at(&path, "path"),
                    format!(
                        "resource path {:?} must be relative, use '/', and contain no '..', ':' or '\\\\'",
                        r.path
                    ),
                );
            }
            if let Some(h) = &r.sha256 {
                if !(h.len() == 64 && h.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f'))) {
                    self.error(
                        C::InvalidResource,
                        at(&path, "sha256"),
                        "sha256 must be 64 lowercase hex digits",
                    );
                }
            }
        }
    }

    fn check_themes(&mut self) {
        for (i, t) in self.p.themes.iter().enumerate() {
            let path = format!("/themes/{i}");
            for (name, c) in &t.colors {
                if !is_hex_color(c) {
                    self.error(
                        C::InvalidStyle,
                        at(&at(&path, "colors"), name),
                        format!("color {c:?} must be #RRGGBB or #RRGGBBAA"),
                    );
                }
            }
            for (name, ts) in &t.typography {
                let tp = at(&at(&path, "typography"), name);
                if !(ts.size.is_finite() && ts.size > 0.0) {
                    self.error(C::InvalidStyle, at(&tp, "size"), "font size must be > 0");
                }
                if !(100..=900).contains(&ts.weight) {
                    self.error(
                        C::InvalidStyle,
                        at(&tp, "weight"),
                        "weight must be 100..=900",
                    );
                }
                if let Some(lh) = ts.line_height {
                    if !(lh.is_finite() && lh > 0.0) {
                        self.error(C::InvalidStyle, at(&tp, "lineHeight"), "must be > 0");
                    }
                }
                if let Some(font) = &ts.font {
                    match self.ix.resources.get(font.as_str()) {
                        None => self.error(
                            C::UnknownReference,
                            at(&tp, "font"),
                            format!("unknown resource {font:?}"),
                        ),
                        Some(r) if r.kind != ResourceKind::Font => self.error(
                            C::InvalidReference,
                            at(&tp, "font"),
                            format!("resource {font:?} is not a font"),
                        ),
                        Some(_) => {}
                    }
                }
            }
            for (name, m) in &t.metrics {
                if !m.is_finite() {
                    self.error(
                        C::InvalidStyle,
                        at(&at(&path, "metrics"), name),
                        "metric must be finite",
                    );
                }
            }
        }
    }

    fn check_styles(&mut self) {
        for (i, s) in self.p.styles.iter().enumerate() {
            let path = format!("/styles/{i}/properties");
            for (name, value) in &s.properties {
                let pp = at(&path, name);
                match crate::catalog::find_property(s.target, name) {
                    None => self.error(
                        C::UnknownProperty,
                        pp,
                        format!(
                            "property {name:?} is not defined for {}",
                            s.target.map_or("all components", |k| k.as_str())
                        ),
                    ),
                    Some(spec) => self.check_value(value, &spec.ty, &pp),
                }
            }
        }
    }

    fn check_units(&mut self) {
        for (i, u) in self.p.units.iter().enumerate() {
            if u.symbol.trim().is_empty() {
                self.error(
                    C::InvalidValue,
                    format!("/units/{i}/symbol"),
                    "unit symbol is empty",
                );
            }
            if let Some(name) = &u.name {
                self.check_text(name, &format!("/units/{i}/name"));
            }
        }
    }

    // ------------------------------------------------------------ data types

    fn check_data_types(&mut self) {
        for (i, t) in self.p.data_types.iter().enumerate() {
            let path = format!("/dataTypes/{i}");
            if t.members.is_empty() {
                self.error(
                    C::InvalidDataType,
                    at(&path, "members"),
                    "struct has no members",
                );
            }
            let mut names = BTreeSet::new();
            for (j, m) in t.members.iter().enumerate() {
                let mp = format!("{path}/members/{j}");
                if !is_identifier_name(&m.name) {
                    self.error(
                        C::InvalidDataType,
                        at(&mp, "name"),
                        format!("invalid member name {:?}", m.name),
                    );
                }
                if !names.insert(m.name.as_str()) {
                    self.error(
                        C::InvalidDataType,
                        at(&mp, "name"),
                        format!("duplicate member name {:?}", m.name),
                    );
                }
                self.check_data_type(&m.data_type, &at(&mp, "dataType"));
            }
        }
        // Cycle detection over struct containment (through arrays too).
        let edges: BTreeMap<&str, Vec<&str>> = self
            .p
            .data_types
            .iter()
            .map(|t| {
                let mut out = Vec::new();
                for m in &t.members {
                    collect_struct_refs(&m.data_type, &mut out);
                }
                (t.id.as_str(), out)
            })
            .collect();
        let order: Vec<&str> = self.p.data_types.iter().map(|t| t.id.as_str()).collect();
        for cycle in find_cycles(&order, &edges) {
            let idx = self
                .p
                .data_types
                .iter()
                .position(|t| t.id.as_str() == cycle[0])
                .unwrap_or(0);
            self.error(
                C::CircularDataType,
                format!("/dataTypes/{idx}"),
                format!("data types contain each other: {}", cycle.join(" -> ")),
            );
        }
    }

    pub fn check_data_type(&mut self, dt: &DataType, path: &str) {
        match dt {
            DataType::Array { element, length } => {
                if *length == 0 {
                    self.error(C::InvalidDataType, path, "array length must be >= 1");
                }
                self.check_data_type(element, path);
            }
            DataType::Struct(id) if !self.ix.data_types.contains_key(id.as_str()) => {
                self.error(
                    C::UnknownReference,
                    path,
                    format!("unknown data type {id:?}"),
                );
            }
            _ => {}
        }
    }

    // ----------------------------------------------------------- connections

    fn check_connections(&mut self) {
        for (i, c) in self.p.connections.iter().enumerate() {
            let path = format!("/connections/{i}");
            if let Some(e) = &c.endpoint {
                if e.trim().is_empty() {
                    self.error(C::InvalidValue, at(&path, "endpoint"), "endpoint is empty");
                }
            }
            if c.driver != DriverKind::Simulator && c.endpoint.is_none() {
                self.warning(
                    C::InvalidValue,
                    at(&path, "endpoint"),
                    "no endpoint; it must be supplied at publish time",
                );
            }
            for key in c.parameters.keys() {
                let lower = key.to_ascii_lowercase();
                if SECRET_HINTS.iter().any(|h| lower.contains(h)) {
                    self.warning(
                        C::PossibleSecret,
                        at(&at(&path, "parameters"), key),
                        "looks like a secret; store it in the runtime credential store and \
                         reference it with credentialRef",
                    );
                }
            }
        }
    }

    // ------------------------------------------------------------------ tags

    fn check_tags(&mut self) {
        for (i, t) in self.p.tags.iter().enumerate() {
            let path = format!("/tags/{i}");
            if t.name.trim().is_empty() {
                self.error(C::InvalidTag, at(&path, "name"), "tag name is empty");
            }
            self.check_data_type(&t.data_type, &at(&path, "dataType"));
            match (&t.connection, &t.address) {
                (Some(c), addr) => {
                    if !self.ix.connections.contains_key(c.as_str()) {
                        self.error(
                            C::UnknownReference,
                            at(&path, "connection"),
                            format!("unknown connection {c:?}"),
                        );
                    }
                    if addr.as_deref().is_none_or(|a| a.trim().is_empty()) {
                        self.error(
                            C::InvalidTag,
                            at(&path, "address"),
                            "tags with a connection need an address",
                        );
                    }
                }
                (None, Some(_)) => self.error(
                    C::InvalidTag,
                    at(&path, "address"),
                    "internal tags (no connection) must not have an address",
                ),
                (None, None) => {}
            }
            let numeric = t.data_type.is_numeric();
            if let Some(u) = &t.unit {
                if !numeric {
                    self.error(
                        C::InvalidTag,
                        at(&path, "unit"),
                        "only numeric tags have units",
                    );
                }
                if !self.ix.units.contains(u.as_str()) {
                    self.error(
                        C::UnknownReference,
                        at(&path, "unit"),
                        format!("unknown unit {u:?}"),
                    );
                }
            }
            if let Some(r) = &t.range {
                if !numeric {
                    self.error(
                        C::InvalidTag,
                        at(&path, "range"),
                        "only numeric tags have ranges",
                    );
                }
                if !(r.min.is_finite() && r.max.is_finite() && r.min <= r.max) {
                    self.error(
                        C::InvalidTag,
                        at(&path, "range"),
                        "range requires min <= max",
                    );
                }
            }
            if let Some(v) = &t.initial_value {
                if let Err(e) = self.value_matches_data_type(v, &t.data_type) {
                    self.error(C::InvalidTag, at(&path, "initialValue"), e);
                }
            }
        }
    }

    // ---------------------------------------------------------------- alarms

    fn check_alarms(&mut self) {
        for (i, a) in self.p.alarms.iter().enumerate() {
            let path = format!("/alarms/{i}");
            self.check_text(&a.message, &at(&path, "message"));
            if !(1..=1000).contains(&a.priority) {
                self.error(
                    C::InvalidAlarm,
                    at(&path, "priority"),
                    "priority must be 1..=1000",
                );
            }
            if let Some(g) = &a.group {
                if g.trim().is_empty() {
                    self.error(C::InvalidAlarm, at(&path, "group"), "group is empty");
                }
            }
            let Some(tag) = self.ix.tags.get(a.tag.as_str()).copied() else {
                self.error(
                    C::UnknownReference,
                    at(&path, "tag"),
                    format!("unknown tag {:?}", a.tag),
                );
                continue;
            };
            let dt = match self.resolve_member(&tag.data_type, a.member.as_deref()) {
                Ok(dt) => dt,
                Err(e) => {
                    self.error(C::InvalidDataType, at(&path, "member"), e);
                    continue;
                }
            };
            if !tag.access.readable() {
                self.error(
                    C::InvalidAlarm,
                    at(&path, "tag"),
                    "alarm tag must be readable",
                );
            }
            let cp = at(&path, "condition");
            match &a.condition {
                AlarmCondition::High { limit } | AlarmCondition::Low { limit } => {
                    if !dt.is_numeric() {
                        self.error(
                            C::InvalidAlarm,
                            &cp,
                            "high/low conditions need a numeric value",
                        );
                    }
                    if !limit.is_finite() {
                        self.error(C::InvalidAlarm, at(&cp, "limit"), "limit must be finite");
                    }
                }
                AlarmCondition::Equals { value } => {
                    if let Err(e) = self.value_matches_data_type(value, &dt) {
                        self.error(C::InvalidAlarm, at(&cp, "value"), e);
                    }
                }
                AlarmCondition::BitSet { index } => match dt.integer_bits() {
                    Some(bits) if *index < bits => {}
                    Some(bits) => self.error(
                        C::InvalidAlarm,
                        at(&cp, "index"),
                        format!("bit {index} out of range for a {bits}-bit value"),
                    ),
                    None => self.error(C::InvalidAlarm, &cp, "bitSet needs an integer value"),
                },
                AlarmCondition::IsTrue {} | AlarmCondition::IsFalse {} => {
                    if dt != DataType::Bool {
                        self.error(C::InvalidAlarm, &cp, "isTrue/isFalse need a bool value");
                    }
                }
            }
            if let Some(db) = a.deadband {
                let applies = matches!(
                    a.condition,
                    AlarmCondition::High { .. } | AlarmCondition::Low { .. }
                );
                if !applies {
                    self.error(
                        C::InvalidAlarm,
                        at(&path, "deadband"),
                        "deadband only applies to high/low conditions",
                    );
                } else if !finite_non_negative(db) {
                    self.error(
                        C::InvalidAlarm,
                        at(&path, "deadband"),
                        "deadband must be >= 0",
                    );
                }
            }
        }
    }

    // --------------------------------------------------------------- recipes

    fn check_recipes(&mut self) {
        for (i, r) in self.p.recipes.iter().enumerate() {
            let path = format!("/recipes/{i}");
            self.check_text(&r.name, &at(&path, "name"));
            if r.fields.is_empty() {
                self.error(
                    C::InvalidRecipe,
                    at(&path, "fields"),
                    "recipe has no fields",
                );
            }
            let mut field_types: BTreeMap<&str, Option<DataType>> = BTreeMap::new();
            for (j, f) in r.fields.iter().enumerate() {
                let fp = format!("{path}/fields/{j}");
                self.check_text(&f.name, &at(&fp, "name"));
                let dt = match self.ix.tags.get(f.tag.as_str()).copied() {
                    None => {
                        self.error(
                            C::UnknownReference,
                            at(&fp, "tag"),
                            format!("unknown tag {:?}", f.tag),
                        );
                        None
                    }
                    Some(tag) => {
                        if !tag.access.writable() {
                            self.error(
                                C::InvalidRecipe,
                                at(&fp, "tag"),
                                format!("recipe tag {:?} is not writable", f.tag),
                            );
                        }
                        match self.resolve_member(&tag.data_type, f.member.as_deref()) {
                            Ok(dt) => Some(dt),
                            Err(e) => {
                                self.error(C::InvalidDataType, at(&fp, "member"), e);
                                None
                            }
                        }
                    }
                };
                field_types.insert(f.id.as_str(), dt);
            }
            for (j, s) in r.sets.iter().enumerate() {
                let sp = format!("{path}/sets/{j}");
                self.check_text(&s.name, &at(&sp, "name"));
                for (field, value) in &s.values {
                    let vp = at(&at(&sp, "values"), field);
                    match field_types.get(field.as_str()) {
                        None => self.error(
                            C::InvalidRecipe,
                            vp,
                            format!("{field:?} is not a field of this recipe"),
                        ),
                        Some(Some(dt)) => {
                            let dt = dt.clone();
                            if let Err(e) = self.value_matches_data_type(value, &dt) {
                                self.error(C::InvalidRecipe, vp, e);
                            }
                        }
                        Some(None) => {}
                    }
                }
                for f in &r.fields {
                    if !s.values.contains_key(&f.id) {
                        self.warning(
                            C::InvalidRecipe,
                            at(&sp, "values"),
                            format!("no value for field {:?}; the tag keeps its value", f.id),
                        );
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------ navigation

    fn check_navigation(&mut self) {
        let start = &self.p.navigation.start_screen;
        self.check_screen_ref(
            start,
            ScreenKind::Page,
            "/navigation/startScreen",
            C::InvalidReference,
        );
        for (i, item) in self.p.navigation.items.iter().enumerate() {
            let path = format!("/navigation/items/{i}");
            self.check_text(&item.label, &at(&path, "label"));
            self.check_screen_ref(
                &item.screen,
                ScreenKind::Page,
                &at(&path, "screen"),
                C::InvalidReference,
            );
            if let Some(icon) = &item.icon {
                self.check_resource_ref(
                    icon,
                    &[ResourceKind::Image, ResourceKind::Vector],
                    &at(&path, "icon"),
                );
            }
        }
    }

    /// Checks a screen reference; `kind_code` is used for kind mismatches.
    pub fn check_screen_ref(&mut self, id: &ScreenId, kind: ScreenKind, path: &str, kind_code: C) {
        match self.ix.screens.get(id.as_str()) {
            None => self.error(C::UnknownReference, path, format!("unknown screen {id:?}")),
            Some(s) if s.kind != kind => self.error(
                kind_code,
                path,
                format!("screen {id:?} is a {:?}, expected a {:?}", s.kind, kind),
            ),
            Some(_) => {}
        }
    }

    pub fn check_resource_ref(&mut self, id: &ResourceId, kinds: &[ResourceKind], path: &str) {
        match self.ix.resources.get(id.as_str()) {
            None => self.error(
                C::UnknownReference,
                path,
                format!("unknown resource {id:?}"),
            ),
            Some(r) if !kinds.is_empty() && !kinds.contains(&r.kind) => self.error(
                C::InvalidReference,
                path,
                format!("resource {id:?} is {:?}, expected one of {kinds:?}", r.kind),
            ),
            Some(_) => {}
        }
    }

    // ---------------------------------------------------- definitions/screens

    fn check_definitions(&mut self) {
        let common: BTreeSet<&str> = crate::catalog::common_properties()
            .iter()
            .map(|s| s.name)
            .collect();
        for (i, d) in self.p.components.iter().enumerate() {
            let path = format!("/components/{i}");
            if d.name.trim().is_empty() {
                self.error(
                    C::InvalidValue,
                    at(&path, "name"),
                    "definition name is empty",
                );
            }
            let mut names = BTreeSet::new();
            for (j, param) in d.parameters.iter().enumerate() {
                let pp = format!("{path}/parameters/{j}");
                if !is_identifier_name(&param.name) {
                    self.error(
                        C::InvalidParameter,
                        at(&pp, "name"),
                        format!("invalid parameter name {:?}", param.name),
                    );
                }
                if !names.insert(param.name.as_str()) {
                    self.error(
                        C::InvalidParameter,
                        at(&pp, "name"),
                        format!("duplicate parameter {:?}", param.name),
                    );
                }
                if common.contains(param.name.as_str()) {
                    self.error(
                        C::InvalidParameter,
                        at(&pp, "name"),
                        format!("parameter {:?} shadows a common node property", param.name),
                    );
                }
                if let PropertyType::Tag {
                    data_type: Some(dt),
                    ..
                } = &param.ty
                {
                    self.check_data_type(dt, &at(&pp, "type"));
                }
                self.check_property_type_decl(&param.ty, &at(&pp, "type"));
                if let Some(default) = &param.default {
                    if matches!(param.ty, PropertyType::Tag { .. }) {
                        self.error(
                            C::InvalidParameter,
                            at(&pp, "default"),
                            "tag parameters cannot have a default; wire them per instance",
                        );
                    } else {
                        self.check_value(default, &param.ty, &at(&pp, "default"));
                    }
                }
            }
            self.check_tree(&d.root, &at(&path, "root"), Some(d));
        }
    }

    fn check_property_type_decl(&mut self, ty: &PropertyType, path: &str) {
        match ty {
            PropertyType::Number {
                min: Some(a),
                max: Some(b),
            } if !a.is_finite() || !b.is_finite() || a > b => {
                self.error(C::InvalidParameter, path, "min must be <= max");
            }
            PropertyType::Integer {
                min: Some(a),
                max: Some(b),
            } if a > b => {
                self.error(C::InvalidParameter, path, "min must be <= max");
            }
            PropertyType::Enum { options } if options.is_empty() => {
                self.error(C::InvalidParameter, path, "enum needs at least one option");
            }
            _ => {}
        }
    }

    fn check_screens(&mut self) {
        if self.p.screens.is_empty() {
            self.error(C::InvalidValue, "/screens", "project has no screens");
        }
        for (i, s) in self.p.screens.iter().enumerate() {
            let path = format!("/screens/{i}");
            if let Some(t) = &s.title {
                self.check_text(t, &at(&path, "title"));
            }
            if let Some(dp) = &s.design_profile {
                if !self.ix.device_profiles.contains(dp.as_str()) {
                    self.error(
                        C::UnknownReference,
                        at(&path, "designProfile"),
                        format!("unknown device profile {dp:?}"),
                    );
                }
            }
            for (j, h) in s.events.iter().enumerate() {
                let hp = format!("{path}/events/{j}");
                if !crate::catalog::SCREEN_EVENTS.contains(&h.event) {
                    self.error(
                        C::InvalidEvent,
                        at(&hp, "event"),
                        format!("screens raise only show/hide, not {:?}", h.event),
                    );
                }
                self.check_handler(h, &hp, None);
            }
            self.check_tree(&s.root, &at(&path, "root"), None);
        }
    }

    fn check_definition_cycles(&mut self) {
        let mut edges: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for d in &self.p.components {
            let mut out: Vec<&str> = Vec::new();
            d.root.walk(&mut |n| {
                if let Element::Instance { definition, .. } = &n.element {
                    if !out.contains(&definition.as_str()) {
                        out.push(definition.as_str());
                    }
                }
            });
            edges.entry(d.id.as_str()).or_insert(out);
        }
        let order: Vec<&str> = self.p.components.iter().map(|d| d.id.as_str()).collect();
        for cycle in find_cycles(&order, &edges) {
            let idx = self
                .p
                .components
                .iter()
                .position(|d| d.id.as_str() == cycle[0])
                .unwrap_or(0);
            self.error(
                C::CircularComponentReference,
                format!("/components/{idx}"),
                format!(
                    "component definitions instantiate each other: {}",
                    cycle.join(" -> ")
                ),
            );
        }
    }
}

fn collect_struct_refs<'a>(dt: &'a DataType, out: &mut Vec<&'a str>) {
    match dt {
        DataType::Struct(id) => {
            if !out.contains(&id.as_str()) {
                out.push(id.as_str());
            }
        }
        DataType::Array { element, .. } => collect_struct_refs(element, out),
        _ => {}
    }
}

/// Finds elementary cycles reachable in a directed graph, deterministically.
/// Each cycle is reported once, rotated so that it starts at the node that
/// comes first in `order`, and closed (`[a, b, a]`).
pub(super) fn find_cycles<'a>(
    order: &[&'a str],
    edges: &BTreeMap<&'a str, Vec<&'a str>>,
) -> Vec<Vec<&'a str>> {
    let rank: BTreeMap<&str, usize> = order.iter().enumerate().map(|(i, n)| (*n, i)).collect();
    let mut found: BTreeSet<Vec<&'a str>> = BTreeSet::new();
    let mut result = Vec::new();
    #[derive(Clone, Copy, PartialEq)]
    enum Mark {
        New,
        Active,
        Done,
    }
    let mut marks: BTreeMap<&str, Mark> = order.iter().map(|n| (*n, Mark::New)).collect();
    fn dfs<'a>(
        node: &'a str,
        edges: &BTreeMap<&'a str, Vec<&'a str>>,
        marks: &mut BTreeMap<&'a str, Mark>,
        stack: &mut Vec<&'a str>,
        rank: &BTreeMap<&str, usize>,
        found: &mut BTreeSet<Vec<&'a str>>,
        result: &mut Vec<Vec<&'a str>>,
    ) {
        marks.insert(node, Mark::Active);
        stack.push(node);
        for &next in edges.get(node).map(Vec::as_slice).unwrap_or(&[]) {
            match marks.get(next).copied() {
                None => {} // dangling reference, reported elsewhere
                Some(Mark::Active) => {
                    if let Some(pos) = stack.iter().position(|n| *n == next) {
                        let mut cycle: Vec<&str> = stack[pos..].to_vec();
                        // Rotate to the lowest-ranked member.
                        let min = (0..cycle.len())
                            .min_by_key(|i| rank.get(cycle[*i]).copied().unwrap_or(usize::MAX))
                            .unwrap_or(0);
                        cycle.rotate_left(min);
                        if found.insert(cycle.clone()) {
                            let mut closed = cycle.clone();
                            closed.push(cycle[0]);
                            result.push(closed);
                        }
                    }
                }
                Some(Mark::New) => dfs(next, edges, marks, stack, rank, found, result),
                Some(Mark::Done) => {}
            }
        }
        stack.pop();
        marks.insert(node, Mark::Done);
    }
    for &n in order {
        if marks.get(n).copied() == Some(Mark::New) {
            let mut stack = Vec::new();
            dfs(
                n,
                edges,
                &mut marks,
                &mut stack,
                &rank,
                &mut found,
                &mut result,
            );
        }
    }
    result
}
