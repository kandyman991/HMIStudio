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

#[test]
fn all_five_placed_contracts_validate_and_strict_v1_admission_is_preserved() {
    let mut v = base();
    v["schemaVersion"] = json!(2);
    v["tags"][3]["access"] = json!("readWrite");
    v["tags"][4]["access"] = json!("readWrite");
    let specs = json!([
        {"id":"line-new","contract":{"kind":"line","start":{"x":0,"y":0},"end":{"x":12,"y":5},"strokeColor":"#000000","strokeWidth":1}},
        {"id":"arrow-new","contract":{"kind":"arrow","start":{"x":0,"y":0},"end":{"x":12,"y":5},"strokeColor":"#000000","strokeWidth":1,"headLength":2}},
        {"id":"poly-new","contract":{"kind":"polygon","points":[{"x":0,"y":0},{"x":10,"y":0},{"x":0,"y":10}],"fill":"#FFFFFF","strokeColor":"#000000","strokeWidth":1}},
        {"id":"input-new","contract":{"kind":"textInput","tag":"tag-text","maxLength":24}},
        {"id":"drop-new","contract":{"kind":"dropDown","tag":"tag-word","enumeration":"enum-word"}}
    ]);
    for spec in specs.as_array().unwrap() {
        v["screens"][0]["root"]["children"].as_array_mut().unwrap().push(json!({"id":spec["id"],"element":{"type":"component","kind":spec["contract"]["kind"]}}));
    }
    v["v2"] = json!({"bindings":{"enumerations":[{"id":"enum-word","name":"State","dataType":"uint16","entries":[{"value":1,"label":"One"}]}]},"components":{"components":specs}});
    let p = load_project(&v.to_string()).unwrap().project;
    assert!(p.has_v2_features());
    assert_eq!(load_project(&save_project(&p).unwrap()).unwrap().project, p);
    let mut bad = v.clone();
    bad["v2"]["components"]["components"][0]["id"] = json!("n-text");
    assert!(load_project(&bad.to_string()).is_err());
    bad = v.clone();
    bad["v2"]["components"]["components"]
        .as_array_mut()
        .unwrap()
        .pop();
    assert!(load_project(&bad.to_string()).is_err());
    bad = v.clone();
    bad["v2"]["components"]["components"]
        .as_array_mut()
        .unwrap()
        .push(specs[0].clone());
    assert!(load_project(&bad.to_string()).is_err());
    v.as_object_mut().unwrap().remove("v2");
    assert!(load_project(&v.to_string()).is_err());
    v["schemaVersion"] = json!(1);
    assert!(parse_project(&v.to_string()).is_err());
    let mut extension = base();
    extension["extensions"] = json!({"example":{"element":{"type":"component","kind":"line"}}});
    assert!(load_project(&extension.to_string()).is_ok());
}

#[test]
fn canonical_diagnostics_point_to_real_nested_declarations() {
    use hmi_project_model::validation::validate;
    let cases = [
        (
            json!({"industrial":{"security":{"autoLogoutSeconds":0}}}),
            "/v2/industrial/security/autoLogoutSeconds",
        ),
        (
            json!({"bindings":{"enumerations":[{"id":"enum-state","name":"State","dataType":"uint16","entries":[{"value":65536,"label":"Invalid"}]}]}}),
            "/v2/bindings/enumerations/0/entries/0/value",
        ),
        (
            json!({"components":{"handlers":[{"origin":{"node":"missing-node"},"event":"click","actions":[{"type":"navigateStartup"}]}]}}),
            "/v2/components/handlers/0/origin",
        ),
    ];
    for (declarations, expected) in cases {
        let mut doc = base();
        doc["schemaVersion"] = json!(2);
        doc["v2"] = declarations;
        let project = parse_project(&doc.to_string()).unwrap().project;
        let report = validate(&project);
        assert!(report.errors().any(|d| d.path == expected), "{report:?}");
        assert!(doc.pointer(expected).is_some());
    }
}
