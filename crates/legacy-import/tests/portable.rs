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
        assert!(a.report.has_complete_mapping(), "{:?}", a.report);
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
    f["externalMappings"]["map-a"] = json!({"id":"map-a","internalVariableId":"label-a","sourceId":"src/a","externalAddress":"DB1.DINT0","access":"READ_WRITE","writeConfirmed":true,"command":{"kind":"SETPOINT","min":-2147483648i64,"max":2147483647i64,"step":2,"confirm":false}});
    f["externalMappings"]["map-b"] = json!({"id":"map-b","internalVariableId":"second","sourceId":"src/a","externalAddress":"DB1.DINT8","access":"READ"});
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
    assert_eq!(i["tags"][0]["limits"]["min"], json!(-2147483648i64));
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
    f["externalMappings"]["map-a"]["externalAddress"] = json!("DB1.X0.0");
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
    f["externalMappings"]["map-a"]["externalAddress"] = json!("ns=2;i=100");
    f["externalMappings"]["map-b"]["externalAddress"] = json!("ns=2;i=101");
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
    f["externalMappings"]["map-a"]["externalAddress"] = json!("DB1.X0.0");
    f["variables"]["label-a"]["access"] = json!("COMMAND");
    f["variables"]["label-a"]["engineeringUnit"] = Value::Null;
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

#[test]
fn exact_numeric_constant_action_and_policy_bounds() {
    let mut f = industrial_fixture();
    f["schemaVersion"] = json!(10);
    f["externalSources"]["src/a"] = json!({"id":"src/a","driver":"opcua","configuration":{"endpoint":"opc.tcp://example.invalid:4840","securityPolicy":"None","securityMode":"None"}});
    f["externalMappings"]["map-a"]["externalAddress"] = json!("ns=2;i=100");
    f["externalMappings"]["map-b"]["externalAddress"] = json!("ns=2;i=101");
    f["externalMappings"]["map-a"]["command"] =
        json!({"kind":"SETPOINT","min":-9007199254740992i64,"max":9007199254740992i64});
    let c = &mut f["screens"]["screen/a"]["components"]["label-a"];
    c["type"] = json!("button");
    c["actions"] = json!({"press":{"action":"value.set","variableId":"label-a","value":{"valueType":"long","value":9007199254740991i64}}});
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    let p = a.project.unwrap_or_else(|| panic!("{:?}", a.report));
    let d = serde_json::to_value(&p).unwrap();
    assert_eq!(
        d["v2"]["components"]["handlers"][0]["actions"][0]["value"],
        json!(9007199254740991i64)
    );
    f["screens"]["screen/a"]["components"]["label-a"]["actions"]["press"]["value"]["value"] =
        json!(9007199254740993i64);
    assert!(import_portable(&serde_json::to_vec(&f).unwrap())
        .project
        .is_none());
}

#[test]
fn cardinal_arrow_and_distinct_fill_refusal() {
    let mut f = fixture(7);
    let c = &mut f["screens"]["screen/a"]["components"]["label-a"];
    c["type"] = json!("arrow");
    c["properties"] =
        json!({"direction":"left","strokeColor":"#123456","fillColor":"#123456","strokeWidth":"2"});
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    let p = a.project.unwrap_or_else(|| panic!("{:?}", a.report));
    let d = serde_json::to_value(&p).unwrap();
    let contract = &d["v2"]["components"]["components"][0]["contract"];
    assert_eq!(contract["kind"], "arrow");
    assert!(contract["start"]["x"].as_f64() > contract["end"]["x"].as_f64());
    f["screens"]["screen/a"]["components"]["label-a"]["properties"]["fillColor"] = json!("#FFFFFF");
    assert!(import_portable(&serde_json::to_vec(&f).unwrap())
        .project
        .is_none());
}

#[test]
fn typed_dropdown_and_policy_bounds() {
    let mut f = industrial_fixture();
    f["schemaVersion"] = json!(10);
    f["externalMappings"]["map-a"]["command"] = json!({"kind":"SETPOINT","min":0,"max":3});
    f["enumerations"]["modes"] = json!({"id":"modes","name":"Modes","entries":[{"value":0,"text":"Off"},{"value":1,"text":"On"}]});
    let c = &mut f["screens"]["screen/a"]["components"]["label-a"];
    c["type"] = json!("dropDown");
    c["properties"] = json!({"label":"Mode","enumeration":"modes"});
    c["bindings"] = json!({"value":"label-a"});
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    let p = a.project.unwrap_or_else(|| panic!("{:?}", a.report));
    let d = serde_json::to_value(&p).unwrap();
    let contract = &d["v2"]["components"]["components"][0]["contract"];
    assert_eq!(contract["kind"], "dropDown");
    assert_eq!(contract["tag"], d["tags"][0]["id"]);
    assert_eq!(
        contract["enumeration"],
        d["v2"]["bindings"]["enumerations"][0]["id"]
    );
    f["enumerations"]["modes"]["entries"][1]["value"] = json!(4);
    assert!(import_portable(&serde_json::to_vec(&f).unwrap())
        .project
        .is_none());
}

#[test]
fn png_resource_transfer_is_atomic_and_integrity_checked() {
    use sha2::{Digest, Sha256};
    let mut f = fixture(7);
    let data = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+ip1sAAAAASUVORK5CYII=";
    let bytes = [
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 4,
        0, 0, 0, 181, 28, 12, 2, 0, 0, 0, 11, 73, 68, 65, 84, 120, 218, 99, 252, 255, 31, 0, 3, 3,
        2, 0, 239, 162, 167, 91, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];
    let hash = format!("{:x}", Sha256::digest(bytes));
    f["assets"]["a"] = json!({"id":"a","name":"Original synthetic pixel","kind":"PNG","sha256":hash,"sizeBytes":bytes.len(),"widthPx":1,"heightPx":1,"data":data});
    let c = &mut f["screens"]["screen/a"]["components"]["label-a"];
    c["type"] = json!("image");
    c["properties"] = json!({"assetId":"a","fit":"CONTAIN"});
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    let p = a.project.unwrap_or_else(|| panic!("{:?}", a.report));
    assert_eq!(p.resources.len(), 1);
    assert_eq!(a.resource_bytes[&p.resources[0].path], bytes);
    f["componentDefinitions"]["def"] = json!({"id":"def","displayName":"Reusable","type":"composite",
        "composition":{"elements":{"part":{"id":"part","type":"image","properties":{"assetId":"a","fit":"COVER"}}}}});
    let imported = import_portable(&serde_json::to_vec(&f).unwrap());
    let project = imported
        .project
        .unwrap_or_else(|| panic!("{:?}", imported.report));
    let doc = serde_json::to_value(&project).unwrap();
    let props = &doc["components"][0]["root"]["children"][0]["properties"];
    assert_eq!(imported.report.importer_version, "0.1.2");
    let size = imported
        .report
        .findings
        .iter()
        .find(|f| f.path == "/assets/a/sizeBytes")
        .unwrap();
    assert_eq!(
        doc.pointer(size.target.as_deref().unwrap()),
        Some(&json!(bytes.len()))
    );
    assert_eq!(props["source"], doc["resources"][0]["id"]);
    assert_eq!(props["contentScale"], "crop");
    assert_eq!(imported.resource_bytes.len(), 1);
    for component in [
        "/screens/screen~1a/components/label-a",
        "/componentDefinitions/def/composition/elements/part",
    ] {
        let mut default_fit = f.clone();
        default_fit.pointer_mut(component).unwrap()["properties"]
            .as_object_mut()
            .unwrap()
            .remove("fit");
        let imported = import_portable(&serde_json::to_vec(&default_fit).unwrap());
        let project = imported
            .project
            .unwrap_or_else(|| panic!("{:?}", imported.report));
        let doc = serde_json::to_value(project).unwrap();
        let target = imported
            .report
            .findings
            .iter()
            .find(|f| f.path == format!("{component}/properties/assetId"))
            .and_then(|f| f.target.as_deref())
            .and_then(|p| p.strip_suffix("/source"))
            .unwrap();
        assert_eq!(
            doc.pointer(&format!("{target}/contentScale")),
            Some(&json!("fit"))
        );
        for (field, value, code) in [
            (
                "properties/assetId",
                json!("missing"),
                "missing-image-asset",
            ),
            (
                "properties/assetId",
                json!(false),
                "invalid-image-asset-reference",
            ),
            ("properties/fit", json!(false), "invalid-image-fit"),
            ("properties/fit", json!("unknown"), "invalid-image-fit"),
            ("type", json!("svgImage"), "image-asset-kind-mismatch"),
        ] {
            let mut bad = f.clone();
            *bad.pointer_mut(&format!("{component}/{field}")).unwrap() = value;
            let result = import_portable(&serde_json::to_vec(&bad).unwrap());
            let field = if field == "type" {
                "properties/assetId"
            } else {
                field
            };
            let path = format!("{component}/{field}");
            assert!(result.project.is_none());
            assert!(result.resource_bytes.is_empty());
            assert!(result
                .report
                .findings
                .iter()
                .any(|f| f.code == code && f.path == path));
        }
    }
    f["assets"]["a"]["sizeBytes"] = json!(1);
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    assert!(a.project.is_none());
    assert!(a.resource_bytes.is_empty());
}

#[test]
fn explicit_theme_token_and_unsupported_cascade() {
    let mut f = fixture(7);
    f["theme"] =
        json!({"id":"custom-theme","name":"Custom","dark":true,"tokens":{"vessel":"#123456"}});
    let c = &mut f["screens"]["screen/a"]["components"]["label-a"];
    c["type"] = json!("rectangle");
    c["properties"] =
        json!({"fillColor":"@vessel","strokeColor":"#FFFFFF","strokeWidth":"2","cornerRadius":"0"});
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    let p = a.project.unwrap_or_else(|| panic!("{:?}", a.report));
    let d = serde_json::to_value(&p).unwrap();
    assert_eq!(d["themes"][0]["colors"]["vessel"], "#123456");
    assert_eq!(d["settings"]["defaultTheme"], d["themes"][0]["id"]);
    assert_eq!(
        d["screens"][0]["root"]["children"][0]["properties"]["fill"],
        json!({"token":"vessel"})
    );
    f["theme"]["componentDefaults"]["rectangle"] = json!({"id":"default","strokeStyle":"DASHED"});
    assert!(import_portable(&serde_json::to_vec(&f).unwrap())
        .project
        .is_none());
}

#[test]
fn baseline_jpeg_resource_and_trailing_bytes_refusal() {
    let mut f = fixture(7);
    let data = "/9j/4AAQSkZJRgABAQAAAQABAAD/2wBDAAgGBgcGBQgHBwcJCQgKDBQNDAsLDBkSEw8UHRofHh0aHBwgJC4nICIsIxwcKDcpLDAxNDQ0Hyc5PTgyPC4zNDL/2wBDAQkJCQwLDBgNDRgyIRwhMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjL/wAARCAADAAIDASIAAhEBAxEB/8QAHwAAAQUBAQEBAQEAAAAAAAAAAAECAwQFBgcICQoL/8QAtRAAAgEDAwIEAwUFBAQAAAF9AQIDAAQRBRIhMUEGE1FhByJxFDKBkaEII0KxwRVS0fAkM2JyggkKFhcYGRolJicoKSo0NTY3ODk6Q0RFRkdISUpTVFVWV1hZWmNkZWZnaGlqc3R1dnd4eXqDhIWGh4iJipKTlJWWl5iZmqKjpKWmp6ipqrKztLW2t7i5usLDxMXGx8jJytLT1NXW19jZ2uHi4+Tl5ufo6erx8vP09fb3+Pn6/8QAHwEAAwEBAQEBAQEBAQAAAAAAAAECAwQFBgcICQoL/8QAtREAAgECBAQDBAcFBAQAAQJ3AAECAxEEBSExBhJBUQdhcRMiMoEIFEKRobHBCSMzUvAVYnLRChYkNOEl8RcYGRomJygpKjU2Nzg5OkNERUZHSElKU1RVVldYWVpjZGVmZ2hpanN0dXZ3eHl6goOEhYaHiImKkpOUlZaXmJmaoqOkpaanqKmqsrO0tba3uLm6wsPExcbHyMnK0tPU1dbX2Nna4uPk5ebn6Onq8vP09fb3+Pn6/9oADAMBAAIRAxEAPwD3+iiigD//2Q==";
    let hash = "8579c1034f2eddd87ed11c521b7ac944c86802a1f5f3d8b9fc3b28cdb1d71520";
    f["assets"]["jpeg"] = json!({"id":"jpeg","name":"Synthetic white JPEG","kind":"JPEG","sha256":hash,"sizeBytes":631,"widthPx":2,"heightPx":3,"data":data});
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    let p = a.project.unwrap_or_else(|| panic!("{:?}", a.report));
    assert_eq!(p.resources.len(), 1);
    assert_eq!(p.resources[0].media_type.as_deref(), Some("image/jpeg"));
    assert_eq!(a.resource_bytes[&p.resources[0].path].len(), 631);
    let asset = &mut f["assets"]["jpeg"];
    asset["data"] = json!(format!("{}AA", data.trim_end_matches('=')));
    asset["sizeBytes"] = json!(633);
    asset["sha256"] = json!("e9f7d2d559ba28fb1777335ba534bf9707e293040bb14284d5823e4e013c1e27");
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    assert!(a.project.is_none());
    assert!(a.resource_bytes.is_empty());
}

#[test]
fn restricted_svg_transfer_and_active_content_refusal() {
    use sha2::{Digest, Sha256};
    let mut f = fixture(7);
    let data = r##"<svg xmlns="http://www.w3.org/2000/svg" width="2" height="3"><rect x="0" y="0" width="2" height="3" fill="#112233"/></svg>"##;
    let hash = format!("{:x}", Sha256::digest(data.as_bytes()));
    f["assets"]["vector"] = json!({"id":"vector","name":"Synthetic rectangle","kind":"SVG","sha256":hash,"sizeBytes":data.len(),"widthPx":2,"heightPx":3,"data":data});
    let c = &mut f["screens"]["screen/a"]["components"]["label-a"];
    c["type"] = json!("svgImage");
    c["properties"] = json!({"assetId":"vector","fit":"CONTAIN"});
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    let p = a.project.unwrap_or_else(|| panic!("{:?}", a.report));
    assert_eq!(p.resources[0].media_type.as_deref(), Some("image/svg+xml"));
    assert_eq!(a.resource_bytes[&p.resources[0].path], data.as_bytes());
    let bad = data.replace("<rect", "<rect onclick=\"alert(1)\"");
    let asset = &mut f["assets"]["vector"];
    asset["sha256"] = json!(format!("{:x}", Sha256::digest(bad.as_bytes())));
    asset["sizeBytes"] = json!(bad.len());
    asset["data"] = json!(bad);
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    assert!(a.project.is_none());
    assert!(a.resource_bytes.is_empty());
}

#[test]
fn alarm_declaration_is_review_only_and_blocks_project() {
    let mut f = fixture(10);
    f["alarms"]["high"] = json!({"id":"high","sourceVariableId":"label-a","message":"Synthetic alarm","severity":"HIGH"});
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    assert!(a.project.is_none());
    assert!(a.resource_bytes.is_empty());
    assert_eq!(a.review_only_alarms.len(), 1);
    let alarm = &a.review_only_alarms[0];
    assert_eq!(alarm.source_variable_id, "label-a");
    assert_eq!(alarm.message, "Synthetic alarm");
    assert_eq!(alarm.severity, "HIGH");
    assert!(a.report.findings.iter().any(|x| {
        x.path == "/alarms/high/message" && x.code == "legacy-alarm-activation-not-representable"
    }));
    assert!(!serde_json::to_string(&a.report)
        .unwrap()
        .contains("Synthetic alarm"));
    f["alarms"]["high"]["sourceVariableId"] = json!("missing");
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    assert!(a.project.is_none());
    assert!(a.review_only_alarms.is_empty());
}

#[test]
fn named_style_default_and_component_override_precedence() {
    let mut f = fixture(7);
    f["theme"] = json!({"id":"custom-theme","name":"Custom","styles":{"vessel":{"id":"vessel","fillColor":"#222222","strokeWidthDp":3}},"componentDefaults":{"rectangle":{"id":"default","fillColor":"#111111","strokeColor":"#333333","strokeWidthDp":2,"cornerRadiusDp":4}}});
    let c = &mut f["screens"]["screen/a"]["components"]["label-a"];
    c["type"] = json!("rectangle");
    c["properties"] = json!({"styleId":"vessel","fillColor":"#444444"});
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    let p = a.project.unwrap_or_else(|| panic!("{:?}", a.report));
    assert_eq!(p.styles.len(), 2);
    let props = &p.screens[0].root.children[0].properties;
    assert_eq!(props["fill"], "#444444");
    assert_eq!(props["strokeColor"], "#333333");
    assert_eq!(props["strokeWidth"].as_f64(), Some(3.));
    assert_eq!(props["cornerRadius"].as_f64(), Some(4.));
    f["screens"]["screen/a"]["components"]["label-a"]["properties"]["styleId"] = json!("unknown");
    assert!(import_portable(&serde_json::to_vec(&f).unwrap())
        .project
        .is_none());
}

#[test]
fn bounded_integer_step_and_invalid_clamps() {
    let mut f = industrial_fixture();
    f["schemaVersion"] = json!(10);
    f["variables"]["label-a"]["type"] = json!("INT");
    f["externalMappings"]["map-a"]["command"] = json!({"kind":"SETPOINT","min":0,"max":100});
    let c = &mut f["screens"]["screen/a"]["components"]["label-a"];
    c["type"] = json!("button");
    c["actions"] =
        json!({"press":{"action":"value.step","variableId":"label-a","by":-2,"min":0,"max":100}});
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    let p = a.project.unwrap_or_else(|| panic!("{:?}", a.report));
    let d = serde_json::to_value(&p).unwrap();
    let action = &d["v2"]["components"]["handlers"][0]["actions"][0];
    assert_eq!(action["type"], "stepTag");
    assert_eq!(action["step"].as_f64(), Some(-2.));
    assert_eq!(action["min"].as_f64(), Some(0.));
    assert_eq!(action["max"].as_f64(), Some(100.));
    for (key, value) in [
        ("by", json!(0)),
        ("by", json!(0.5)),
        ("min", Value::Null),
        ("min", json!(-1)),
        ("max", json!(101)),
    ] {
        let mut bad = f.clone();
        bad["screens"]["screen/a"]["components"]["label-a"]["actions"]["press"][key] = value;
        assert!(import_portable(&serde_json::to_vec(&bad).unwrap())
            .project
            .is_none());
    }
}

#[test]
fn passive_thermal_value_and_additional_status_refusal() {
    let mut f = fixture(7);
    f["variables"]["label-a"]["type"] = json!("DOUBLE");
    let c = &mut f["screens"]["screen/a"]["components"]["label-a"];
    c["type"] = json!("thermalIndicator");
    c["properties"] = json!({});
    c["bindings"] = json!({"value":"label-a"});
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    let p = a.project.unwrap_or_else(|| panic!("{:?}", a.report));
    let d = serde_json::to_value(&p).unwrap();
    assert_eq!(
        d["screens"][0]["root"]["children"][0]["element"]["parameters"]["temperature"],
        d["tags"][0]["id"]
    );
    f["screens"]["screen/a"]["components"]["label-a"]["bindings"]["active"] = json!("label-a");
    assert!(import_portable(&serde_json::to_vec(&f).unwrap())
        .project
        .is_none());
}

#[test]
fn screen_lifecycle_navigation_is_refused() {
    let mut f = fixture(10);
    f["screens"]["screen/a"]["actions"] = json!({"open":{"action":"navigate.startup"}});
    assert!(import_portable(&serde_json::to_vec(&f).unwrap())
        .project
        .is_none());
}

#[test]
fn unsupported_contract_review_is_separate_and_value_free_report() {
    let mut f = fixture(10);
    f["variables"]["label-a"]["type"] = json!("STRING");
    f["componentDefinitions"]["fp"] = json!({"id":"fp","type":"composite","kind":"FACEPLATE"});
    let c = &mut f["screens"]["screen/a"]["components"]["label-a"];
    c["type"] = json!("textInput");
    c["properties"] = json!({"label":"Local review text","maxLength":"40"});
    c["bindings"] = json!({"value":"label-a"});
    c["actions"] = json!({"press":{"action":"open.faceplate","definitionId":"fp","modal":true,"parameters":{"destination":"source"}}});
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    assert!(a.project.is_none());
    assert!(a.resource_bytes.is_empty());
    assert_eq!(a.review_only_contracts.len(), 2);
    assert_eq!(a.review_only_contracts[0].kind, "text-input-write-policy");
    assert_eq!(
        a.review_only_contracts[1].declaration["parameters"]["destination"],
        "source"
    );
    assert!(!format!("{a:?}").contains("Local review text"));
    assert!(!serde_json::to_string(&a.report)
        .unwrap()
        .contains("Local review text"));
    f["screens"]["screen/a"]["components"]["label-a"]["properties"]["password"] =
        json!("SECRET-TEST");
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    assert!(a.project.is_none());
    assert_eq!(a.review_only_contracts.len(), 1);
    assert!(!format!("{a:?}").contains("SECRET-TEST"));
}

#[test]
fn writable_shared_endpoint_and_debug_redaction() {
    let mut f = industrial_fixture();
    f["externalSources"]["other"] = f["externalSources"]["src/a"].clone();
    f["externalSources"]["other"]["id"] = json!("other");
    f["externalSources"]["other"]["configuration"]["host"] = json!("EXAMPLE.INVALID");
    f["externalMappings"]["map-b"]["sourceId"] = json!("other");
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    assert!(a.project.is_none());
    assert!(a.resource_bytes.is_empty());
    assert!(a
        .report
        .findings
        .iter()
        .any(|x| x.code == "ambiguous-writable-shared-endpoint"));
    let f = industrial_fixture();
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    assert!(a.project.is_some(), "{:?}", a.report);
    let debug = format!("{a:?}");
    for private in ["example.invalid", "DB1.DINT0", "Synthetic source", "role/a"] {
        assert!(!debug.contains(private));
    }
}

#[test]
fn legacy_default_operate_and_mixed_permission_refusal() {
    let mut f = industrial_fixture();
    f["schemaVersion"] = json!(10);
    f["externalMappings"]["map-a"]["command"] = json!({"kind":"SETPOINT","min":0,"max":100});
    let c = &mut f["screens"]["screen/a"]["components"]["label-a"];
    c["type"] = json!("button");
    c["requiredPermission"] = Value::Null;
    c["actions"] = json!({"press":{"action":"value.set","variableId":"label-a","value":{"valueType":"long","value":2}}});
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    let p = a.project.unwrap_or_else(|| panic!("{:?}", a.report));
    let d = serde_json::to_value(&p).unwrap();
    assert_eq!(
        d["v2"]["industrial"]["nodes"][0]["requiredPermission"],
        "operate"
    );
    f["screens"]["screen/a"]["components"]["label-a"]["actions"]["release"] =
        json!({"action":"navigate.back"});
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    assert!(a.project.is_none());
    assert!(a
        .report
        .findings
        .iter()
        .any(|x| x.code == "mixed-implicit-command-permission"));
}

#[test]
fn absent_startup_uses_admitted_screen_order() {
    let mut f = fixture(1);
    f["metadata"]["defaultScreenId"] = Value::Null;
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    let p = a.project.unwrap_or_else(|| panic!("{:?}", a.report));
    assert_eq!(p.navigation.start_screen, p.screens[0].id);
    f["metadata"]["defaultScreenId"] = json!("missing");
    assert!(import_portable(&serde_json::to_vec(&f).unwrap())
        .project
        .is_none());
}

#[test]
fn typed_s7_address_bounds_and_overlap() {
    let f = industrial_fixture();
    for address in [
        "DB1.DBD0",
        "DB1.REAL0",
        "DB0.DINT0",
        "DB1.DINT65535",
        "DB1.DINT4294967295",
        "é",
        "DB1.DINT2",
        "db01.dint0",
    ] {
        let mut bad = f.clone();
        bad["externalMappings"]["map-b"]["externalAddress"] = json!(address);
        let a = import_portable(&serde_json::to_vec(&bad).unwrap());
        assert!(a.project.is_none(), "{address}");
    }
    let mut good = f;
    good["externalMappings"]["map-b"]["externalAddress"] = json!("DB1.DINT4");
    assert!(import_portable(&serde_json::to_vec(&good).unwrap())
        .project
        .is_some());
}

#[test]
fn opcua_node_identity_and_invalid_address_refusal() {
    let mut f = industrial_fixture();
    f["externalSources"]["src/a"] = json!({"id":"src/a","driver":"opcua","configuration":{"endpoint":"opc.tcp://example.invalid:4840","securityPolicy":"None","securityMode":"None"}});
    f["externalMappings"]["map-a"]["externalAddress"] = json!("ns=2;i=100");
    f["externalMappings"]["map-b"]["externalAddress"] = json!("ns=2;s=Other");
    assert!(import_portable(&serde_json::to_vec(&f).unwrap())
        .project
        .is_some());
    for address in [
        "ns=02;i=0100",
        "ns=65536;i=100",
        "ns=2;i=4294967296",
        "ns=2;s=",
        "invalid",
    ] {
        f["externalMappings"]["map-b"]["externalAddress"] = json!(address);
        assert!(
            import_portable(&serde_json::to_vec(&f).unwrap())
                .project
                .is_none(),
            "{address}"
        );
    }
}

#[test]
fn source_version_permission_and_input_gates() {
    let mut f = fixture(7);
    f["screens"]["screen/a"]["components"]["label-a"]["requiredPermission"] = json!("operate");
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    assert!(a.project.is_none());
    assert!(a
        .report
        .findings
        .iter()
        .any(|x| x.code == "source-feature-version-mismatch"));
    let mut f = fixture(9);
    f["screens"]["screen/a"]["components"]["label-a"]["type"] = json!("textInput");
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    assert!(a.project.is_none());
    assert!(a
        .report
        .findings
        .iter()
        .any(|x| x.code == "source-feature-version-mismatch"));
}

#[test]
fn exact_standard_label_catalog_signature() {
    let mut f = fixture(1);
    f["componentDefinitions"]["standard.label"] = json!({"id":"standard.label","displayName":"Label","type":"label","bindingSlots":{},"defaultProperties":{"text":"Label"}});
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    let p = a.project.unwrap_or_else(|| panic!("{:?}", a.report));
    let d = serde_json::to_value(&p).unwrap();
    assert_eq!(
        d["extensions"]["hmiforge.standardCatalog"]["standard.label"]["id"],
        "standard.label"
    );
    f["componentDefinitions"]["standard.label"]["defaultProperties"]["text"] = json!("Modified");
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    assert!(a.project.is_none());
    assert!(a
        .report
        .findings
        .iter()
        .any(|x| x.code == "standard-catalog-signature-mismatch"));
}

#[test]
fn destination_audit_reports_only_source_paths() {
    let f = fixture(1);
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    let p = a.project.unwrap_or_else(|| panic!("{:?}", a.report));
    let d = serde_json::to_value(&p).unwrap();
    for finding in &a.report.findings {
        let unresolved = finding
            .target
            .as_deref()
            .is_none_or(|t| d.pointer(t).is_none());
        assert_eq!(
            a.report.unresolved_targets.contains(&finding.path),
            unresolved
        );
    }
    assert!(!a
        .report
        .unresolved_targets
        .contains(&"/screens/screen~1a/components/label-a/properties/text".to_owned()));
}

#[test]
fn catalog_signature_rejects_changed_slot_semantics() {
    let mut f = fixture(1);
    f["componentDefinitions"]["standard.button"] = json!({"id":"standard.button","displayName":"Button","type":"button","bindingSlots":{"command":{"role":"command","acceptedTypes":["BOOL"],"required":false,"writable":true}},"defaultProperties":{"text":"Button"}});
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    assert!(a.project.is_some(), "{:?}", a.report);
    f["componentDefinitions"]["standard.button"]["bindingSlots"]["command"]["writable"] =
        json!(false);
    assert!(import_portable(&serde_json::to_vec(&f).unwrap())
        .project
        .is_none());
}

#[test]
fn physical_s7_integer_limits_and_bit_shorthand() {
    let mut f = industrial_fixture();
    f["externalMappings"]["map-a"]["externalAddress"] = json!("DB1.INT0");
    assert!(import_portable(&serde_json::to_vec(&f).unwrap())
        .project
        .is_none());
    f["externalMappings"]["map-a"]["command"] = json!({"kind":"SETPOINT","min":-32768,"max":32767});
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    assert!(a.project.is_some(), "{:?}", a.report);
    f["externalMappings"]["map-a"]["command"]["max"] = json!(32768);
    assert!(import_portable(&serde_json::to_vec(&f).unwrap())
        .project
        .is_none());
    f["externalMappings"]["map-a"]["externalAddress"] = json!("M0.0");
    f["variables"]["label-a"]["type"] = json!("BOOL");
    f["variables"]["label-a"]["access"] = json!("COMMAND");
    f["variables"]["label-a"]["engineeringUnit"] = Value::Null;
    f["externalMappings"]["map-a"]["command"] = json!({"kind":"MAINTAINED"});
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    assert!(a.project.is_some(), "{:?}", a.report);
    f["variables"]["second"]["type"] = json!("BOOL");
    f["variables"]["second"]["engineeringUnit"] = Value::Null;
    f["externalMappings"]["map-b"]["externalAddress"] = json!("M.X0.1");
    assert!(import_portable(&serde_json::to_vec(&f).unwrap())
        .project
        .is_none());
}

#[test]
fn explicit_standard_reference_integrity() {
    let mut f = fixture(1);
    f["componentDefinitions"]["standard.label"] = json!({"id":"standard.label","displayName":"Label","type":"label","defaultProperties":{"text":"Label"}});
    f["screens"]["screen/a"]["components"]["label-a"]["componentDefinitionId"] =
        json!("standard.label");
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    assert!(a.project.is_some(), "{:?}", a.report);
    f["screens"]["screen/a"]["components"]["label-a"]["componentDefinitionId"] =
        json!("standard.button");
    assert!(import_portable(&serde_json::to_vec(&f).unwrap())
        .project
        .is_none());
}

#[test]
fn strict_report_gate_and_default_elision() {
    let f = fixture(1);
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    let p = a
        .project
        .as_ref()
        .unwrap_or_else(|| panic!("{:?}", a.report));
    let d = serde_json::to_value(p).unwrap();
    let mut expected = BTreeSet::new();
    paths(&f, "", &mut expected);
    assert_eq!(a.report.source_leaf_count, Some(expected.len()));
    assert!(a.report.destination_audit_completed);
    assert!(a.report.has_complete_mapping(), "{:?}", a.report);
    for finding in &a.report.findings {
        if let Some(original) = &finding.elided_target {
            assert!(d.pointer(original).is_none());
            assert!(d.pointer(finding.target.as_deref().unwrap()).is_some());
        }
    }
    let mut report = a.report.clone();
    report.unresolved_targets.push("/synthetic".into());
    assert!(!report.has_complete_mapping());
    report.unresolved_targets.clear();
    report.source_leaf_count = Some(expected.len() + 1);
    assert!(!report.has_complete_mapping());
    let mut f = f;
    f["unknown"] = json!("unexpected");
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    assert!(a.project.is_none());
    assert!(!a.report.has_complete_mapping());
}

#[test]
fn pre_v10_event_contract_is_rejected_before_mapping() {
    let mut f = fixture(5);
    f["screens"]["screen/a"]["components"]["label-a"]["type"] = json!("button");
    f["screens"]["screen/a"]["components"]["label-a"]["actions"] =
        json!({"release":{"action":"navigate.back"}});
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    assert!(a.project.is_none());
    assert!(a
        .report
        .findings
        .iter()
        .any(|f| f.code == "source-feature-version-mismatch"));
}

#[test]
fn full_pinned_catalog_round_trip_and_mutation_refusal() {
    let catalog: Value =
        serde_json::from_str(include_str!("fixtures/standard_catalog.json")).unwrap();
    let definitions = catalog.as_object().unwrap();
    assert_eq!(definitions.len(), 27);
    let mut f = fixture(10);
    f["componentDefinitions"] = catalog.clone();
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    let p = a.project.unwrap_or_else(|| panic!("{:?}", a.report));
    assert!(a.report.has_complete_mapping());
    let d = serde_json::to_value(&p).unwrap();
    assert_eq!(d["extensions"]["hmiforge.standardCatalog"], catalog);
    let bytes = save_project(&p).unwrap();
    let loaded = hmi_project_model::serialization::load_project(&bytes).unwrap();
    assert_eq!(serde_json::to_value(loaded.project).unwrap(), d);
    for key in definitions.keys() {
        let mut bad = fixture(10);
        bad["componentDefinitions"][key] = catalog[key].clone();
        bad["componentDefinitions"][key]["displayName"] = json!("Changed declaration");
        let a = import_portable(&serde_json::to_vec(&bad).unwrap());
        assert!(a.project.is_none(), "{key}");
        assert!(a
            .report
            .findings
            .iter()
            .any(|f| f.code == "standard-catalog-signature-mismatch"));
    }
}

#[test]
fn adaptive_destination_pointers_follow_stable_node_identity() {
    let mut f = fixture(3);
    let s = &mut f["screens"]["screen/a"];
    s["components"]["label-b"] =
        json!({"id":"label-b","type":"label","properties":{"text":"Second"}});
    s["layout"] = json!({"root":{"node":"stack","id":"outer","direction":"COLUMN","children":[{"node":"stack","id":"inner","direction":"ROW","children":[{"node":"component","componentId":"label-b"},{"node":"component","componentId":"label-a"}]}]}});
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    let p = a.project.unwrap_or_else(|| panic!("{:?}", a.report));
    let d = serde_json::to_value(&p).unwrap();
    for (id, value) in [("label-a", "Synthetic"), ("label-b", "Second")] {
        let path = format!("/screens/screen~1a/components/{id}/properties/text");
        let finding = a.report.findings.iter().find(|f| f.path == path).unwrap();
        let target = finding.target.as_deref().unwrap();
        assert_eq!(d.pointer(target), Some(&json!(value)));
    }
    assert!(a.report.has_complete_mapping());
}

#[test]
fn guarded_write_declarations_require_v9() {
    let mut f = industrial_fixture();
    f["schemaVersion"] = json!(8);
    f["externalSources"]["src/a"]["commissioningAllowed"] = json!(false);
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    assert!(a.project.is_none());
    assert!(a
        .report
        .findings
        .iter()
        .any(|f| f.code == "source-feature-version-mismatch"));
    f["schemaVersion"] = json!(9);
    let a = import_portable(&serde_json::to_vec(&f).unwrap());
    assert!(a.project.is_some(), "{:?}", a.report);
}

#[test]
fn every_version_feature_boundary() {
    let mut v2 = fixture(2);
    v2["screens"]["screen/a"]["layout"] = json!({"root":{"node":"stack","id":"root","direction":"COLUMN","children":[{"node":"component","componentId":"label-a"}]}});
    let mut v3 = v2.clone();
    v3["screens"]["screen/a"]["layout"]["root"]["direction"] = json!("GRID");
    v3["screens"]["screen/a"]["layout"]["root"]["columns"] = json!(1);
    let mut v4 = fixture(4);
    v4["variables"]["label-a"]["type"] = json!("STRING");
    v4["screens"]["screen/a"]["components"]["label-a"]["expressions"] =
        json!({"text":"`label-a` + '!' "});
    let mut v5 = fixture(5);
    v5["screenOrder"] = json!(["screen/a"]);
    let mut v6 = fixture(6);
    v6["componentDefinitions"]["def"] = json!({"id":"def","displayName":"Reusable","type":"composite","composition":{"elements":{"part":{"id":"part","type":"label","properties":{"text":"Inside"}}}}});
    let mut v7 = fixture(7);
    v7["theme"] =
        json!({"id":"custom-theme","name":"Custom","dark":true,"tokens":{"vessel":"#123456"}});
    let mut v8 = fixture(8);
    v8["security"] = json!({"roles":{},"permissions":{}});
    let mut v10 = fixture(10);
    v10["enumerations"]["mode"] =
        json!({"id":"mode","name":"Mode","entries":[{"value":0,"text":"Off"}]});
    for (version, mut project) in [
        (2, v2),
        (3, v3),
        (4, v4),
        (5, v5),
        (6, v6),
        (7, v7),
        (8, v8),
        (9, industrial_fixture()),
        (10, v10),
    ] {
        project["schemaVersion"] = json!(version);
        let a = import_portable(&serde_json::to_vec(&project).unwrap());
        assert!(a.project.is_some(), "v{version}: {:?}", a.report);
        assert!(a.report.has_complete_mapping());
        project["schemaVersion"] = json!(version - 1);
        let a = import_portable(&serde_json::to_vec(&project).unwrap());
        assert!(
            a.project.is_none(),
            "v{} admitted newer feature",
            version - 1
        );
        assert!(a.resource_bytes.is_empty());
        assert!(!a.report.has_complete_mapping());
    }
}

fn instrument_fixture(kind: &str) -> Value {
    let mut f = fixture(10);
    f["variables"]["label-a"]["type"] = json!("DOUBLE");
    let c = &mut f["screens"]["screen/a"]["components"]["label-a"];
    c["type"] = json!(kind);
    if kind == "gauge" {
        c["properties"] = json!({"min":"-12.5","max":"83.25","label":"Pressure","unit":"bar"});
        c["bindings"] = json!({"value":"label-a"});
        f["variables"]["label-a"]["engineeringUnit"] = json!("bar");
    } else {
        c["properties"] = json!({"min":"0","max":"100","label":"Level","unit":"%","orientation":"horizontal","fillColor":"#123456"});
        c["bindings"] = json!({"level":"label-a"});
    }
    f
}

#[test]
fn passive_instrument_mapping_is_deterministic_and_read_only() {
    for kind in ["gauge", "levelIndicator"] {
        let f = instrument_fixture(kind);
        let bytes = serde_json::to_vec(&f).unwrap();
        let a = import_portable(&bytes);
        let b = import_portable(&bytes);
        assert!(a.report.has_complete_mapping(), "{:?}", a.report);
        assert_eq!(a.report, b.report);
        assert_eq!(a.report.importer_version, "0.1.2");
        let project = a.project.unwrap();
        assert_eq!(
            save_project(&project).unwrap(),
            save_project(&b.project.unwrap()).unwrap()
        );
        let doc = serde_json::to_value(&project).unwrap();
        let n = &doc["screens"][0]["root"]["children"][0];
        let (target, min, max) = if kind == "gauge" {
            assert_eq!(n["properties"]["variant"], "radial");
            assert_eq!(n["properties"]["showUnit"], true);
            ("gauge", -12.5, 83.25)
        } else {
            assert_eq!(n["properties"]["orientation"], "horizontal");
            assert_eq!(n["properties"]["barColor"], "#123456");
            ("progressBar", 0.0, 100.0)
        };
        assert_eq!(n["element"]["kind"], target);
        assert_eq!(n["properties"]["min"].as_f64(), Some(min));
        assert_eq!(n["properties"]["max"].as_f64(), Some(max));
        assert_eq!(n["bindings"][0]["property"], "value");
        assert_eq!(n["bindings"][0]["source"]["tag"], doc["tags"][0]["id"]);
        assert!(n["bindings"][0]["mode"].is_null() || n["bindings"][0]["mode"] == "read");
        assert_eq!(
            n["extensions"]["hmiforge.instrumentPresentation"]["label"],
            f["screens"]["screen/a"]["components"]["label-a"]["properties"]["label"]
        );
        let mut expected = BTreeSet::new();
        paths(&f, "", &mut expected);
        assert_eq!(
            expected,
            a.report.findings.iter().map(|x| x.path.clone()).collect()
        );
        let report = serde_json::to_value(&a.report).unwrap();
        let replay = hmi_legacy_import::replay_portable(&bytes, &report).unwrap();
        assert_eq!(replay.report, a.report);
        assert_eq!(
            save_project(&replay.project.unwrap()).unwrap(),
            save_project(&project).unwrap()
        );
        for old in ["0.1.0", "0.1.1"] {
            let mut historical = report.clone();
            historical["importerVersion"] = json!(old);
            assert!(hmi_legacy_import::replay_portable(&bytes, &historical)
                .unwrap()
                .project
                .is_none());
        }
    }
}

#[test]
fn passive_instrument_refuses_unrepresentable_semantics() {
    let component = "/screens/screen~1a/components/label-a";
    for (kind, field, value) in [
        ("gauge", "properties/min", json!("NaN")),
        ("gauge", "properties/max", json!("-12.5")),
        ("gauge", "properties/max", json!("-13")),
        ("gauge", "properties/min", json!("-1e309")),
        ("gauge", "properties/highWarning", json!("75")),
        ("gauge", "properties/highFault", json!("95")),
        ("gauge", "properties/unit", json!("psi")),
        ("gauge", "properties/label", json!("{{template}}")),
        ("gauge", "bindings", json!({"value":"label-a","fault":"label-a"})),
        ("gauge", "bindings", json!({})),
        ("gauge", "expressions", json!({"value":"1"})),
        ("levelIndicator", "properties/orientation", json!("vertical")),
        ("levelIndicator", "properties/unit", json!("L")),
        ("levelIndicator", "properties/fillColor", json!("@level.fill")),
        ("levelIndicator", "bindings", json!({"value":"label-a"})),
    ] {
        let mut f = instrument_fixture(kind);
        let full = format!("{component}/{field}");
        let (parent, key) = full.rsplit_once('/').unwrap();
        f.pointer_mut(parent).unwrap()[key] = value;
        let a = import_portable(&serde_json::to_vec(&f).unwrap());
        assert!(a.project.is_none(), "{kind} {field}");
        assert!(a.resource_bytes.is_empty());
        assert!(!a.report.has_complete_mapping());
    }
    for ty in ["BOOL", "STRING"] {
        let mut f = instrument_fixture("gauge");
        f["variables"]["label-a"]["type"] = json!(ty);
        let a = import_portable(&serde_json::to_vec(&f).unwrap());
        assert!(a.project.is_none());
        assert!(a.resource_bytes.is_empty());
    }
    let mut f = instrument_fixture("gauge");
    f["variables"]["label-a"]["access"] = json!("READ_WRITE");
    assert!(import_portable(&serde_json::to_vec(&f).unwrap()).project.is_none());
    let mut f = instrument_fixture("levelIndicator");
    f["variables"]["label-a"]["engineeringUnit"] = json!("L");
    assert!(import_portable(&serde_json::to_vec(&f).unwrap()).project.is_none());
}

#[test]
fn passive_instrument_revision_preserves_predecessor_mapping() {
    let bytes = serde_json::to_vec(&fixture(1)).unwrap();
    let current = import_portable(&bytes);
    let canonical = save_project(current.project.as_ref().unwrap()).unwrap();
    for version in ["0.1.0", "0.1.1"] {
        let mut expected = serde_json::to_value(&current.report).unwrap();
        expected["importerVersion"] = json!(version);
        let replay = hmi_legacy_import::replay_portable(&bytes, &expected).unwrap();
        assert_eq!(serde_json::to_value(&replay.report).unwrap(), expected);
        assert_eq!(save_project(&replay.project.unwrap()).unwrap(), canonical);
    }
}
