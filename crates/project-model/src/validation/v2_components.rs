//! Validate staged component/action declarations against the same project
//! and binding/security contracts as the other candidate v2 layers.
use super::checks::{at, is_hex_color, Validator};
use super::{validate_v2_bindings, Diagnostic, DiagnosticCode as C, Severity, ValidationReport};
use crate::catalog;
use crate::id::validate_id;
use crate::model::v2::IndustrialSecurityDeclarations;
use crate::model::v2_bindings::ExpressionEnumerationDeclarations;
use crate::model::v2_components::*;
use crate::model::*;
use std::collections::{BTreeMap, BTreeSet};

fn error(r: &mut ValidationReport, c: C, p: impl Into<String>, m: impl Into<String>) {
    r.diagnostics.push(Diagnostic {
        severity: Severity::Error,
        code: c,
        path: p.into(),
        message: m.into(),
    });
}
fn point(p: Point) -> bool {
    p.x.is_finite() && p.y.is_finite()
}
fn edge(a: Point, b: Point) -> bool {
    point(a) && point(b) && a != b && (b.x - a.x).hypot(b.y - a.y).is_finite()
}
fn stroke(r: &mut ValidationReport, path: &str, color: &str, width: f64) {
    if !is_hex_color(color) {
        error(
            r,
            C::InvalidValue,
            format!("{path}/strokeColor"),
            "invalid hex stroke color",
        );
    }
    if !width.is_finite() || width <= 0.0 {
        error(
            r,
            C::InvalidValue,
            format!("{path}/strokeWidth"),
            "stroke width must be finite and positive",
        );
    }
}
fn check_label(v: &mut Validator<'_>, label: &Option<LocalizedText>, path: &str) {
    if let Some(l) = label {
        v.check_text(l, path);
    }
}
/// Candidate validation. The declarations are never executed, and all
/// unsupported or ambiguous staging combinations are refused.
pub fn validate_v2_components(
    p: &Project,
    industrial: &IndustrialSecurityDeclarations,
    bindings: &ExpressionEnumerationDeclarations,
    d: &ComponentActionDeclarations,
) -> ValidationReport {
    let mut r = validate_v2_bindings(p, industrial, bindings);
    if r.has_errors() {
        return r;
    }
    let mut validator = Validator::new(p);
    validator.build_index();
    let mut ids: BTreeSet<String> = validator
        .collect_ids()
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    ids.extend(
        industrial
            .security
            .permissions
            .iter()
            .map(|x| x.id.as_str().to_owned()),
    );
    ids.extend(
        industrial
            .security
            .roles
            .iter()
            .map(|x| x.id.as_str().to_owned()),
    );
    ids.extend(
        bindings
            .enumerations
            .iter()
            .map(|x| x.id.as_str().to_owned()),
    );
    let defs: BTreeMap<_, _> = p.components.iter().map(|c| (c.id.as_str(), c)).collect();
    let mut classifications = BTreeMap::new();
    for (i, c) in d.definitions.iter().enumerate() {
        let path = format!("/v2/definitions/{i}");
        if classifications
            .insert(c.definition.as_str(), &c.classification)
            .is_some()
        {
            error(
                &mut r,
                C::DuplicateIdentifier,
                format!("{path}/definition"),
                "duplicate definition classification",
            );
        }
        let Some(def) = defs.get(c.definition.as_str()) else {
            error(
                &mut r,
                C::UnknownReference,
                format!("{path}/definition"),
                "unknown reusable definition",
            );
            continue;
        };
        if let DefinitionKind::IndustrialSymbol { symbol } = c.classification {
            for (name, expected) in symbol.slots() {
                let valid =
                    def.parameters
                        .iter()
                        .find(|p| p.name == *name)
                        .is_some_and(|p| match &p.ty {
                            PropertyType::Tag {
                                data_type: Some(dt),
                                writable: false,
                            } => match expected {
                                SymbolSlotType::Boolean => *dt == DataType::Bool,
                                SymbolSlotType::Numeric => dt.is_numeric(),
                            },
                            _ => false,
                        });
                if !valid {
                    error(
                        &mut r,
                        C::InvalidParameter,
                        format!("{path}/classification"),
                        format!("symbol requires read-only typed tag parameter {name}"),
                    );
                }
            }
            // A symbol is a passive visualization, never a hidden operator action.
            def.root.walk(&mut |node| {
                if !node.events.is_empty()
                    || node.bindings.iter().any(|b| b.mode.writes())
                    || matches!(
                        node.element,
                        Element::Instance { .. }
                            | Element::Component {
                                kind: ComponentKind::TextInput | ComponentKind::DropDown
                            }
                    )
                {
                    error(
                        &mut r,
                        C::InvalidBinding,
                        format!("{path}/classification"),
                        "passive symbol cannot contain events, write bindings or nested instances",
                    );
                }
            });
        }
    }
    let tags: BTreeMap<_, _> = p.tags.iter().map(|t| (t.id.as_str(), t)).collect();
    let enums: BTreeMap<_, _> = bindings
        .enumerations
        .iter()
        .map(|e| (e.id.as_str(), e))
        .collect();
    let mut contract_ids = BTreeSet::new();
    let mut placed = BTreeMap::new();
    for root in p
        .screens
        .iter()
        .map(|s| &s.root)
        .chain(p.components.iter().map(|c| &c.root))
    {
        root.walk(&mut |n| {
            if let Element::Component { kind } = n.element {
                placed.insert(n.id.as_str(), kind);
            }
        });
    }
    for (i, c) in d.components.iter().enumerate() {
        let path = format!("/v2/components/{i}");
        if validate_id(c.id.as_str()).is_err() {
            error(
                &mut r,
                C::InvalidIdentifier,
                format!("{path}/id"),
                "invalid node identifier",
            );
        }
        if !contract_ids.insert(c.id.as_str())
            || (ids.contains(c.id.as_str())
                && placed.get(c.id.as_str()) != Some(&c.contract.kind()))
        {
            error(
                &mut r,
                C::DuplicateIdentifier,
                format!("{path}/id"),
                "identifier already used",
            );
        }
        let path = format!("{path}/contract");
        match &c.contract {
            ComponentContract::Line {
                start,
                end,
                stroke_color,
                stroke_width,
            }
            | ComponentContract::Arrow {
                start,
                end,
                stroke_color,
                stroke_width,
                ..
            } => {
                if !edge(*start, *end) {
                    error(
                        &mut r,
                        C::InvalidValue,
                        format!("{path}/end"),
                        "line endpoints must be finite and distinct",
                    );
                }
                stroke(&mut r, &path, stroke_color, *stroke_width);
                if let ComponentContract::Arrow { head_length, .. } = c.contract {
                    let length = (end.x - start.x).hypot(end.y - start.y);
                    if !length.is_finite()
                        || !head_length.is_finite()
                        || head_length <= 0.0
                        || head_length > length
                    {
                        error(
                            &mut r,
                            C::InvalidValue,
                            format!("{path}/headLength"),
                            "arrow head must be positive and no longer than segment",
                        );
                    }
                }
            }
            ComponentContract::Polygon {
                points,
                fill,
                stroke_color,
                stroke_width,
            } => {
                let valid = points.len() >= 3
                    && points.len() <= 1024
                    && points.iter().copied().all(point)
                    && points
                        .iter()
                        .zip(points.iter().cycle().skip(1))
                        .all(|(a, b)| a != b)
                    && points
                        .iter()
                        .enumerate()
                        .all(|(i, a)| !points[..i].contains(a));
                if !valid {
                    error(
                        &mut r,
                        C::InvalidValue,
                        format!("{path}/points"),
                        "polygon requires 3–1024 distinct finite points",
                    );
                } else {
                    let area = points
                        .iter()
                        .zip(points.iter().cycle().skip(1))
                        .map(|(a, b)| a.x * b.y - b.x * a.y)
                        .sum::<f64>();
                    if !area.is_finite() || area == 0.0 {
                        error(
                            &mut r,
                            C::InvalidValue,
                            format!("{path}/points"),
                            "polygon must have a finite nonzero signed area",
                        );
                    }
                }
                if !is_hex_color(fill) {
                    error(
                        &mut r,
                        C::InvalidValue,
                        format!("{path}/fill"),
                        "invalid hex fill",
                    );
                }
                stroke(&mut r, &path, stroke_color, *stroke_width);
            }
            ComponentContract::TextInput {
                tag,
                max_length,
                label,
            } => {
                match tags.get(tag.as_str()) {
                    None => error(
                        &mut r,
                        C::UnknownReference,
                        format!("{path}/tag"),
                        "unknown input tag",
                    ),
                    Some(t)
                        if t.data_type != DataType::String
                            || !t.access.readable()
                            || !t.access.writable() =>
                    {
                        error(
                            &mut r,
                            C::InvalidBinding,
                            format!("{path}/tag"),
                            "text input requires a readable and writable string tag",
                        )
                    }
                    _ => {}
                }
                if *max_length == 0 || *max_length > 65536 {
                    error(
                        &mut r,
                        C::InvalidValue,
                        format!("{path}/maxLength"),
                        "text input limit must be 1–65536",
                    );
                }
                check_label(&mut validator, label, &format!("{path}/label"));
            }
            ComponentContract::DropDown {
                tag,
                enumeration,
                label,
            } => {
                let t = tags.get(tag.as_str());
                let e = enums.get(enumeration.as_str());
                if t.is_none() {
                    error(
                        &mut r,
                        C::UnknownReference,
                        format!("{path}/tag"),
                        "unknown input tag",
                    );
                }
                if e.is_none() {
                    error(
                        &mut r,
                        C::UnknownReference,
                        format!("{path}/enumeration"),
                        "unknown enumeration",
                    );
                }
                if let (Some(t), Some(e)) = (t, e) {
                    if t.data_type != e.data_type || !t.access.readable() || !t.access.writable() {
                        error(
                            &mut r,
                            C::InvalidBinding,
                            format!("{path}/tag"),
                            "dropdown requires exact enumeration type and readable/writable tag",
                        );
                    }
                }
                check_label(&mut validator, label, &format!("{path}/label"));
            }
        }
    }
    let screens: BTreeMap<_, _> = p.screens.iter().map(|s| (s.id.as_str(), s)).collect();
    let mut nodes: BTreeMap<&str, (&Node, Option<&ComponentDefinition>)> = BTreeMap::new();
    for s in &p.screens {
        s.root.walk(&mut |n| {
            nodes.insert(n.id.as_str(), (n, None));
        });
    }
    for def in &p.components {
        def.root.walk(&mut |n| {
            nodes.insert(n.id.as_str(), (n, Some(def)));
        });
    }
    let mut handlers = BTreeSet::new();
    for (i, h) in d.handlers.iter().enumerate() {
        let path = format!("/v2/handlers/{i}");
        let (id, existing, allowed, context) = match &h.origin {
            EventOrigin::Screen(id) => match screens.get(id.as_str()) {
                Some(s) => (id.as_str(), &s.events, catalog::SCREEN_EVENTS, None),
                None => {
                    error(
                        &mut r,
                        C::UnknownReference,
                        format!("{path}/origin"),
                        "unknown screen",
                    );
                    continue;
                }
            },
            EventOrigin::Node(id) => match nodes.get(id.as_str()) {
                Some((n, context)) => {
                    let kind = match n.element {
                        Element::Component { kind } => Some(kind),
                        _ => None,
                    };
                    (id.as_str(), &n.events, catalog::events(kind), *context)
                }
                None => {
                    error(
                        &mut r,
                        C::UnknownReference,
                        format!("{path}/origin"),
                        "unknown node",
                    );
                    continue;
                }
            },
        };
        if context.is_some_and(|def| {
            matches!(
                classifications.get(def.id.as_str()),
                Some(DefinitionKind::IndustrialSymbol { .. })
            )
        }) {
            error(
                &mut r,
                C::InvalidEvent,
                format!("{path}/origin"),
                "passive symbol cannot contain a staged action handler",
            );
        }
        if !handlers.insert((id, h.event)) || existing.iter().any(|e| e.event == h.event) {
            error(
                &mut r,
                C::InvalidEvent,
                format!("{path}/event"),
                "duplicate event; merge into one ordered handler during integration",
            );
        }
        if !allowed.contains(&h.event) {
            error(
                &mut r,
                C::InvalidEvent,
                format!("{path}/event"),
                "event not supported by origin",
            );
        }
        if h.actions.is_empty() || h.actions.len() > 8 {
            error(
                &mut r,
                C::InvalidEvent,
                format!("{path}/actions"),
                "candidate handler requires 1–8 actions",
            );
        }
        check_label(&mut validator, &h.confirm, &format!("{path}/confirm"));
        for (j, a) in h.actions.iter().enumerate() {
            let path = format!("{path}/actions/{j}");
            match a {
                CandidateAction::Existing(a) => validator.check_action(a, &path, context),
                CandidateAction::New(NewAction::NavigateStartup {}) => {}
                CandidateAction::New(NewAction::OpenFaceplate {
                    definition,
                    parameters,
                }) => {
                    let Some(def) = defs.get(definition.as_str()) else {
                        error(
                            &mut r,
                            C::UnknownReference,
                            format!("{path}/definition"),
                            "unknown faceplate definition",
                        );
                        continue;
                    };
                    if !matches!(
                        classifications.get(definition.as_str()),
                        Some(DefinitionKind::Faceplate {})
                    ) {
                        error(
                            &mut r,
                            C::InvalidAction,
                            format!("{path}/definition"),
                            "target not classified as faceplate",
                        );
                    }
                    for name in parameters.keys() {
                        if !def.parameters.iter().any(|p| p.name == *name) {
                            error(
                                &mut r,
                                C::InvalidParameter,
                                at(&format!("{path}/parameters"), name),
                                "unknown faceplate argument",
                            );
                        }
                    }
                    for param in &def.parameters {
                        let pp = at(&format!("{path}/parameters"), &param.name);
                        match parameters.get(&param.name).or(param.default.as_ref()) {
                            None => error(
                                &mut r,
                                C::InvalidParameter,
                                pp,
                                "required faceplate argument missing",
                            ),
                            Some(value) => {
                                validator.check_value(value, &param.ty, &pp);
                                if matches!(param.ty, PropertyType::Tag { .. })
                                    && value
                                        .as_str()
                                        .and_then(|id| tags.get(id))
                                        .is_some_and(|t| !t.access.readable())
                                {
                                    error(
                                        &mut r,
                                        C::InvalidParameter,
                                        pp,
                                        "faceplate tag argument must be readable",
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    r.diagnostics.extend(validator.into_diagnostics());
    r
}
