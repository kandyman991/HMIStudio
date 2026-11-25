//! Original synthetic staged v2 declarations.
#![allow(clippy::unwrap_used)]
mod common;
use common::*;
use hmi_project_model::model::v2::*;
use hmi_project_model::validation::validate_v2_declarations;
use hmi_project_model::{parse_project, DiagnosticCode as C, CURRENT_SCHEMA_VERSION};
use serde_json::{json, Value};

fn declarations() -> Value {
    json!({
        "connections":[{"connection":"conn-sim","driver":"loopback"}],
        "tags":[{"tag":"tag-bool-rw","command":"momentary","pulseMs":100,"critical":true,"confirmationRequired":true},
                {"tag":"tag-real-rw","command":"setpoint","limits":{"min":0,"max":10,"step":0.5}}],
        "security":{"permissions":[{"id":"perm-run","name":"Run"}],
                    "roles":[{"id":"role-operator","name":"Operator","permissions":["perm-run"]}],
                    "anonymousRole":"role-operator","autoLogoutSeconds":60},
        "screens":[{"screen":"scr-main","requiredPermission":"perm-run"}],
        "nodes":[{"node":"n-lamp-ind","requiredPermission":"perm-run"}]
    })
}
fn check(p: Value, d: Value) -> hmi_project_model::ValidationReport {
    validate_v2_declarations(&project(p), &serde_json::from_value(d).unwrap())
}
#[test]
fn typed_declarations_roundtrip_without_activating_v1() {
    let d: IndustrialSecurityDeclarations = serde_json::from_value(declarations()).unwrap();
    assert_eq!(
        d,
        serde_json::from_str(&serde_json::to_string(&d).unwrap()).unwrap()
    );
    assert!(validate_v2_declarations(&project(base()), &d).is_valid());
    assert_eq!(CURRENT_SCHEMA_VERSION, 2);
    let mut v = base();
    v["security"] = declarations()["security"].clone();
    assert!(parse_project(&v.to_string()).is_err());
    assert!(!d.connections[0].commissioning_allowed);
    assert!(!d.tags[0].write_confirmed);
}
#[test]
fn exact_unsigned_limits_do_not_round_adjacent_values() {
    let mut p = base();
    p["tags"][2]["dataType"] = json!("uint64");
    p["recipes"][0]["sets"][0]["values"]["fld-1"] = json!(1);
    let mut d = declarations();
    d["tags"][1]["limits"] = json!({"min":u64::MAX-1,"max":u64::MAX,"step":1});
    assert!(check(p.clone(), d.clone()).is_valid());
    d["tags"][1]["limits"]["min"] = json!(u64::MAX);
    d["tags"][1]["limits"]["max"] = json!(u64::MAX - 1);
    assert_error(
        &check(p.clone(), d.clone()),
        C::InvalidTag,
        "/v2/tags/1/limits/max",
    );
    d["tags"][1]["limits"] = json!({"min":-1,"step":0});
    let r = check(p, d);
    assert_error(&r, C::InvalidTag, "/v2/tags/1/limits/min");
    assert_error(&r, C::InvalidTag, "/v2/tags/1/limits/step");
}
#[test]
fn reject_invalid_command_semantics_and_narrowing() {
    let mut d = declarations();
    d["tags"][0]["tag"] = json!("tag-bool-ro");
    d["tags"][0]["pulseMs"] = json!(0);
    d["tags"][0]["confirmationRequired"] = json!(false);
    d["tags"][1]["limits"]["step"] = json!(0.1);
    let r = check(base(), d);
    for path in [
        "/v2/tags/0/tag",
        "/v2/tags/0/pulseMs",
        "/v2/tags/0/confirmationRequired",
        "/v2/tags/1/limits/step",
    ] {
        assert_error(&r, C::InvalidTag, path);
    }
}
#[test]
fn reject_unknown_roles_permissions_targets_and_global_collisions() {
    let mut d = declarations();
    d["security"]["permissions"][0]["id"] = json!("tag-word");
    d["security"]["roles"][0]["permissions"] = json!(["missing", "missing"]);
    d["security"]["anonymousRole"] = json!("missing");
    d["security"]["autoLogoutSeconds"] = json!(0);
    d["screens"][0]["screen"] = json!("n-root");
    d["nodes"][0]["node"] = json!("scr-main");
    let r = check(base(), d.clone());
    assert_error(&r, C::DuplicateIdentifier, "/v2/security/permissions/0/id");
    assert_error(
        &r,
        C::DuplicateIdentifier,
        "/v2/security/roles/0/permissions/1",
    );
    for path in [
        "/v2/security/roles/0/permissions/0",
        "/v2/security/anonymousRole",
        "/v2/screens/0/screen",
        "/v2/nodes/0/node",
        "/v2/nodes/0/requiredPermission",
    ] {
        assert_error(&r, C::UnknownReference, path);
    }
    assert_error(&r, C::InvalidValue, "/v2/security/autoLogoutSeconds");
    assert_eq!(r, check(base(), d));
}
#[test]
fn loopback_has_no_physical_connection_or_commissioning() {
    let mut p = base();
    p["connections"][0]["endpoint"] = json!("local-test");
    let mut d = declarations();
    d["connections"][0]["commissioningAllowed"] = json!(true);
    let r = check(p, d);
    assert_error(&r, C::InvalidValue, "/v2/connections/0/driver");
    assert_error(
        &r,
        C::InvalidValue,
        "/v2/connections/0/commissioningAllowed",
    );
}
#[test]
fn reject_unknown_fields_in_security_and_duplicate_declarations() {
    let mut d = declarations();
    d["security"]["password"] = json!("synthetic-not-a-credential");
    assert!(serde_json::from_value::<IndustrialSecurityDeclarations>(d).is_err());
    let mut d = declarations();
    let entry = d["nodes"][0].clone();
    d["nodes"].as_array_mut().unwrap().push(entry);
    assert_error(
        &check(base(), d),
        C::DuplicateIdentifier,
        "/v2/nodes/1/node",
    );
}
#[test]
fn invalid_base_cannot_validate_candidate() {
    let mut p = base();
    p["navigation"]["startScreen"] = json!("missing");
    assert!(check(p, declarations()).has_errors());
}
