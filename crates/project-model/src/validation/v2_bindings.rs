//! Validate the candidate expression/enumeration layer against base project
//! and staged security/industrial declarations before future v2 integration.
use super::checks::{is_hex_color, Validator};
use super::{
    validate_v2_declarations, Diagnostic, DiagnosticCode as C, Severity, ValidationReport,
};
use crate::catalog::{self, Bindability};
use crate::expression::{self, Type};
use crate::id::validate_id;
use crate::model::v2::IndustrialSecurityDeclarations;
use crate::model::v2_bindings::*;
use crate::model::{DataType, Element, LocalizedText, Node, Project, PropertyType};
use std::collections::{BTreeMap, BTreeSet};

fn error(r: &mut ValidationReport, c: C, p: String, m: impl Into<String>) {
    r.diagnostics.push(Diagnostic {
        severity: Severity::Error,
        code: c,
        path: p,
        message: m.into(),
    });
}
fn enum_type(dt: &DataType) -> bool {
    dt.is_integer() || matches!(dt, DataType::Bool | DataType::String)
}
fn expression_type(dt: &DataType) -> Option<Type> {
    match dt {
        DataType::Bool => Some(Type::Bool),
        DataType::String => Some(Type::Text),
        d if d.is_numeric() => Some(Type::Number),
        _ => None,
    }
}
fn label_valid(p: &Project, l: &LocalizedText) -> bool {
    match l {
        LocalizedText::Literal(s) => !s.trim().is_empty(),
        LocalizedText::Key(k) => p
            .localization
            .strings
            .get(&k.text_key)
            .and_then(|v| v.get(&p.localization.default_locale))
            .is_some_and(|s| !s.trim().is_empty()),
    }
}
fn feeds(t: Type, p: &PropertyType) -> bool {
    matches!(
        (t, p),
        (Type::Bool, PropertyType::Bool {})
            | (Type::Number, PropertyType::Number { .. })
            | (Type::Text, PropertyType::String {} | PropertyType::Text {})
    )
}
/// Validate staged definitions and bindings. Integer enumerations deliberately
/// use exact JSON integer values rather than a float conversion.
pub fn validate_v2_bindings(
    p: &Project,
    industrial: &IndustrialSecurityDeclarations,
    d: &ExpressionEnumerationDeclarations,
) -> ValidationReport {
    let mut r = validate_v2_declarations(p, industrial);
    if r.has_errors() {
        return r;
    }
    let validator = Validator::new(p);
    let mut ids: BTreeSet<_> = validator
        .collect_ids()
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    ids.extend(
        industrial
            .security
            .permissions
            .iter()
            .map(|v| v.id.as_str().to_owned()),
    );
    ids.extend(
        industrial
            .security
            .roles
            .iter()
            .map(|v| v.id.as_str().to_owned()),
    );
    let mut enums = BTreeMap::new();
    for (i, e) in d.enumerations.iter().enumerate() {
        let path = format!("/v2/enumerations/{i}");
        if validate_id(e.id.as_str()).is_err() {
            error(
                &mut r,
                C::InvalidIdentifier,
                format!("{path}/id"),
                "invalid enumeration identifier",
            );
        }
        if !ids.insert(e.id.as_str().to_owned()) {
            error(
                &mut r,
                C::DuplicateIdentifier,
                format!("{path}/id"),
                "identifier already in use",
            );
        }
        enums.insert(e.id.as_str(), e);
        if e.name.trim().is_empty() {
            error(
                &mut r,
                C::InvalidValue,
                format!("{path}/name"),
                "name is empty",
            );
        }
        if !enum_type(&e.data_type) {
            error(
                &mut r,
                C::InvalidDataType,
                format!("{path}/dataType"),
                "enumerations require discrete scalar types",
            );
        }
        if e.entries.is_empty() {
            error(
                &mut r,
                C::InvalidValue,
                format!("{path}/entries"),
                "enumeration must have values",
            );
        }
        let mut values = BTreeSet::new();
        for (j, v) in e.entries.iter().enumerate() {
            let path = format!("{path}/entries/{j}");
            if enum_type(&e.data_type) {
                if let Err(m) = validator.value_matches_data_type(&v.value, &e.data_type) {
                    error(&mut r, C::InvalidValue, format!("{path}/value"), m);
                }
            }
            if !values.insert(v.value.to_string()) {
                error(
                    &mut r,
                    C::InvalidValue,
                    format!("{path}/value"),
                    "duplicate enumeration value",
                );
            }
            if !label_valid(p, &v.label) {
                error(
                    &mut r,
                    C::InvalidValue,
                    format!("{path}/label"),
                    "missing or empty default localized label",
                );
            }
            if v.color.as_ref().is_some_and(|s| !is_hex_color(s)) {
                error(
                    &mut r,
                    C::InvalidValue,
                    format!("{path}/color"),
                    "invalid hex color",
                );
            }
        }
    }
    let tags: BTreeMap<_, _> = p.tags.iter().map(|t| (t.id.as_str(), t)).collect();
    let mut mapped = BTreeSet::new();
    for (i, m) in d.mappings.iter().enumerate() {
        let path = format!("/v2/mappings/{i}");
        if !mapped.insert(m.tag.as_str()) {
            error(
                &mut r,
                C::InvalidValue,
                format!("{path}/tag"),
                "tag already has an enumeration mapping",
            );
        }
        let tag = tags.get(m.tag.as_str());
        let e = enums.get(m.enumeration.as_str());
        if tag.is_none() {
            error(
                &mut r,
                C::UnknownReference,
                format!("{path}/tag"),
                "unknown tag",
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
        if let (Some(t), Some(e)) = (tag, e) {
            if t.data_type != e.data_type || !t.access.readable() {
                error(
                    &mut r,
                    C::InvalidBinding,
                    format!("{path}/tag"),
                    "enumeration requires a readable tag of the exact declared type",
                );
            }
        }
    }
    let types: BTreeMap<String, Type> = p
        .tags
        .iter()
        .filter(|t| t.access.readable())
        .filter_map(|t| expression_type(&t.data_type).map(|ty| (t.id.as_str().to_owned(), ty)))
        .collect();
    let mut nodes: BTreeMap<&str, &Node> = BTreeMap::new();
    for root in p
        .screens
        .iter()
        .map(|s| &s.root)
        .chain(p.components.iter().map(|c| &c.root))
    {
        root.walk(&mut |n| {
            nodes.insert(n.id.as_str(), n);
        });
    }
    let mut bound = BTreeSet::new();
    for (i, b) in d.expressions.iter().enumerate() {
        let path = format!("/v2/expressions/{i}");
        if !bound.insert((b.node.as_str(), b.property.as_str())) {
            error(
                &mut r,
                C::DuplicateBinding,
                format!("{path}/property"),
                "duplicate expression target",
            );
        }
        let Some(n) = nodes.get(b.node.as_str()) else {
            error(
                &mut r,
                C::UnknownReference,
                format!("{path}/node"),
                "unknown node",
            );
            continue;
        };
        if n.bindings.iter().any(|old| old.property == b.property) {
            error(
                &mut r,
                C::DuplicateBinding,
                format!("{path}/property"),
                "target already has a direct binding",
            );
        }
        let kind = match n.element {
            Element::Component { kind } => Some(kind),
            _ => None,
        };
        // Reusable-instance arguments have a distinct parameter contract;
        // defer them rather than silently treating them as catalog properties.
        let target = if matches!(n.element, Element::Instance { .. }) {
            None
        } else {
            catalog::find_property(kind, &b.property)
        };
        if target
            .as_ref()
            .is_none_or(|s| s.bindable != Bindability::Read)
        {
            error(
                &mut r,
                C::InvalidBinding,
                format!("{path}/property"),
                "expression requires a read-only bindable catalog property",
            );
        }
        if b.contract_version != expression::CONTRACT_VERSION {
            error(
                &mut r,
                C::InvalidBinding,
                format!("{path}/contractVersion"),
                "unsupported expression contract",
            );
            continue;
        }
        match expression::compile(&b.expression, &types) {
            Ok(expr) => {
                if target
                    .as_ref()
                    .is_some_and(|s| !feeds(expr.result_type(), &s.ty))
                {
                    error(
                        &mut r,
                        C::InvalidBinding,
                        format!("{path}/expression"),
                        "expression result incompatible with target property",
                    );
                }
            }
            Err(problems) => {
                for problem in problems {
                    error(
                        &mut r,
                        C::InvalidBinding,
                        format!("{path}/expression"),
                        format!("column {}: {}", problem.position + 1, problem.message),
                    );
                }
            }
        }
    }
    r
}
