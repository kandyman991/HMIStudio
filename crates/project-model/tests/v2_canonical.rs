//! Canonical v2 admission and migration regression coverage.
#![allow(clippy::unwrap_used)]
mod common;
use common::*;
use hmi_project_model::{load_project, parse_project, save_project, CURRENT_SCHEMA_VERSION};
use serde_json::json;

#[test]
fn migration_preserves_v1_and_never_promotes_v2_policy() {
    let mut v = base();
    v["extensions"] = json!({"synthetic.exact":u64::MAX});
    let loaded = load_project(&v.to_string()).unwrap();
    assert_eq!(loaded.source_version, 1);
    assert_eq!(loaded.migrations.len(), 1);
    assert!(loaded.project.v2.is_empty());
    assert_eq!(loaded.project.schema_version, CURRENT_SCHEMA_VERSION);
    let saved = save_project(&loaded.project).unwrap();
    let migrated = load_project(&saved).unwrap();
    assert_eq!(migrated.project, loaded.project);
    assert!(migrated.migrations.is_empty());
    for invalid in [
        json!(null),
        json!({}),
        json!({"industrial":{"security":{"autoLogoutSeconds":30}}}),
    ] {
        v["v2"] = invalid;
        assert!(parse_project(&v.to_string()).is_err());
    }
}

#[test]
fn canonical_v2_sections_validate_and_roundtrip_together() {
    let mut v = base();
    v["schemaVersion"] = json!(2);
    v["v2"] = json!({
        "industrial":{"security":{"permissions":[{"id":"perm-read","name":"Read"}],"roles":[{"id":"role-operator","name":"Operator","permissions":["perm-read"]}]},"screens":[{"screen":"scr-main","requiredPermission":"perm-read"}]},
        "bindings":{"enumerations":[{"id":"enum-word","name":"State","dataType":"uint16","entries":[{"value":1,"label":{"textKey":"hello"}}]}],"mappings":[{"tag":"tag-word","enumeration":"enum-word"}],"expressions":[{"node":"n-button","property":"enabled","contractVersion":1,"expression":"`tag-word` > 0"}]},
        "components":{"definitions":[{"definition":"cmp-lamp","classification":{"kind":"faceplate"}}],"handlers":[{"origin":{"node":"n-text"},"event":"click","actions":[{"type":"navigateStartup"}]}]}
    });
    let p = load_project(&v.to_string()).unwrap().project;
    assert!(!p.v2.is_empty());
    assert_eq!(load_project(&save_project(&p).unwrap()).unwrap().project, p);
    v["v2"]["bindings"]["enumerations"][0]["id"] = json!("perm-read");
    assert!(load_project(&v.to_string()).is_err());
    v["v2"]["bindings"]["enumerations"][0]["id"] = json!("enum-word");
    v["v2"]["components"]["components"] = json!([{"id":"detached","contract":{"kind":"line","start":{"x":0,"y":0},"end":{"x":1,"y":1},"strokeColor":"#000000","strokeWidth":1}}]);
    assert!(load_project(&v.to_string()).is_err());
}
