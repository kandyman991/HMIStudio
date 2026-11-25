//! Conservative, declaration-only industrial mapping; no active credentials or I/O.
use super::{path, Mapping};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

fn field(m: &mut Mapping, p: &str, k: &str, t: &str, target: &str) {
    m.mark(&path(p, k), &path(t, target));
}
fn flag(x: &Value, k: &str) -> Option<bool> {
    match x.get(k) {
        None => Some(false),
        Some(v) => v.as_bool(),
    }
}
fn fail<T>(m: &mut Mapping, p: &str, code: &'static str) -> Option<T> {
    m.fail(p, code);
    None
}
fn map_obj<'a>(
    v: &'a Value,
    k: &str,
    m: &mut Mapping,
    t: &str,
) -> Option<Option<&'a serde_json::Map<String, Value>>> {
    match v.get(k) {
        None => Some(None),
        Some(x) => {
            let o = x.as_object()?;
            if o.is_empty() {
                m.mark(&path("", k), t);
            }
            Some(Some(o))
        }
    }
}
fn host(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 253
        && s.split('.').all(|p| {
            !p.is_empty()
                && p.len() <= 63
                && p.as_bytes().first().is_some_and(u8::is_ascii_alphanumeric)
                && p.as_bytes().last().is_some_and(u8::is_ascii_alphanumeric)
                && p.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
}
fn uint(s: &str, min: u64, max: u64) -> Option<u64> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n = s.parse().ok()?;
    (min..=max).contains(&n).then_some(n)
}
fn endpoint(s: &str) -> bool {
    // Only a plain DNS/IPv4 host and explicit port; no userinfo, URL query or path.
    let Some((h, p)) = s
        .strip_prefix("opc.tcp://")
        .and_then(|s| s.rsplit_once(':'))
    else {
        return false;
    };
    host(h) && uint(p, 1, 65535).is_some()
}
fn numeric_limit(x: &Value, ty: &str) -> Option<Value> {
    // Legacy limits are binary64. Reject source integers that would already
    // round during legacy decoding, and reject all narrowing on conversion.
    let n = x.as_f64()?;
    if !n.is_finite()
        || x.as_i64().is_some_and(|i| n as i128 != i128::from(i))
        || x.as_u64().is_some_and(|i| n as i128 != i128::from(i))
    {
        return None;
    }
    match ty {
        "int32" if n.fract() == 0. && n >= i32::MIN as f64 && n <= i32::MAX as f64 => {
            Some(json!(n as i32))
        }
        "int64"
            if n.fract() == 0. && (-9223372036854775808.0..9223372036854775808.0).contains(&n) =>
        {
            Some(json!(n as i64))
        }
        "float32" if f64::from(n as f32) == n => Some(json!(n)),
        "float64" => Some(json!(n)),
        _ => None,
    }
}

pub(super) fn map(v: &Value, doc: &mut Value, m: &mut Mapping) -> Option<()> {
    let ver = v["schemaVersion"].as_u64()?;
    let mut connections = Vec::new();
    let mut policies = Vec::new();
    let mut source_ids = BTreeMap::new();
    if let Some(sources) = map_obj(v, "externalSources", m, "/connections")? {
        for (i, (key, x)) in sources.iter().enumerate() {
            let p = path("/externalSources", key);
            let t = format!("/connections/{i}");
            let pt = format!("/v2/industrial/connections/{i}");
            if x.get("id")?.as_str() != Some(key) {
                return fail(m, &p, "source-id-mismatch");
            }
            let legacy = x.get("driver")?.as_str()?;
            let (driver, policy) = match legacy {
                "s7" => ("siemensS7", "siemensS7"),
                "opcua" => ("opcUa", "opcUa"),
                "loopback" => ("simulator", "loopback"),
                _ => return fail(m, &path(&p, "driver"), "unsupported-driver"),
            };
            let id = m.id(&path(&p, "id"), key, "connection");
            field(m, &p, "id", &t, "id");
            field(m, &p, "driver", &pt, "driver");
            let name = match x.get("displayName") {
                Some(n) => m.text(n, &path(&p, "displayName"), &path(&t, "name"))?,
                None => key.clone(),
            };
            let allowed = flag(x, "commissioningAllowed")?;
            if allowed && (ver < 9 || legacy == "loopback") {
                return fail(
                    m,
                    &path(&p, "commissioningAllowed"),
                    "invalid-commissioning-declaration",
                );
            }
            field(m, &p, "commissioningAllowed", &pt, "commissioningAllowed");
            let mut c = json!({"id":id,"name":name,"driver":driver});
            let mut params = json!({});
            let config = x
                .get("configuration")
                .map(Value::as_object)
                .unwrap_or(Some(&EMPTY))?;
            let cp = path(&p, "configuration");
            if config.is_empty() {
                m.mark(&cp, &path(&t, "parameters"));
            }
            match legacy {
                "s7" => {
                    let h = config.get("host")?.as_str()?;
                    if !host(h) {
                        return fail(m, &path(&cp, "host"), "invalid-host");
                    }
                    let port = match config.get("port") {
                        Some(v) => uint(v.as_str()?, 1, 65535)?,
                        None => 102,
                    };
                    c["endpoint"] = json!(format!("{h}:{port}"));
                    m.mark(&path(&cp, "host"), &path(&t, "endpoint"));
                    m.mark(&path(&cp, "port"), &path(&t, "endpoint"));
                    for (k, min, max) in [
                        ("rack", 0, 7),
                        ("slot", 0, 31),
                        ("pollIntervalMs", 20, 60000),
                        ("timeoutMs", 1, 60000),
                    ] {
                        if let Some(x) = config.get(k) {
                            params[k] = json!(uint(x.as_str()?, min, max)?);
                            m.mark(&path(&cp, k), &format!("{t}/parameters/{k}"));
                        }
                    }
                }
                "opcua" => {
                    let ep = config.get("endpoint")?.as_str()?;
                    if !endpoint(ep) {
                        return fail(
                            m,
                            &path(&cp, "endpoint"),
                            "unsupported-or-sensitive-endpoint",
                        );
                    }
                    c["endpoint"] = json!(ep);
                    m.mark(&path(&cp, "endpoint"), &path(&t, "endpoint"));
                    let pol = config.get("securityPolicy")?.as_str()?;
                    let mode = config.get("securityMode")?.as_str()?;
                    if !matches!(
                        (pol, mode),
                        ("None", "None")
                            | ("Basic256Sha256", "Sign")
                            | ("Basic256Sha256", "SignAndEncrypt")
                    ) {
                        return fail(m, &cp, "unsupported-security-preset");
                    }
                    for (k, s) in [("securityPolicy", pol), ("securityMode", mode)] {
                        params[k] = json!(s);
                        m.mark(&path(&cp, k), &format!("{t}/parameters/{k}"));
                    }
                    if let Some(x) = config.get("publishingIntervalMs") {
                        params["publishingIntervalMs"] = json!(uint(x.as_str()?, 20, 60000)?);
                        m.mark(
                            &path(&cp, "publishingIntervalMs"),
                            &format!("{t}/parameters/publishingIntervalMs"),
                        );
                    }
                }
                "loopback" => {
                    if !config.is_empty() {
                        return fail(m, &cp, "loopback-parameters-not-representable");
                    }
                }
                _ => return None,
            }
            if legacy != "loopback" {
                c["parameters"] = params;
            }
            source_ids.insert(key.as_str(), id.clone());
            connections.push(c);
            policies.push(json!({"connection":id,"driver":policy,"commissioningAllowed":allowed}));
        }
    }
    let tags = doc.get_mut("tags")?.as_array_mut()?;
    let mut tag_index = BTreeMap::new();
    let mut units = Vec::new();
    let mut unit_ids = BTreeMap::new();
    let mut variable_access = BTreeMap::new();
    if let Some(vars) = v.get("variables") {
        for (i, (key, x)) in vars.as_object()?.iter().enumerate() {
            tag_index.insert(key.as_str(), i);
            let p = path("/variables", key);
            let t = format!("/tags/{i}");
            let a = x
                .get("access")
                .map(Value::as_str)
                .unwrap_or(Some("READ_ONLY"))?;
            variable_access.insert(key.as_str(), a);
            // Base mapping previously marked this leaf; final access comes from the mapping below.
            if let Some(u) = x.get("engineeringUnit") {
                if u.is_null() {
                    field(m, &p, "engineeringUnit", &t, "unit");
                } else {
                    let symbol = u.as_str()?;
                    if symbol.trim().is_empty() {
                        return None;
                    }
                    let id = if let Some(id) = unit_ids.get(symbol) {
                        String::clone(id)
                    } else {
                        let j = units.len();
                        let id = m.id(
                            &format!("/@generated-units/{j}"),
                            &format!("legacy-unit-{j}"),
                            "unit",
                        );
                        units.push(json!({"id":id,"symbol":symbol}));
                        unit_ids.insert(symbol.to_owned(), id.clone());
                        id
                    };
                    tags[i]["unit"] = json!(id);
                    field(m, &p, "engineeringUnit", &t, "unit");
                }
            }
        }
    }
    let mut mapped = BTreeSet::new();
    let mut addresses = BTreeMap::new();
    let mut writes = Vec::new();
    if let Some(mappings) = map_obj(v, "externalMappings", m, "/tags")? {
        for (key, x) in mappings {
            let p = path("/externalMappings", key);
            if x.get("id")?.as_str() != Some(key) {
                return fail(m, &p, "mapping-id-mismatch");
            }
            let var = x.get("internalVariableId")?.as_str()?;
            let i = *tag_index.get(var).or_else(|| {
                m.fail(&path(&p, "internalVariableId"), "unknown-variable");
                None
            })?;
            if !mapped.insert(var) {
                return fail(m, &p, "multiple-mappings-for-variable");
            }
            let source = x.get("sourceId")?.as_str()?;
            let conn = source_ids.get(source).or_else(|| {
                m.fail(&path(&p, "sourceId"), "unknown-source");
                None
            })?;
            let t = format!("/tags/{i}");
            let access = x.get("access").map(Value::as_str).unwrap_or(Some("READ"))?;
            let va = *variable_access.get(var)?;
            let confirmed = flag(x, "writeConfirmed")?;
            let writable = match (va, access) {
                ("READ_ONLY", "READ") => false,
                ("READ_WRITE" | "COMMAND" | "SETPOINT", "READ_WRITE") if confirmed => true,
                _ => return fail(m, &path(&p, "access"), "ambiguous-or-unconfirmed-access"),
            };
            let addr = x.get("externalAddress")?.as_str()?;
            if addr.trim().is_empty() {
                return None;
            }
            // Alias writes cannot be proven equivalent under a single tag policy.
            let key = (source, addr);
            if let Some(previous_writable) = addresses.insert(key, writable) {
                if writable || previous_writable {
                    return fail(
                        m,
                        &path(&p, "externalAddress"),
                        "ambiguous-writable-address-alias",
                    );
                }
            }
            let tag = &mut tags[i];
            tag["connection"] = json!(conn);
            tag["address"] = json!(addr);
            tag["access"] = json!(if writable { "readWrite" } else { "read" });
            let id = tag["id"].as_str()?.to_owned();
            m.ids.insert(path(&p, "id"), id.clone());
            field(m, &p, "id", &t, "id");
            field(m, &p, "internalVariableId", &t, "id");
            field(m, &p, "sourceId", &t, "connection");
            field(m, &p, "externalAddress", &t, "address");
            field(m, &p, "access", &t, "access");
            if writable {
                if ver < 9 {
                    return fail(m, &p, "write-policy-requires-v9");
                }
                let cmd = x.get("command").filter(|c| c.is_object()).or_else(|| {
                    m.fail(&path(&p, "command"), "missing-write-command");
                    None
                })?;
                let wt = format!("/v2/industrial/tags/{}", writes.len());
                let w = command(
                    cmd,
                    &path(&p, "command"),
                    &wt,
                    &id,
                    tag["dataType"].as_str()?,
                    va,
                    m,
                )?;
                field(m, &p, "writeConfirmed", &wt, "writeConfirmed");
                writes.push(w);
            } else {
                if confirmed {
                    return fail(
                        m,
                        &path(&p, "writeConfirmed"),
                        "confirmation-on-read-mapping",
                    );
                }
                field(m, &p, "writeConfirmed", &t, "access");
                if let Some(c) = x.get("command") {
                    if !c.is_null() {
                        return fail(m, &path(&p, "command"), "command-on-read-only-tag");
                    }
                    m.mark(&path(&p, "command"), &t);
                }
            }
        }
    }
    for (key, access) in variable_access {
        if access != "READ_ONLY" && !mapped.contains(key) {
            return fail(
                m,
                &format!("{}/access", path("/variables", key)),
                "unmapped-writable-variable",
            );
        }
    }
    let (security, screens, nodes) = security(v, m, ver)?;
    doc["connections"] = json!(connections);
    doc["units"] = json!(units);
    doc["v2"]["industrial"] = json!({"connections":policies,"tags":writes,"security":security,"screens":screens,"nodes":nodes});
    Some(())
}

static EMPTY: std::sync::LazyLock<serde_json::Map<String, Value>> =
    std::sync::LazyLock::new(serde_json::Map::new);

fn command(
    x: &Value,
    p: &str,
    t: &str,
    id: &str,
    ty: &str,
    va: &str,
    m: &mut Mapping,
) -> Option<Value> {
    let kind = x.get("kind")?.as_str()?;
    let k = match (kind, ty, va) {
        ("MOMENTARY", "bool", "COMMAND" | "READ_WRITE") => "momentary",
        ("MAINTAINED", "bool", "COMMAND" | "READ_WRITE") => "maintained",
        ("SETPOINT", "int32" | "int64" | "float32" | "float64", "SETPOINT" | "READ_WRITE") => {
            "setpoint"
        }
        _ => return fail(m, &path(p, "kind"), "command-type-or-access-mismatch"),
    };
    let critical = flag(x, "critical")?;
    let confirm = flag(x, "confirm")?;
    // Critical reason/second confirmation are not expressible by schema v2.
    if critical {
        return fail(
            m,
            &path(p, "critical"),
            "critical-reason-policy-not-representable",
        );
    }
    if flag(x, "writeOnly")? {
        return fail(
            m,
            &path(p, "writeOnly"),
            "write-only-command-not-representable",
        );
    }
    let mut w = json!({"tag":id,"command":k,"critical":false,"confirmationRequired":confirm||k!="momentary","writeConfirmed":true});
    for (a, b) in [
        ("kind", "command"),
        ("critical", "critical"),
        ("confirm", "confirmationRequired"),
    ] {
        field(m, p, a, t, b);
    }
    m.mark(&path(p, "writeOnly"), t);
    for key in ["decimals", "maxChange", "readbackTimeoutMs"] {
        if let Some(x) = x.get(key) {
            if !x.is_null() {
                return fail(m, &path(p, key), "write-policy-field-not-representable");
            }
            m.mark(&path(p, key), t);
        }
    }
    if k == "momentary" {
        let pulse = match x.get("pulseMs") {
            None | Some(Value::Null) => 500,
            Some(x) => x.as_u64()?,
        };
        if !(100..=5000).contains(&pulse) {
            return fail(m, &path(p, "pulseMs"), "pulse-out-of-range");
        }
        w["pulseMs"] = json!(pulse);
        field(m, p, "pulseMs", t, "pulseMs");
    } else if let Some(x) = x.get("pulseMs") {
        if !x.is_null() {
            return None;
        }
        m.mark(&path(p, "pulseMs"), t);
    }
    let mut limits = json!({});
    for key in ["min", "max", "step"] {
        match x.get(key) {
            Some(v) if !v.is_null() => {
                if k != "setpoint" {
                    return None;
                }
                limits[key] = numeric_limit(v, ty).or_else(|| {
                    m.fail(&path(p, key), "inexact-write-limit");
                    None
                })?;
                m.mark(&path(p, key), &format!("{t}/limits/{key}"));
            }
            _ if k == "setpoint" && key != "step" => {
                return fail(m, &path(p, key), "missing-setpoint-limit")
            }
            Some(_) => m.mark(&path(p, key), t),
            None => {}
        }
    }
    w["limits"] = limits;
    Some(w)
}

fn security(v: &Value, m: &mut Mapping, ver: u64) -> Option<(Value, Vec<Value>, Vec<Value>)> {
    let mut permissions = Vec::new();
    let mut roles = Vec::new();
    let mut ps = BTreeMap::new();
    let mut rs = BTreeMap::new();
    let mut sec = json!({});
    let mut screens = Vec::new();
    let mut nodes = Vec::new();
    if let Some(s) = v.get("security") {
        if s.is_null() {
            m.mark("/security", "/v2/industrial/security");
        } else {
            if ver < 8 || !s.is_object() {
                return None;
            }
            let t = "/v2/industrial/security";
            if s.as_object()?.is_empty() {
                m.mark("/security", t);
            }
            for (key, name) in [
                ("operate", "Operate"),
                ("setpoints", "Setpoints"),
                ("alarms.acknowledge", "Alarm acknowledgement"),
                ("project.edit", "Project editing"),
                ("users.manage", "User management"),
                ("runtime.maintain", "Runtime maintenance"),
            ] {
                let id = m.id(&path("/@builtin-permissions", key), key, "permission");
                ps.insert(key.to_owned(), id.clone());
                permissions.push(json!({"id":id,"name":name}));
            }
            if let Some(o) = s.get("permissions") {
                let o = o.as_object()?;
                if o.is_empty() {
                    m.mark("/security/permissions", &path(t, "permissions"));
                }
                for (key, x) in o {
                    let p = path("/security/permissions", key);
                    let dest = format!("{t}/permissions/{}", permissions.len());
                    if x.get("id")?.as_str() != Some(key) || ps.contains_key(key) {
                        return fail(m, &p, "duplicate-or-reserved-permission");
                    }
                    let id = m.id(&path(&p, "id"), key, "permission");
                    let name = m.text(
                        x.get("displayName")?,
                        &path(&p, "displayName"),
                        &path(&dest, "name"),
                    )?;
                    field(m, &p, "id", &dest, "id");
                    if let Some(d) = x.get("description") {
                        if !d.is_null() {
                            return fail(
                                m,
                                &path(&p, "description"),
                                "permission-description-not-representable",
                            );
                        }
                        m.mark(&path(&p, "description"), &dest);
                    }
                    ps.insert(key.clone(), id.clone());
                    permissions.push(json!({"id":id,"name":name}));
                }
            }
            if let Some(o) = s.get("roles") {
                let o = o.as_object()?;
                if o.is_empty() {
                    m.mark("/security/roles", &path(t, "roles"));
                }
                for (key, x) in o {
                    let p = path("/security/roles", key);
                    let dest = format!("{t}/roles/{}", roles.len());
                    if x.get("id")?.as_str() != Some(key) {
                        return None;
                    }
                    let id = m.id(&path(&p, "id"), key, "role");
                    field(m, &p, "id", &dest, "id");
                    let name = m.text(
                        x.get("displayName")?,
                        &path(&p, "displayName"),
                        &path(&dest, "name"),
                    )?;
                    let mut grants = Vec::new();
                    let mut seen = BTreeSet::new();
                    if let Some(a) = x.get("permissions") {
                        let a = a.as_array()?;
                        if a.is_empty() {
                            m.mark(&path(&p, "permissions"), &path(&dest, "permissions"));
                        }
                        for (j, g) in a.iter().enumerate() {
                            let key = g.as_str()?;
                            if !seen.insert(key) {
                                return None;
                            }
                            let id = ps.get(key).or_else(|| {
                                m.fail(&format!("{p}/permissions/{j}"), "unknown-permission");
                                None
                            })?;
                            grants.push(id.clone());
                            m.mark(
                                &format!("{p}/permissions/{j}"),
                                &format!("{dest}/permissions/{j}"),
                            );
                        }
                    }
                    rs.insert(key.clone(), id.clone());
                    roles.push(json!({"id":id,"name":name,"permissions":grants}));
                }
            }
            if let Some(x) = s.get("anonymousRoleId") {
                if !x.is_null() {
                    sec["anonymousRole"] = json!(rs.get(x.as_str()?)?);
                }
                m.mark("/security/anonymousRoleId", &path(t, "anonymousRole"));
            }
            if let Some(x) = s.get("autoLogoutMinutes") {
                if !x.is_null() {
                    let secs = u32::try_from(x.as_u64()?).ok()?.checked_mul(60)?;
                    if secs == 0 || x.as_u64()? > i32::MAX as u64 {
                        return None;
                    }
                    sec["autoLogoutSeconds"] = json!(secs);
                }
                m.mark("/security/autoLogoutMinutes", &path(t, "autoLogoutSeconds"));
            }
            sec["permissions"] = json!(permissions);
            sec["roles"] = json!(roles);
        }
    }
    // Resolve permissions by source IDs, not by display names. The screens
    // and placed nodes have already been allocated by the base mapper.
    for (key, s) in v["screens"].as_object()? {
        let p = path("/screens", key);
        if let Some(x) = s.get("requiredPermission").filter(|x| !x.is_null()) {
            let perm = ps.get(x.as_str()?).or_else(|| {
                m.fail(&path(&p, "requiredPermission"), "unknown-permission");
                None
            })?;
            let id = m.ids.get(&path(&p, "id"))?;
            let t = format!("/v2/industrial/screens/{}", screens.len());
            screens.push(json!({"screen":id,"requiredPermission":perm}));
            field(m, &p, "requiredPermission", &t, "requiredPermission");
        }
        if let Some(c) = s.get("components") {
            for (key, x) in c.as_object()? {
                let p = path(&path(&p, "components"), key);
                if let Some(x) = x.get("requiredPermission").filter(|x| !x.is_null()) {
                    let perm = ps.get(x.as_str()?).or_else(|| {
                        m.fail(&path(&p, "requiredPermission"), "unknown-permission");
                        None
                    })?;
                    let id = m.ids.get(&path(&p, "id"))?;
                    let t = format!("/v2/industrial/nodes/{}", nodes.len());
                    nodes.push(json!({"node":id,"requiredPermission":perm}));
                    field(m, &p, "requiredPermission", &t, "requiredPermission");
                }
            }
        }
    }
    Some((sec, screens, nodes))
}
