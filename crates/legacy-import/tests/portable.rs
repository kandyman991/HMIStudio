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

fn industrial_fixture() -> Value {
    let mut f = fixture(9);
    f["variables"]["label-a"]["access"] = json!("SETPOINT");
    f["variables"]["label-a"]["engineeringUnit"] = json!("rpm");
    f["variables"]["second"] = json!({"id":"second","path":"Second","type":"LONG","access":"READ_ONLY","engineeringUnit":"rpm"});
    f["externalSources"]["src/a"] = json!({"id":"src/a","driver":"s7","displayName":"Synthetic source","configuration":{"host":"example.invalid","port":"102","rack":"0","slot":"1","pollIntervalMs":"200","timeoutMs":"2000"},"commissioningAllowed":true});
    f["externalMappings"]["map-a"] = json!({"id":"map-a","internalVariableId":"label-a","sourceId":"src/a","externalAddress":"DB1.DBD0","access":"READ_WRITE","writeConfirmed":true,"command":{"kind":"SETPOINT","min":-9007199254740992i64,"max":9007199254740992i64,"step":2,"confirm":false}});
    f["externalMappings"]["map-b"] = json!({"id":"map-b","internalVariableId":"second","sourceId":"src/a","externalAddress":"DB1.DBD8","access":"READ"});
    f["security"] = json!({"permissions":{"custom/a":{"id":"custom/a","displayName":"Custom"}},"roles":{"role/a":{"id":"role/a","displayName":"Operator","permissions":["custom/a","operate"]}},"anonymousRoleId":"role/a","autoLogoutMinutes":3});
    f["screens"]["screen/a"]["requiredPermission"] = json!("custom/a");
    f["screens"]["screen/a"]["components"]["label-a"]["requiredPermission"] = json!("operate");
    f
}
#[test]
fn industrial_mapping_and_leaf_accounting() {
    let f = industrial_fixture();
    let bytes = serde_json::to_vec(&f).unwrap();
    let a = import_portable(&bytes);
    let b = import_portable(&bytes);
    assert_eq!(a.report, b.report);
    let p = a.project.unwrap_or_else(|| panic!("{:?}", a.report));
    let q = b.project.unwrap();
    assert_eq!(save_project(&p).unwrap(), save_project(&q).unwrap());
    assert!(p.has_v2_features());
    assert_eq!(p.connections.len(), 1);
    assert_eq!(p.units.len(), 1);
    assert_eq!(p.tags[0].unit, p.tags[1].unit);
    assert_eq!(p.tags[0].connection.as_ref(), Some(&p.connections[0].id));
    let v = serde_json::to_value(&p).unwrap();
    let i = &v["v2"]["industrial"];
    assert_eq!(i["connections"][0]["commissioningAllowed"], true);
    assert_eq!(i["tags"][0]["limits"]["min"], json!(-9007199254740992i64));
    assert_eq!(i["tags"][0]["confirmationRequired"], true);
    assert_eq!(i["tags"][0]["writeConfirmed"], true);
    assert_eq!(i["security"]["autoLogoutSeconds"], 180);
    assert_eq!(
        i["security"]["roles"][0]["id"],
        i["security"]["anonymousRole"]
    );
    assert_eq!(
        i["screens"][0]["requiredPermission"],
        i["security"]["roles"][0]["permissions"][0]
    );
    assert_eq!(
        i["nodes"][0]["requiredPermission"],
        i["security"]["roles"][0]["permissions"][1]
    );
    let mut expected = BTreeSet::new();
    paths(&f, "", &mut expected);
    assert_eq!(
        expected,
        a.report.findings.iter().map(|x| x.path.clone()).collect()
    );
}
#[test]
fn industrial_refusal_cases() {
    let base = industrial_fixture();
    for (p, v) in [
        ("/schemaVersion", json!(8)),
        ("/externalMappings/map-a/writeConfirmed", json!(false)),
        ("/externalMappings/map-a/sourceId", json!("unknown")),
        (
            "/externalMappings/map-a/internalVariableId",
            json!("unknown"),
        ),
        ("/externalMappings/map-a/access", json!("WRITE")),
        (
            "/externalMappings/map-a/command/min",
            json!(9007199254740993i64),
        ),
        (
            "/externalMappings/map-a/command/max",
            json!(9223372036854775807i64),
        ),
        ("/externalMappings/map-a/command/step", json!(0)),
        ("/security/roles/role~1a/permissions/0", json!("unknown")),
        ("/security/anonymousRoleId", json!("unknown")),
        ("/security/autoLogoutMinutes", json!(71582789)),
        ("/screens/screen~1a/requiredPermission", json!("unknown")),
        (
            "/externalSources/src~1a/configuration/host",
            json!("user:secret@example.invalid"),
        ),
    ] {
        let mut f = base.clone();
        *f.pointer_mut(p).unwrap() = v;
        let a = import_portable(&serde_json::to_vec(&f).unwrap());
        assert!(a.project.is_none(), "{p}");
        assert!(a.report.findings.iter().any(|f| f.disposition == "refused"));
    }
    for (k, v) in [
        ("maxChange", json!(1)),
        ("decimals", json!(2)),
        ("readbackTimeoutMs", json!(3000)),
        ("critical", json!(true)),
        ("writeOnly", json!(true)),
    ] {
        let mut f = base.clone();
        f["externalMappings"]["map-a"]["command"][k] = v;
        assert!(
            import_portable(&serde_json::to_vec(&f).unwrap())
                .project
                .is_none(),
            "{k}"
        );
    }
    let mut f = base.clone();
    f["externalMappings"]["duplicate"] = f["externalMappings"]["map-a"].clone();
    f["externalMappings"]["duplicate"]["id"] = json!("duplicate");
    assert!(import_portable(&serde_json::to_vec(&f).unwrap())
        .project
        .is_none());
}
#[test]
fn momentary_and_maintained_defaults() {
    let mut f = industrial_fixture();
    f["variables"]["label-a"]["access"] = json!("COMMAND");
    f["variables"]["label-a"]["type"] = json!("BOOL");
    f["variables"]["label-a"]["engineeringUnit"] = Value::Null;
    for (kind, expected) in [("MOMENTARY", "momentary"), ("MAINTAINED", "maintained")] {
        f["externalMappings"]["map-a"]["command"] = json!({"kind":kind});
        let a = import_portable(&serde_json::to_vec(&f).unwrap());
        let v =
            serde_json::to_value(a.project.unwrap_or_else(|| panic!("{:?}", a.report))).unwrap();
        let w = &v["v2"]["industrial"]["tags"][0];
        assert_eq!(w["command"], expected);
        if kind == "MOMENTARY" {
            assert_eq!(w["pulseMs"], 500);
        } else {
            assert_eq!(w["confirmationRequired"], true);
        }
    }
    f["externalMappings"]["map-a"]["command"] = json!({"kind":"MOMENTARY","pulseMs":99});
    assert!(import_portable(&serde_json::to_vec(&f).unwrap())
        .project
        .is_none());
}
#[test]
fn opcua_and_loopback_declarations() {
    let mut f = industrial_fixture();
    f["externalSources"]["src/a"] = json!({"id":"src/a","driver":"opcua","configuration":{"endpoint":"opc.tcp://example.invalid:4840","securityPolicy":"Basic256Sha256","securityMode":"SignAndEncrypt","publishingIntervalMs":"200"}});
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    let p = a.project.unwrap_or_else(|| panic!("{:?}", a.report));
    assert!(p.has_v2_features());
    f["externalSources"]["src/a"]["configuration"]["endpoint"] =
        json!("opc.tcp://user:secret@example.invalid:4840");
    assert!(import_portable(&serde_json::to_vec(&f).unwrap())
        .project
        .is_none());
    f["externalSources"]["src/a"] = json!({"id":"src/a","driver":"loopback","configuration":{}});
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    assert!(a
        .project
        .unwrap_or_else(|| panic!("{:?}", a.report))
        .has_v2_features());
}

#[test]
fn refuse_address_alias_with_writable_tag_and_credential_keys() {
    let mut f = industrial_fixture();
    f["externalMappings"]["map-b"]["externalAddress"] =
        f["externalMappings"]["map-a"]["externalAddress"].clone();
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    assert!(a.project.is_none());
    assert!(a
        .report
        .findings
        .iter()
        .any(|x| x.code == "ambiguous-writable-address-alias"));
    let mut f = industrial_fixture();
    f["externalSources"]["src/a"]["configuration"]["password"] = json!("SECRET-TEST");
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    assert!(a.project.is_none());
    assert!(!serde_json::to_string(&a.report)
        .unwrap()
        .contains("SECRET-TEST"));
}

#[test]
fn graphics_and_expression_remap() {
    let mut f = fixture(10);
    f["variables"]["label-a"]["type"] = json!("STRING");
    let n = &mut f["screens"]["screen/a"]["components"]["label-a"];
    n["expressions"] = json!({"text":"`label-a` + ' label-a'"});
    f["enumerations"] = json!({"state/a":{"id":"state/a","name":"States","entries":[{"value":9223372036854775807i64,"text":"Max","color":"#123456"},{"value":-1,"text":"Other"}]}});
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    let p = a.project.unwrap_or_else(|| panic!("{:?}", a.report));
    let v = serde_json::to_value(&p).unwrap();
    assert_eq!(
        v["v2"]["bindings"]["expressions"][0]["expression"],
        format!("`{}` + ' label-a'", p.tags[0].id)
    );
    assert_eq!(
        v["v2"]["bindings"]["enumerations"][0]["entries"][0]["value"],
        json!(i64::MAX)
    );
    let mut expected = BTreeSet::new();
    paths(&f, "", &mut expected);
    assert_eq!(
        expected,
        a.report.findings.iter().map(|x| x.path.clone()).collect()
    );
    let mut f2 = f.clone();
    f2["screens"]["screen/a"]["components"]["label-a"]["bindings"] = json!({"text":"label-a"});
    assert!(import_portable(&serde_json::to_vec(&f2).unwrap())
        .project
        .is_none());
    f["screens"]["screen/a"]["components"]["label-a"]["expressions"] =
        json!({"text":"unknown + 'x'"});
    assert!(import_portable(&serde_json::to_vec(&f).unwrap())
        .project
        .is_none());
}
#[test]
fn direct_binding_and_shapes() {
    let mut f = fixture(5);
    f["variables"]["label-a"]["type"] = json!("STRING");
    let n = &mut f["screens"]["screen/a"]["components"]["label-a"];
    n["bindings"] = json!({"text":"label-a"});
    f["screens"]["screen/a"]["components"]["shape"] = json!({"id":"shape","type":"rectangle","zIndex":2,"properties":{"fillColor":"#123456","strokeColor":"#000000","strokeWidth":"2","cornerRadius":"4"}});
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    let p = a.project.unwrap_or_else(|| panic!("{:?}", a.report));
    let v = serde_json::to_value(&p).unwrap();
    assert_eq!(
        v["screens"][0]["root"]["children"][0]["bindings"][0]["source"]["tag"],
        p.tags[0].id.as_str()
    );
    assert_eq!(
        v["screens"][0]["root"]["children"][1]["properties"]["strokeWidth"],
        2.0
    );
}
#[test]
fn nested_adaptive_tree() {
    let mut f = fixture(3);
    f["screens"]["screen/a"]["components"]["label-b"] =
        json!({"id":"label-b","type":"label","properties":{"text":"Second"}});
    f["screens"]["screen/a"]["layout"] = json!({"version":1,"root":{"node":"stack","id":"outer","direction":"COLUMN","children":[{"node":"stack","id":"inner","direction":"ROW","gapDp":8,"children":[{"node":"component","componentId":"label-b","minWidthDp":80,"weight":2},{"node":"component","componentId":"label-a","minHeightDp":30}]}]}});
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    let p = a.project.unwrap_or_else(|| panic!("{:?}", a.report));
    let v = serde_json::to_value(&p).unwrap();
    let inner = &v["screens"][0]["root"]["children"][0];
    assert_eq!(inner["element"]["type"], "row");
    assert_eq!(inner["children"][0]["id"], "label-b");
    assert!(a.report.findings.iter().any(|f| f.severity == "warning"));
    assert!(a
        .report
        .findings
        .iter()
        .any(|f| f.disposition == "preserved"));
    let mut expected = BTreeSet::new();
    paths(&f, "", &mut expected);
    assert_eq!(
        expected,
        a.report.findings.iter().map(|x| x.path.clone()).collect()
    );
    f["screens"]["screen/a"]["layout"]["root"]["children"][0]["children"][0]["componentId"] =
        json!("label-a");
    assert!(import_portable(&serde_json::to_vec(&f).unwrap())
        .project
        .is_none());
}
#[test]
fn static_reusable_definition() {
    let mut f = fixture(6);
    f["componentDefinitions"] = json!({"label-a":{"id":"label-a","displayName":"Reusable","type":"composite","kind":"COMPONENT","composition":{"widthDp":200,"heightDp":100,"elements":{"part":{"id":"part","type":"label","properties":{"text":"Inside"}}}}}});
    f["screens"]["screen/a"]["components"]["label-a"] =
        json!({"id":"label-a","type":"composite","componentDefinitionId":"label-a"});
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    let p = a.project.unwrap_or_else(|| panic!("{:?}", a.report));
    let v = serde_json::to_value(&p).unwrap();
    assert_eq!(
        v["screens"][0]["root"]["children"][0]["element"]["definition"],
        v["components"][0]["id"]
    );
    assert_ne!(v["components"][0]["id"], "label-a");
    f["componentDefinitions"]["label-a"]["kind"] = json!("FACEPLATE");
    assert!(import_portable(&serde_json::to_vec(&f).unwrap())
        .project
        .is_none());
}

#[test]
fn reusable_read_only_tag_parameters() {
    let mut f = fixture(6);
    f["variables"]["label-a"]["type"] = json!("STRING");
    f["componentDefinitions"] = json!({"def":{"id":"def","displayName":"Read-only composite","type":"composite","parameters":{"caption":{"type":"TEXT_VARIABLE"}},"composition":{"elements":{"part":{"id":"part","type":"label","properties":{"text":"Fallback"},"bindings":{"text":"caption"}}}}}});
    f["screens"]["screen/a"]["components"]["label-a"] = json!({"id":"label-a","type":"composite","componentDefinitionId":"def","bindings":{"caption":"label-a"}});
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    let p = a.project.unwrap_or_else(|| panic!("{:?}", a.report));
    let v = serde_json::to_value(&p).unwrap();
    assert_eq!(
        v["components"][0]["root"]["children"][0]["bindings"][0]["source"]["tagParameter"],
        "caption"
    );
    assert_eq!(
        v["screens"][0]["root"]["children"][0]["element"]["parameters"]["caption"],
        p.tags[0].id.as_str()
    );
    f["componentDefinitions"]["def"]["parameters"]["caption"]["writable"] = json!(true);
    assert!(import_portable(&serde_json::to_vec(&f).unwrap())
        .project
        .is_none());
}

#[test]
fn adaptive_unknown_geometry_and_unsupported_layout_refuse() {
    let mut f = fixture(3);
    f["screens"]["screen/a"]["layout"] = json!({"root":{"node":"stack","id":"outer","direction":"COLUMN","children":[{"node":"component","componentId":"label-a"}]}});
    f["screens"]["screen/a"]["components"]["label-a"]["geometry"]["unknown"] = json!(1);
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    assert!(a.project.is_none());
    assert!(a
        .report
        .findings
        .iter()
        .any(|x| x.path.ends_with("/geometry/unknown") && x.disposition == "refused"));
    f["screens"]["screen/a"]["components"]["label-a"]["geometry"]
        .as_object_mut()
        .unwrap()
        .remove("unknown");
    f["screens"]["screen/a"]["layout"]["root"]["direction"] = json!("ADAPTIVE");
    assert!(import_portable(&serde_json::to_vec(&f).unwrap())
        .project
        .is_some());
    f["screens"]["screen/a"]["layout"]["root"]["direction"] = json!("COLUMN");
    f["screens"]["screen/a"]["components"]["label-a"]["expressions"] = json!({"visible":"true"});
    assert!(import_portable(&serde_json::to_vec(&f).unwrap())
        .project
        .is_none());
}

#[test]
fn ordered_navigation_declarations_and_reference_remapping() {
    let mut f = fixture(10);
    f["screens"]["screen/a"]["components"]["label-a"]["type"] = json!("button");
    f["screens"]["screen/a"]["components"]["label-a"]["actions"] = json!({
        "press": [
            {"action":"navigate.screen","screenId":"screen/a"},
            {"action":"navigate.startup"},
            {"action":"navigate.back"}
        ],
        "release": {"action":"popup.close"}
    });
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    let p = a.project.unwrap_or_else(|| panic!("{:?}", a.report));
    let d = serde_json::to_value(&p).unwrap();
    let h = &d["v2"]["components"]["handlers"];
    assert_eq!(h[0]["event"], "press");
    assert_eq!(h[0]["actions"][0]["screen"], d["screens"][0]["id"]);
    assert_eq!(h[0]["actions"][1]["type"], "navigateStartup");
    assert_eq!(h[0]["actions"][2]["type"], "back");
    assert_eq!(h[1]["event"], "release");
    let mut expected = BTreeSet::new();
    paths(&f, "", &mut expected);
    assert_eq!(
        expected,
        a.report.findings.iter().map(|x| x.path.clone()).collect()
    );
    for bad in [
        json!({"press":{"action":"navigate.screen","screenId":"missing"}}),
        json!({"press":{"action":"value.toggle","variableId":"label-a"}}),
        json!({"hold":{"action":"navigate.back"}}),
        json!({"press":{"action":"navigate.back","extra":true}}),
        json!({"press":[]}),
    ] {
        f["screens"]["screen/a"]["components"]["label-a"]["actions"] = bad;
        assert!(import_portable(&serde_json::to_vec(&f).unwrap())
            .project
            .is_none());
    }
}

#[test]
fn fixed_frame_primitives_and_refusals() {
    let mut f = fixture(7);
    let c = &mut f["screens"]["screen/a"]["components"]["label-a"];
    c["type"] = json!("line");
    c["properties"] = json!({"points":"0,50 100,50","strokeColor":"#123456","strokeWidth":"2","strokeStyle":"SOLID"});
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    let p = a.project.unwrap_or_else(|| panic!("{:?}", a.report));
    let d = serde_json::to_value(&p).unwrap();
    let contract = &d["v2"]["components"]["components"][0]["contract"];
    assert_eq!(contract["start"]["x"], 1.0);
    assert_eq!(contract["end"]["x"], 99.0);
    assert_eq!(contract["start"]["y"], 10.0);
    let c = &mut f["screens"]["screen/a"]["components"]["label-a"];
    c["type"] = json!("polygon");
    c["properties"]["points"] = json!("0,0 100,0 50,100");
    c["properties"]["fillColor"] = json!("#FFFFFF");
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    assert!(a.project.is_some(), "{:?}", a.report);
    f["screens"]["screen/a"]["components"]["label-a"]["properties"]["strokeStyle"] =
        json!("DASHED");
    assert!(import_portable(&serde_json::to_vec(&f).unwrap())
        .project
        .is_none());
}

#[test]
fn passive_symbol_and_single_column_grid() {
    let mut f = fixture(7);
    f["variables"]["label-a"]["type"] = json!("BOOL");
    f["variables"]["fault"] = json!({"id":"fault","path":"Fault","type":"BOOL"});
    let c = &mut f["screens"]["screen/a"]["components"]["label-a"];
    c["type"] = json!("motor");
    c["properties"] = json!({});
    c["bindings"] = json!({"running":"label-a","fault":"fault"});
    f["screens"]["screen/a"]["layout"] = json!({"root":{"node":"stack","id":"outer","direction":"GRID","columns":1,"children":[{"node":"component","componentId":"label-a"}]}});
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    let p = a.project.unwrap_or_else(|| panic!("{:?}", a.report));
    let d = serde_json::to_value(&p).unwrap();
    assert_eq!(d["screens"][0]["root"]["element"]["type"], "column");
    assert_eq!(
        d["v2"]["components"]["definitions"][0]["classification"]["symbol"],
        "motor"
    );
    assert_eq!(
        d["screens"][0]["root"]["children"][0]["element"]["definition"],
        d["components"][0]["id"]
    );
    f["screens"]["screen/a"]["components"]["label-a"]["bindings"]["start"] = json!("fault");
    assert!(import_portable(&serde_json::to_vec(&f).unwrap())
        .project
        .is_none());
}

#[test]
fn root_adaptive_and_grid_design_frame() {
    let mut f = fixture(7);
    let s = &mut f["screens"]["screen/a"];
    s["components"]["label-b"] =
        json!({"id":"label-b","type":"label","properties":{"text":"Second"}});
    s["layout"] = json!({"root":{"node":"stack","id":"outer","direction":"ADAPTIVE","children":[{"node":"component","componentId":"label-a"},{"node":"component","componentId":"label-b"}]}});
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    let p = a.project.unwrap_or_else(|| panic!("{:?}", a.report));
    let d = serde_json::to_value(&p).unwrap();
    assert_eq!(d["screens"][0]["root"]["element"]["type"], "row");
    f["screens"]["screen/a"]["widthDp"] = json!(300);
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    let p = a.project.unwrap_or_else(|| panic!("{:?}", a.report));
    let d = serde_json::to_value(&p).unwrap();
    assert_eq!(d["screens"][0]["root"]["element"]["type"], "column");
    f["screens"]["screen/a"]["widthDp"] = json!(640);
    f["screens"]["screen/a"]["layout"]["root"]["direction"] = json!("GRID");
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    let p = a.project.unwrap_or_else(|| panic!("{:?}", a.report));
    let d = serde_json::to_value(&p).unwrap();
    let root = &d["screens"][0]["root"];
    assert_eq!(root["element"]["type"], "grid");
    assert_eq!(root["element"]["columns"].as_array().unwrap().len(), 2);
    assert_eq!(root["children"][1]["layout"]["gridCell"]["column"], 1);
    assert!(a.report.findings.iter().any(|x| x.severity == "warning"));
    f["screens"]["screen/a"]["layout"]["root"] = json!({"node":"scaleGroup","id":"g","componentIds":["label-a","label-b"],"designWidthDp":640,"designHeightDp":480});
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    assert!(a.project.is_none());
    assert!(a
        .report
        .findings
        .iter()
        .any(|x| { x.code == "scale-group-uniform-scale-and-reflow-not-representable" }));
}

#[test]
fn passive_pipe_and_tank_slot_conversion() {
    for (kind, slot, target, ty) in [
        ("pipe", "flow", "flowing", "BOOL"),
        ("tank", "level", "level", "DOUBLE"),
    ] {
        let mut f = fixture(7);
        f["variables"]["label-a"]["type"] = json!(ty);
        let c = &mut f["screens"]["screen/a"]["components"]["label-a"];
        c["type"] = json!(kind);
        c["properties"] = json!({});
        c["bindings"] = json!({});
        c["bindings"][slot] = json!("label-a");
        let a = import_portable(&serde_json::to_vec(&f).unwrap());
        let p = a.project.unwrap_or_else(|| panic!("{:?}", a.report));
        let d = serde_json::to_value(&p).unwrap();
        assert_eq!(
            d["screens"][0]["root"]["children"][0]["element"]["parameters"][target],
            d["tags"][0]["id"]
        );
    }
}

#[test]
fn typed_boolean_action_policy_and_pulse_agreement() {
    let mut f = industrial_fixture();
    f["schemaVersion"] = json!(10);
    f["variables"]["label-a"]["type"] = json!("BOOL");
    f["variables"]["label-a"]["access"] = json!("COMMAND");
    f["externalMappings"]["map-a"]["command"] = json!({"kind":"MAINTAINED"});
    let c = &mut f["screens"]["screen/a"]["components"]["label-a"];
    c["type"] = json!("button");
    c["actions"] = json!({"press":[{"action":"value.set","variableId":"label-a","value":{"valueType":"bool","value":true}},{"action":"value.toggle","variableId":"label-a"}]});
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    let p = a.project.unwrap_or_else(|| panic!("{:?}", a.report));
    let d = serde_json::to_value(&p).unwrap();
    let actions = &d["v2"]["components"]["handlers"][0]["actions"];
    assert_eq!(actions[0]["type"], "writeTag");
    assert_eq!(actions[0]["target"]["tag"], d["tags"][0]["id"]);
    assert_eq!(actions[1]["type"], "toggleTag");
    f["externalMappings"]["map-a"]["command"] = json!({"kind":"MOMENTARY","pulseMs":300});
    f["screens"]["screen/a"]["components"]["label-a"]["actions"] =
        json!({"press":{"action":"value.pulse","variableId":"label-a","ms":300}});
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    assert!(a.project.is_some(), "{:?}", a.report);
    f["screens"]["screen/a"]["components"]["label-a"]["actions"]["press"]["ms"] = json!(500);
    assert!(import_portable(&serde_json::to_vec(&f).unwrap())
        .project
        .is_none());
}

#[test]
fn passive_faceplate_declaration_and_direct_placement_refusal() {
    let mut f = fixture(7);
    f["componentDefinitions"]["fp"] = json!({"id":"fp","displayName":"Faceplate","type":"composite","kind":"FACEPLATE","composition":{"elements":{"caption":{"id":"caption","type":"label","properties":{"text":"Status"}}}}});
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    let p = a.project.unwrap_or_else(|| panic!("{:?}", a.report));
    let d = serde_json::to_value(&p).unwrap();
    let class = &d["v2"]["components"]["definitions"][0];
    assert_eq!(class["classification"]["kind"], "faceplate");
    assert_eq!(class["definition"], d["components"][0]["id"]);
    let c = &mut f["screens"]["screen/a"]["components"]["label-a"];
    c["type"] = json!("composite");
    c["componentDefinitionId"] = json!("fp");
    c["properties"] = json!({});
    assert!(import_portable(&serde_json::to_vec(&f).unwrap())
        .project
        .is_none());
}
