//! Semantic validation for staged industrial/security declarations.
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use super::{validate_base, Diagnostic, DiagnosticCode as C, Severity, ValidationReport};
use crate::id::validate_id;
use crate::model::v2::*;
use crate::model::{DataType, DriverKind, Project};

fn error(report: &mut ValidationReport, code: C, path: String, message: &str) {
    report.diagnostics.push(Diagnostic {
        severity: Severity::Error,
        code,
        path,
        message: message.into(),
    });
}

fn register_id(ids: &mut BTreeSet<String>, id: &str, path: String, report: &mut ValidationReport) {
    if validate_id(id).is_err() {
        error(
            report,
            C::InvalidIdentifier,
            path.clone(),
            "invalid stable identifier",
        );
    }
    if !ids.insert(id.to_owned()) {
        error(
            report,
            C::DuplicateIdentifier,
            path,
            "identifier already used in project",
        );
    }
}

fn unique<'a>(
    ids: &mut BTreeSet<&'a str>,
    id: &'a str,
    path: String,
    report: &mut ValidationReport,
) {
    if !ids.insert(id) {
        error(
            report,
            C::DuplicateIdentifier,
            path,
            "duplicate declaration or reference",
        );
    }
}

fn known(ids: &BTreeSet<&str>, id: &str, path: String, report: &mut ValidationReport) {
    if !ids.contains(id) {
        error(
            report,
            C::UnknownReference,
            path,
            "reference does not resolve to the required kind",
        );
    }
}

#[derive(Clone, Copy, Debug)]
enum Numeric {
    Integer(i128),
    Float(f64),
}
impl Numeric {
    fn cmp(self, other: Self) -> Option<Ordering> {
        match (self, other) {
            (Self::Integer(a), Self::Integer(b)) => Some(a.cmp(&b)),
            (Self::Float(a), Self::Float(b)) => a.partial_cmp(&b),
            _ => None,
        }
    }
    fn positive(self) -> bool {
        match self {
            Self::Integer(n) => n > 0,
            Self::Float(n) => n > 0.0,
        }
    }
}

fn number(value: &serde_json::Number, dt: &DataType) -> Option<Numeric> {
    let int = |min: i128, max: i128| {
        let n = value
            .as_i64()
            .map(i128::from)
            .or_else(|| value.as_u64().map(i128::from))?;
        (min..=max).contains(&n).then_some(Numeric::Integer(n))
    };
    match dt {
        DataType::Int8 => int(i8::MIN.into(), i8::MAX.into()),
        DataType::Int16 => int(i16::MIN.into(), i16::MAX.into()),
        DataType::Int32 => int(i32::MIN.into(), i32::MAX.into()),
        DataType::Int64 => int(i64::MIN.into(), i64::MAX.into()),
        DataType::UInt8 => int(0, u8::MAX.into()),
        DataType::UInt16 => int(0, u16::MAX.into()),
        DataType::UInt32 => int(0, u32::MAX.into()),
        DataType::UInt64 => int(0, u64::MAX.into()),
        DataType::Float32 | DataType::Float64 => {
            let n = value.as_f64()?;
            // Do not silently round integer literals when a float limit is declared.
            if value.as_i64().is_some_and(|v| n as i128 != i128::from(v))
                || value.as_u64().is_some_and(|v| n as i128 != i128::from(v))
            {
                return None;
            }
            // Conservative candidate: no implicit narrowing of f32 safety limits.
            (n.is_finite() && (*dt != DataType::Float32 || f64::from(n as f32) == n))
                .then_some(Numeric::Float(n))
        }
        _ => None,
    }
}

fn check_write(
    policy: &TagWritePolicy,
    tag: &crate::model::Tag,
    path: &str,
    report: &mut ValidationReport,
) {
    if !tag.access.writable() {
        error(
            report,
            C::InvalidTag,
            format!("{path}/tag"),
            "write policy requires a writable tag",
        );
    }
    if policy.critical && !policy.confirmation_required {
        error(
            report,
            C::InvalidTag,
            format!("{path}/confirmationRequired"),
            "critical command requires confirmation",
        );
    }
    match policy.command {
        CommandKind::Momentary | CommandKind::Maintained if tag.data_type != DataType::Bool => {
            error(
                report,
                C::InvalidTag,
                format!("{path}/command"),
                "binary command requires a boolean tag",
            );
        }
        CommandKind::Setpoint if !tag.data_type.is_numeric() => {
            error(
                report,
                C::InvalidTag,
                format!("{path}/command"),
                "setpoint requires a numeric scalar",
            );
        }
        _ => {}
    }
    if (policy.command == CommandKind::Momentary && !policy.pulse_ms.is_some_and(|v| v > 0))
        || (policy.command != CommandKind::Momentary && policy.pulse_ms.is_some())
    {
        error(
            report,
            C::InvalidTag,
            format!("{path}/pulseMs"),
            "positive pulse duration is required only for momentary commands",
        );
    }
    if policy.command != CommandKind::Setpoint && policy.limits != WriteLimits::default() {
        error(
            report,
            C::InvalidTag,
            format!("{path}/limits"),
            "numeric limits apply only to setpoints",
        );
    }
    let limits = &policy.limits;
    let mut parsed = [None; 3];
    for (i, (key, val)) in [
        ("min", &limits.min),
        ("max", &limits.max),
        ("step", &limits.step),
    ]
    .into_iter()
    .enumerate()
    {
        if let Some(val) = val {
            parsed[i] = number(val, &tag.data_type);
            if parsed[i].is_none() {
                error(
                    report,
                    C::InvalidTag,
                    format!("{path}/limits/{key}"),
                    "limit is not exactly representable in tag type",
                );
            }
        }
    }
    if let (Some(min), Some(max)) = (parsed[0], parsed[1]) {
        if min.cmp(max) == Some(Ordering::Greater) {
            error(
                report,
                C::InvalidTag,
                format!("{path}/limits/max"),
                "maximum precedes minimum",
            );
        }
    }
    if parsed[2].is_some_and(|n| !n.positive()) {
        error(
            report,
            C::InvalidTag,
            format!("{path}/limits/step"),
            "step must be positive",
        );
    }
}

/// Validate the active base project first, then declarations. Every finding
/// is an error; no effective permissions or write authorization are produced.
/// Staging paths start at /v2 and are not part of the active v1 schema.
pub fn validate_v2_declarations(
    project: &Project,
    v2: &IndustrialSecurityDeclarations,
) -> ValidationReport {
    let mut report = validate_base(project);
    if report.has_errors() {
        return report;
    }

    let mut ids: BTreeSet<String> = super::checks::Validator::new(project)
        .collect_ids()
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    let mut seen = BTreeSet::new();
    let connections: BTreeMap<_, _> = project
        .connections
        .iter()
        .map(|c| (c.id.as_str(), c))
        .collect();
    for (i, policy) in v2.connections.iter().enumerate() {
        let path = format!("/v2/connections/{i}");
        unique(
            &mut seen,
            policy.connection.as_str(),
            format!("{path}/connection"),
            &mut report,
        );
        match connections.get(policy.connection.as_str()) {
            None => error(
                &mut report,
                C::UnknownReference,
                format!("{path}/connection"),
                "unknown connection",
            ),
            Some(c) => {
                if policy.driver == DriverKindV2::Loopback {
                    if c.driver != DriverKind::Simulator
                        || c.endpoint.is_some()
                        || c.credential_ref.is_some()
                        || !c.parameters.is_empty()
                    {
                        error(&mut report, C::InvalidValue, format!("{path}/driver"), "loopback staging requires an endpoint-free, credential-free simulator without driver parameters");
                    }
                } else if policy.driver != DriverKindV2::from(c.driver) {
                    error(
                        &mut report,
                        C::InvalidValue,
                        format!("{path}/driver"),
                        "driver conflicts with base connection",
                    );
                }
                if policy.commissioning_allowed
                    && matches!(
                        policy.driver,
                        DriverKindV2::Simulator | DriverKindV2::Loopback
                    )
                {
                    error(
                        &mut report,
                        C::InvalidValue,
                        format!("{path}/commissioningAllowed"),
                        "test drivers cannot declare physical commissioning",
                    );
                }
            }
        }
    }
    seen.clear();
    let tags: BTreeMap<_, _> = project.tags.iter().map(|t| (t.id.as_str(), t)).collect();
    for (i, policy) in v2.tags.iter().enumerate() {
        let path = format!("/v2/tags/{i}");
        unique(
            &mut seen,
            policy.tag.as_str(),
            format!("{path}/tag"),
            &mut report,
        );
        match tags.get(policy.tag.as_str()) {
            Some(tag) => check_write(policy, tag, &path, &mut report),
            None => error(
                &mut report,
                C::UnknownReference,
                format!("{path}/tag"),
                "unknown tag",
            ),
        }
    }

    let mut permissions = BTreeSet::new();
    for (i, p) in v2.security.permissions.iter().enumerate() {
        let path = format!("/v2/security/permissions/{i}");
        register_id(&mut ids, p.id.as_str(), format!("{path}/id"), &mut report);
        permissions.insert(p.id.as_str());
        if p.name.trim().is_empty() {
            error(
                &mut report,
                C::InvalidValue,
                format!("{path}/name"),
                "permission name is empty",
            );
        }
    }
    let mut roles = BTreeSet::new();
    for (i, role) in v2.security.roles.iter().enumerate() {
        let path = format!("/v2/security/roles/{i}");
        register_id(
            &mut ids,
            role.id.as_str(),
            format!("{path}/id"),
            &mut report,
        );
        roles.insert(role.id.as_str());
        if role.name.trim().is_empty() {
            error(
                &mut report,
                C::InvalidValue,
                format!("{path}/name"),
                "role name is empty",
            );
        }
        seen.clear();
        for (j, p) in role.permissions.iter().enumerate() {
            let path = format!("{path}/permissions/{j}");
            unique(&mut seen, p.as_str(), path.clone(), &mut report);
            known(&permissions, p.as_str(), path, &mut report);
        }
    }
    if let Some(role) = &v2.security.anonymous_role {
        known(
            &roles,
            role.as_str(),
            "/v2/security/anonymousRole".into(),
            &mut report,
        );
    }
    if v2.security.auto_logout_seconds == Some(0) {
        error(
            &mut report,
            C::InvalidValue,
            "/v2/security/autoLogoutSeconds".into(),
            "configured auto logout must be positive",
        );
    }
    let screens: BTreeSet<_> = project.screens.iter().map(|s| s.id.as_str()).collect();
    seen.clear();
    for (i, s) in v2.screens.iter().enumerate() {
        let path = format!("/v2/screens/{i}");
        unique(
            &mut seen,
            s.screen.as_str(),
            format!("{path}/screen"),
            &mut report,
        );
        known(
            &screens,
            s.screen.as_str(),
            format!("{path}/screen"),
            &mut report,
        );
        known(
            &permissions,
            s.required_permission.as_str(),
            format!("{path}/requiredPermission"),
            &mut report,
        );
    }
    let mut nodes = BTreeSet::new();
    for root in project
        .screens
        .iter()
        .map(|s| &s.root)
        .chain(project.components.iter().map(|c| &c.root))
    {
        root.walk(&mut |n| {
            nodes.insert(n.id.as_str());
        });
    }
    seen.clear();
    for (i, n) in v2.nodes.iter().enumerate() {
        let path = format!("/v2/nodes/{i}");
        unique(
            &mut seen,
            n.node.as_str(),
            format!("{path}/node"),
            &mut report,
        );
        known(&nodes, n.node.as_str(), format!("{path}/node"), &mut report);
        known(
            &permissions,
            n.required_permission.as_str(),
            format!("{path}/requiredPermission"),
            &mut report,
        );
    }
    report
}
