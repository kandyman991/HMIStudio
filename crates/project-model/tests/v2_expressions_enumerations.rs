//! Original synthetic vectors, derived from the published contract behavior.
#![allow(clippy::unwrap_used)]
use hmi_project_model::expression::*;
use std::collections::BTreeMap;
mod common;
use common::*;
use hmi_project_model::model::v2::IndustrialSecurityDeclarations;
use hmi_project_model::model::v2_bindings::*;
use hmi_project_model::validation::validate_v2_bindings;
use serde_json::json;
fn snapshot() -> (BTreeMap<String, Type>, BTreeMap<String, Reading>) {
    let values = [
        ("test.active", Value::Bool(true)),
        ("test.stop", Value::Bool(false)),
        ("test.raw", Value::Number(250.0)),
        ("test.speed", Value::Number(12.5)),
        ("test.name", Value::Text("Unit A".into())),
        ("tag-test[0]", Value::Number(9.0)),
    ];
    let mut types = BTreeMap::new();
    let mut readings = BTreeMap::new();
    for (id, v) in values {
        let t = match v {
            Value::Bool(_) => Type::Bool,
            Value::Number(_) => Type::Number,
            Value::Text(_) => Type::Text,
        };
        types.insert(id.into(), t);
        readings.insert(
            id.into(),
            Reading {
                value: Some(v),
                quality: Quality::Good,
            },
        );
    }
    (types, readings)
}
#[test]
fn arithmetic_and_functions() {
    let (t, r) = snapshot();
    for (s, n) in [
        ("4+3*2", 10.0),
        ("(4+3)*2", 14.0),
        ("-(4+2)", -6.0),
        ("11%4", 3.0),
        ("7/2", 3.5),
        (".25e2", 25.0),
        ("scale(test.raw,0,500,0,100)", 50.0),
        ("round(test.speed)", 13.0),
        ("round(-test.speed)", -13.0),
        ("round(2.71828,2)", 2.72),
        ("floor(test.speed)", 12.0),
        ("ceil(test.speed)", 13.0),
        ("clamp(test.speed,0,10)", 10.0),
        ("max(test.speed,1)", 12.5),
        ("min(test.speed,1)", 1.0),
        ("abs(-4)", 4.0),
        ("`tag-test[0]`", 9.0),
    ] {
        let e = compile(s, &t).unwrap();
        assert_eq!(e.result_type(), Type::Number);
        let v = e.evaluate(&r);
        assert_eq!(v.value, Some(Value::Number(n)), "{s}");
        assert_eq!(v.quality, Quality::Good);
    }
}
#[test]
fn boolean_text_and_references() {
    let (t, r) = snapshot();
    for s in [
        "test.active && !test.stop",
        "test.stop || test.speed>10",
        "test.speed>=12.5 && test.speed<=12.5 && test.speed!=0 && test.speed==12.5",
        "test.name=='Unit A'",
        "test.active==true",
    ] {
        assert_eq!(
            compile(s, &t).unwrap().evaluate(&r).value,
            Some(Value::Bool(true)),
            "{s}"
        );
    }
    for (s, v) in [
        ("test.active ? 'ON' : 'OFF'", "ON"),
        (
            "test.stop ? 'OFF' : test.speed>10 ? 'FAST' : 'SLOW'",
            "FAST",
        ),
        ("test.name + ': ' + test.speed", "Unit A: 12.5"),
        ("text(4.0)", "4"),
        ("text(test.active)", "TRUE"),
        (r#""a\"b""#, "a\"b"),
        ("text(0.0001)", "1.0E-4"),
    ] {
        assert_eq!(
            compile(s, &t).unwrap().evaluate(&r).value,
            Some(Value::Text(v.into())),
            "{s}"
        );
    }
    let e = compile("test.active ? test.speed : `tag-test[0]` + test.speed", &t).unwrap();
    assert_eq!(
        e.references(),
        &["test.active", "test.speed", "tag-test[0]"]
    );
}
#[test]
fn errors_and_quality() {
    let (t, mut r) = snapshot();
    for s in [
        "",
        "1 +",
        "1.2.3",
        "foo(2)",
        "round(1,2,3)",
        "test.speed && test.active",
        "test.active + test.stop",
        "test.active ? 1 : 'x'",
        "a+b",
        "1=1",
        "test.raw # 1",
    ] {
        assert!(compile(s, &t).is_err(), "{s}");
    }
    assert_eq!(compile("a+b", &t).unwrap_err().len(), 2);
    for s in [
        "test.raw/0",
        "test.raw%0",
        "scale(1,2,2,0,1)",
        "clamp(1,2,1)",
        "round(1,.5)",
        "1e308*1e308",
    ] {
        let a = compile(s, &t).unwrap().evaluate(&r);
        assert_eq!(a.quality, Quality::Bad);
        assert!(a.value.is_none() && a.error.is_some(), "{s}");
    }
    r.get_mut("test.speed").unwrap().quality = Quality::Bad;
    for (s, value) in [
        ("test.stop && test.speed>1", Value::Bool(false)),
        ("test.active || test.speed>1", Value::Bool(true)),
        ("test.stop ? test.speed : test.raw", Value::Number(250.0)),
        ("ifBad(test.speed,4)", Value::Number(4.0)),
        ("isGood(test.speed)", Value::Bool(false)),
    ] {
        let a = compile(s, &t).unwrap().evaluate(&r);
        assert_eq!(a.value, Some(value), "{s}");
        assert_eq!(a.quality, Quality::Good);
    }
    r.get_mut("test.speed").unwrap().quality = Quality::Uncertain;
    let a = compile("ifBad(test.speed,4)", &t).unwrap().evaluate(&r);
    assert_eq!(a.value, Some(Value::Number(12.5)));
    assert_eq!(a.quality, Quality::Uncertain);
    r.get_mut("test.speed").unwrap().value = Some(Value::Bool(true));
    assert_eq!(
        compile("test.speed+1", &t).unwrap().evaluate(&r).quality,
        Quality::Bad
    );
    r.remove("test.speed");
    assert_eq!(
        compile("test.speed", &t).unwrap().evaluate(&r).quality,
        Quality::Bad
    );
}
#[test]
fn bounds() {
    let t = BTreeMap::new();
    assert!(compile(&"1+".repeat(600), &t).is_err());
    assert!(compile(&format!("{}1{}", "(".repeat(65), ")".repeat(65)), &t).is_err());
    let chain = format!("{}1", "1+".repeat(511));
    let e = compile(&chain, &t).unwrap();
    assert_eq!(
        e.evaluate(&BTreeMap::new()).value,
        Some(Value::Number(512.0))
    );
}
fn candidate() -> serde_json::Value {
    json!({"enumerations":[{"id":"enum-state","name":"State","dataType":"uint16","entries":[{"value":0,"label":"Off","color":"#000000"},{"value":1,"label":{"textKey":"hello"},"color":"#00FF00"}]}],"mappings":[{"tag":"tag-word","enumeration":"enum-state"}],"expressions":[{"node":"n-button","property":"enabled","contractVersion":1,"expression":"`tag-word` > 0"}]})
}
fn validate(d: serde_json::Value) -> hmi_project_model::ValidationReport {
    validate_v2_bindings(
        &project(base()),
        &IndustrialSecurityDeclarations::default(),
        &serde_json::from_value(d).unwrap(),
    )
}
#[test]
fn enumeration_exact_lookup_and_localization() {
    let d: ExpressionEnumerationDeclarations = serde_json::from_value(candidate()).unwrap();
    assert!(
        validate(candidate()).is_valid(),
        "{}",
        validate(candidate())
    );
    assert_eq!(
        d.enumerations[0].lookup(&json!(1)).unwrap().label,
        hmi_project_model::model::LocalizedText::key("hello")
    );
    assert!(d.enumerations[0].lookup(&json!("1")).is_none());
    assert!(d.enumerations[0].lookup(&json!(2)).is_none());
    assert_eq!(
        d,
        serde_json::from_str(&serde_json::to_string(&d).unwrap()).unwrap()
    );
}
#[test]
fn enumeration_and_binding_refusals() {
    let mut d = candidate();
    d["enumerations"][0]["id"] = json!("tag-word");
    d["enumerations"][0]["entries"][1]["value"] = json!(0);
    d["enumerations"][0]["entries"][1]["label"] = json!({"textKey":"absent"});
    d["enumerations"][0]["entries"][1]["color"] = json!("red");
    d["expressions"][0]["expression"] = json!("unknown > 0");
    let r = validate(d);
    assert!(r.error_count() >= 6, "{r}");
    let mut d = candidate();
    d["expressions"][0]["node"] = json!("n-switch");
    d["expressions"][0]["property"] = json!("checked");
    d["expressions"][0]["expression"] = json!("true");
    assert!(validate(d).has_errors());
    let mut d = candidate();
    d["expressions"][0]["contractVersion"] = json!(2);
    assert!(validate(d).has_errors());
}

#[test]
fn canonical_scalar_admission_preserves_integer_precision() {
    use hmi_project_model::model::DataType as D;
    assert!(
        Reading::from_canonical_scalar(Some(&json!(u64::MAX)), &D::UInt64, Quality::Good).is_err()
    );
    assert!(Reading::from_canonical_scalar(
        Some(&json!(9007199254740993u64)),
        &D::UInt64,
        Quality::Good
    )
    .is_err());
    assert_eq!(
        Reading::from_canonical_scalar(
            Some(&json!(9007199254740992u64)),
            &D::UInt64,
            Quality::Uncertain
        )
        .unwrap()
        .value,
        Some(Value::Number(9007199254740992.0))
    );
    assert!(Reading::from_canonical_scalar(Some(&json!(256)), &D::UInt8, Quality::Good).is_err());
    assert!(Reading::from_canonical_scalar(Some(&json!(-1)), &D::UInt64, Quality::Good).is_err());
    assert_eq!(
        Reading::from_canonical_scalar(Some(&json!(u64::MAX)), &D::UInt64, Quality::Bad)
            .unwrap()
            .value,
        None
    );
    assert_eq!(
        Reading::from_canonical_scalar(Some(&json!(0.1)), &D::Float32, Quality::Good)
            .unwrap()
            .value,
        Some(Value::Number(f64::from(0.1_f32)))
    );
}
#[test]
fn exact_enum_values_above_f64_precision_and_type_mismatch() {
    let mut p = base();
    p["tags"][3]["dataType"] = json!("uint64");
    let mut d = candidate();
    d["enumerations"][0]["dataType"] = json!("uint64");
    d["enumerations"][0]["entries"][0]["value"] = json!(u64::MAX - 1);
    d["enumerations"][0]["entries"][1]["value"] = json!(u64::MAX);
    let d: ExpressionEnumerationDeclarations = serde_json::from_value(d).unwrap();
    let r = validate_v2_bindings(&project(p), &IndustrialSecurityDeclarations::default(), &d);
    assert!(r.is_valid(), "{r}");
    assert_ne!(
        d.enumerations[0].lookup(&json!(u64::MAX - 1)),
        d.enumerations[0].lookup(&json!(u64::MAX))
    );
    assert!(validate_v2_bindings(
        &project(base()),
        &IndustrialSecurityDeclarations::default(),
        &d
    )
    .has_errors());
}
