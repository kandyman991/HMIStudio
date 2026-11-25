use hmi_legacy_import::{import_portable, MAX_INPUT_BYTES};
use hmi_project_model::serialization::save_project;
use serde_json::{json, Value};
use std::collections::BTreeSet;
fn fixture(version: u32) -> Value {
    json!({"schemaVersion":version,"metadata":{"name":"Synthetic","description":"Original test","defaultScreenId":"screen/a","targetProfiles":[]},"screens":{"screen/a":{"id":"screen/a","title":"Main","widthDp":640,"heightDp":480,"components":{"label-a":{"id":"label-a","type":"label","geometry":{"x":2,"y":3,"width":100,"height":20},"properties":{"text":"Synthetic"},"visible":true,"requiredPermission":null}}}},"variables":{"label-a":{"id":"label-a","path":"Internal","type":"LONG","access":"READ_ONLY"}},"externalSources":{},"externalMappings":{}})
}
fn paths(v: &Value, p: &str, out: &mut BTreeSet<String>) {
    match v {
        Value::Object(o) if !o.is_empty() => {
            for (k, x) in o {
                paths(
                    x,
                    &format!("{p}/{}", k.replace('~', "~0").replace('/', "~1")),
                    out,
                )
            }
        }
        Value::Array(a) if !a.is_empty() => {
            for (i, x) in a.iter().enumerate() {
                paths(x, &format!("{p}/{i}"), out)
            }
        }
        _ => {
            out.insert(p.into());
        }
    }
}
#[test]
fn all_versions_and_determinism() {
    for ver in 1..=10 {
        let f = fixture(ver);
        let bytes = serde_json::to_vec(&f).unwrap();
        let a = import_portable(&bytes);
        let b = import_portable(&bytes);
        assert_eq!(a.report, b.report);
        assert_eq!(a.report.source_schema_version, Some(ver.into()));
        let p = a.project.unwrap_or_else(|| panic!("{:?}", a.report));
        let q = b.project.unwrap();
        assert_eq!(save_project(&p).unwrap(), save_project(&q).unwrap());
        assert_eq!(p.schema_version, 2);
        assert_ne!(p.tags[0].id.as_str(), "label-a");
        assert_eq!(p.screens[0].id, p.navigation.start_screen);
        assert_eq!(
            p.screens[0].root.children[0].properties["text"],
            "Synthetic"
        );
        let mut expected = BTreeSet::new();
        paths(&f, "", &mut expected);
        assert_eq!(
            expected,
            a.report.findings.iter().map(|x| x.path.clone()).collect()
        );
        assert!(a.report.findings.iter().all(|f| f.disposition == "mapped"));
    }
}
#[test]
fn unsupported_and_sensitive_refuse_without_values() {
    let mut f = fixture(10);
    f["externalSources"]["s"] =
        json!({"configuration":{"password":"SECRET-TEST"},"commissioningAllowed":true});
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    assert!(a.project.is_none());
    assert!(a.report.findings.iter().any(|x| x.path
        == "/externalSources/s/configuration/password"
        && x.disposition == "refused"));
    assert!(!serde_json::to_string(&a.report)
        .unwrap()
        .contains("SECRET-TEST"));
    let mut expected = BTreeSet::new();
    paths(&f, "", &mut expected);
    assert!(expected.is_subset(&a.report.findings.iter().map(|x| x.path.clone()).collect()));
}
#[test]
fn strict_admission() {
    for bytes in [
        b"{\"schemaVersion\":1,\"schemaVersion\":2}".as_slice(),
        b"{\"schemaVersion\":1,\"metadata\":{\"name\":\"a\",\"name\":\"b\"}}",
        b"\xff",
        b"{}",
    ][..]
        .iter()
    {
        assert!(import_portable(bytes).project.is_none());
    }
    for ver in [0, 11] {
        assert!(import_portable(&serde_json::to_vec(&fixture(ver)).unwrap())
            .project
            .is_none());
    }
    assert!(import_portable(&vec![0; MAX_INPUT_BYTES + 1])
        .project
        .is_none());
}
#[test]
fn refuse_wrong_types_write_access_and_unknowns() {
    for (p, value) in [
        ("/variables/label-a/access", json!("COMMAND")),
        ("/externalSources", json!([])),
        (
            "/screens/screen~1a/components/label-a/requiredPermission",
            json!({}),
        ),
        (
            "/screens/screen~1a/components/label-a/properties",
            json!({"text":"ok","unexpected":"value"}),
        ),
    ] {
        let mut f = fixture(10);
        *f.pointer_mut(p).unwrap_or_else(|| panic!("{p}")) = value;
        assert!(
            import_portable(&serde_json::to_vec(&f).unwrap())
                .project
                .is_none(),
            "{p}"
        );
    }
}

#[test]
fn exact_long_default_and_type_mismatch() {
    let mut f = fixture(10);
    f["variables"]["label-a"]["defaultValue"] =
        json!({"valueType":"long","value":9223372036854775807i64});
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    assert_eq!(
        a.project.unwrap_or_else(|| panic!("{:?}", a.report)).tags[0].initial_value,
        Some(json!(i64::MAX))
    );
    f["variables"]["label-a"]["defaultValue"]["valueType"] = json!("double");
    assert!(import_portable(&serde_json::to_vec(&f).unwrap())
        .project
        .is_none());
}
#[test]
fn explicit_screen_order_is_version_aware() {
    let mut f = fixture(5);
    f["screens"]["second"] = json!({"id":"second","title":"Second"});
    f["screenOrder"] = json!(["second", "screen/a"]);
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    assert_eq!(
        a.project
            .unwrap_or_else(|| panic!("{:?}", a.report))
            .screens[0]
            .id
            .as_str(),
        "second"
    );
    f["schemaVersion"] = json!(4);
    assert!(import_portable(&serde_json::to_vec(&f).unwrap())
        .project
        .is_none());
    f["schemaVersion"] = json!(5);
    f["screenOrder"] = json!(["second", "second"]);
    assert!(import_portable(&serde_json::to_vec(&f).unwrap())
        .project
        .is_none());
}
