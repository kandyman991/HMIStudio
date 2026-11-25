//! Tag values, types, member access, access rights, ranges, quality,
//! timestamps and initialization.
#![allow(clippy::unwrap_used)]

mod common;

use common::*;
use hmi_project_model::model::DataType;
use hmi_tag_engine::*;
use serde_json::json;

#[test]
fn synthetic_project_is_valid_rust03a() {
    let r = hmi_project_model::validate(&project());
    assert!(r.is_valid(), "{r}");
}

#[test]
fn initialization_is_deterministic_and_reports_config_issues() {
    let a = rig();
    let b = rig();
    assert_eq!(a.report, b.report);
    assert_eq!(a.engine.tag_ids(), b.engine.tag_ids());
    let subjects: Vec<&str> = a.report.issues.iter().map(|i| i.subject.as_str()).collect();
    assert_eq!(subjects, ["conn-opc"], "{:?}", a.report);
    assert!(matches!(
        a.engine.connection_status(&cid("conn-opc")),
        Some(ConnectionStatus::Unavailable { .. })
    ));
    // Initial qualities.
    let q = |t: &str| a.engine.read(&tid(t)).unwrap().quality;
    assert_eq!(q("int-real"), Quality::GOOD);
    assert_eq!(q("sim-speed"), Quality::bad(QualityReason::NotConnected));
    assert_eq!(q("opc-tag"), Quality::bad(QualityReason::ConfigError));
    assert_eq!(
        a.engine.read(&tid("int-real")).unwrap().value,
        TagValue::Float32(12.5)
    );
    assert_eq!(
        a.engine.read(&tid("int-ro")).unwrap().value,
        TagValue::Int16(5)
    );
    assert_eq!(a.engine.read(&tid("int-real")).unwrap().timestamp, START);
    // Struct default value.
    let m = a.engine.read(&tid("int-motor")).unwrap().value;
    assert_eq!(
        m.to_json(),
        json!({ "codes": [0, 0, 0, 0], "running": false, "speed": 0.0 })
    );
}

#[test]
fn invalid_initial_values_and_config_errors_do_not_abort_startup() {
    let mut v = project_json();
    v["tags"].as_array_mut().unwrap().extend([
        json!({ "id": "bad-init", "name": "x", "dataType": "uint8", "initialValue": 300 }),
        json!({ "id": "bad-conn", "name": "y", "dataType": "bool", "connection": "conn-nope", "address": "a" }),
    ]);
    let r = rig_with(&project_from(v), ScanMode::Manual);
    let subjects: Vec<&str> = r.report.issues.iter().map(|i| i.subject.as_str()).collect();
    assert_eq!(
        subjects,
        ["bad-conn", "conn-opc", "bad-init"],
        "phase order, deterministic"
    );
    assert_eq!(
        r.engine.read(&tid("bad-init")).unwrap().quality,
        Quality::bad(QualityReason::ConfigError)
    );
    assert!(matches!(
        r.engine.write(&tid("bad-conn"), TagValue::Bool(true)),
        Err(TagError::AccessDenied { .. })
    ));
}

#[test]
fn internal_read_write_and_types() {
    let r = rig();
    let e = &r.engine;
    e.write(&tid("int-bool"), TagValue::Bool(true)).unwrap();
    assert_eq!(
        e.read(&tid("int-bool")).unwrap().value,
        TagValue::Bool(true)
    );
    // Numeric coercion: integral value into float, float into int.
    e.write(&tid("int-real"), TagValue::Int32(50)).unwrap();
    assert_eq!(
        e.read(&tid("int-real")).unwrap().value,
        TagValue::Float32(50.0)
    );
    e.write_json(&tid("int-text"), None, &json!("world"))
        .unwrap();
    assert_eq!(
        e.read(&tid("int-text")).unwrap().value,
        TagValue::String("world".into())
    );
    // Type mismatches.
    for (tag, v) in [
        ("int-bool", TagValue::Int16(1)),
        ("int-real", TagValue::String("1".into())),
        ("int-text", TagValue::Float64(1.0)),
    ] {
        assert!(
            matches!(e.write(&tid(tag), v), Err(TagError::Value { .. })),
            "{tag}"
        );
    }
    assert!(matches!(
        e.write_json(&tid("int-real"), None, &json!(1e300)),
        Err(TagError::Value { .. })
    ));
    // Unknown tag.
    assert!(matches!(
        e.read(&tid("nope")),
        Err(TagError::UnknownTag { .. })
    ));
    assert!(matches!(
        e.write(&tid("nope"), TagValue::Bool(true)),
        Err(TagError::UnknownTag { .. })
    ));
}

#[test]
fn access_permissions_are_enforced() {
    let r = rig();
    let e = &r.engine;
    assert!(matches!(
        e.write(&tid("int-ro"), TagValue::Int16(1)),
        Err(TagError::AccessDenied {
            operation: "write",
            ..
        })
    ));
    assert!(matches!(
        e.read(&tid("int-wo")),
        Err(TagError::AccessDenied {
            operation: "read",
            ..
        })
    ));
    e.write(&tid("int-wo"), TagValue::Int32(7)).unwrap();
    assert!(matches!(
        e.read(&tid("sim-cmd")),
        Err(TagError::AccessDenied { .. })
    ));
    assert!(matches!(
        e.subscribe(Some(&[tid("int-wo")])),
        Err(TagError::AccessDenied { .. })
    ));
    assert_eq!(
        e.read(&tid("int-ro")).unwrap().value,
        TagValue::Int16(5),
        "unchanged"
    );
}

#[test]
fn engineering_range_rejects_operator_writes() {
    let r = rig();
    let e = &r.engine;
    let err = e
        .write(&tid("int-real"), TagValue::Float32(100.5))
        .unwrap_err();
    assert!(matches!(err, TagError::OutOfRange { min, max, .. } if min == 0.0 && max == 100.0));
    assert!(e.write(&tid("int-real"), TagValue::Float32(-0.1)).is_err());
    e.write(&tid("int-real"), TagValue::Float32(100.0)).unwrap();
    assert_eq!(
        e.read(&tid("int-real")).unwrap().value,
        TagValue::Float32(100.0)
    );
}

#[test]
fn device_values_outside_range_are_uncertain_not_rejected() {
    let r = connected();
    r.plc.set("DB1.Speed", TagValue::Float32(900.0));
    r.engine.poll_all();
    let s = r.engine.read(&tid("sim-speed")).unwrap();
    assert_eq!(s.value, TagValue::Float32(900.0));
    assert_eq!(s.quality, Quality::uncertain(QualityReason::OutOfRange));
}

#[test]
fn structured_member_access() {
    let r = connected();
    let e = &r.engine;
    // Internal struct: member writes are atomic updates of the whole value.
    e.write_json(&tid("int-motor"), Some("speed"), &json!(12.5))
        .unwrap();
    e.write_json(&tid("int-motor"), Some("codes[2]"), &json!(7))
        .unwrap();
    e.write_member(
        &tid("int-motor"),
        &MemberPath::parse("running").unwrap(),
        TagValue::Bool(true),
    )
    .unwrap();
    assert_eq!(
        e.read(&tid("int-motor")).unwrap().value.to_json(),
        json!({ "codes": [0, 0, 7, 0], "running": true, "speed": 12.5 })
    );
    assert_eq!(
        e.read_member(&tid("int-motor"), "codes[2]").unwrap().value,
        TagValue::UInt16(7)
    );
    // Device struct: member write goes to the PLC.
    e.write_json(&tid("sim-motor"), Some("codes[1]"), &json!(65535))
        .unwrap();
    assert_eq!(
        r.plc
            .get("DB1.Motor")
            .unwrap()
            .get(&MemberPath::parse("codes[1]").unwrap()),
        Some(&TagValue::UInt16(65535))
    );
    // Whole-struct JSON writes need every member.
    assert!(e
        .write_json(&tid("int-motor"), None, &json!({ "running": true }))
        .is_err());
    e.write_json(
        &tid("int-motor"),
        None,
        &json!({ "running": false, "speed": 1, "codes": [1, 2, 3, 4] }),
    )
    .unwrap();
    // Invalid paths and values.
    for (path, v) in [
        ("nope", json!(1)),
        ("codes[4]", json!(1)),
        ("codes[0]", json!(70000)),
        ("running.x", json!(true)),
        ("codes[", json!(1)),
        ("speed", json!("fast")),
    ] {
        assert!(
            matches!(
                e.write_json(&tid("int-motor"), Some(path), &v),
                Err(TagError::Value { .. })
            ),
            "{path}"
        );
    }
    assert!(e.read_member(&tid("int-motor"), "codes[9]").is_err());
}

#[test]
fn type_context_rules() {
    let t = TypeContext::default();
    let ok = |v: serde_json::Value, dt: DataType| t.value_from_json(&v, &dt);
    assert!(ok(json!(127), DataType::Int8).is_ok());
    assert!(ok(json!(128), DataType::Int8).is_err());
    assert!(ok(json!(-1), DataType::UInt64).is_err());
    assert!(ok(json!(18446744073709551615u64), DataType::UInt64).is_ok());
    assert!(ok(json!(1.5), DataType::Int32).is_err());
    assert!(
        ok(json!(3.5e38), DataType::Float32).is_err(),
        "f32 overflow"
    );
    assert!(ok(json!(3.4e38), DataType::Float32).is_ok());
    assert!(ok(json!("x"), DataType::Bool).is_err());
    assert!(ok(
        json!([1, 2]),
        DataType::Array {
            element: Box::new(DataType::Int8),
            length: 3
        }
    )
    .is_err());
    // Coercion is never between kinds.
    assert!(t.coerce(TagValue::Bool(true), &DataType::Int32).is_err());
    assert!(t.coerce(TagValue::Int32(1), &DataType::Bool).is_err());
    assert!(t.coerce(TagValue::Float64(2.0), &DataType::UInt8).is_ok());
    assert!(t.coerce(TagValue::Float64(2.5), &DataType::UInt8).is_err());
    assert!(t.coerce(TagValue::Int64(-1), &DataType::UInt8).is_err());
    assert!(t
        .coerce(TagValue::Float64(f64::NAN), &DataType::Float64)
        .is_err());
    // f32 values serialize with their shortest decimal form.
    assert_eq!(TagValue::Float32(0.1).to_json(), json!(0.1));
    // Member paths (RUST-03A syntax).
    assert_eq!(
        MemberPath::parse("a.b[2].c").unwrap().to_string(),
        "a.b[2].c"
    );
    for bad in ["", ".a", "a..b", "a[x]", "a[1", "1a", "a]"] {
        assert!(MemberPath::parse(bad).is_err(), "{bad}");
    }
}

#[test]
fn timestamps_follow_the_clock() {
    let r = rig();
    r.clock.advance_millis(250);
    r.engine
        .write(&tid("int-bool"), TagValue::Bool(true))
        .unwrap();
    let s = r.engine.read(&tid("int-bool")).unwrap();
    assert_eq!(s.timestamp, START.plus_millis(250));
    assert!(s.sequence > 0);
}

#[test]
fn regression_wide_integer_range_is_not_rounded() {
    let mut v = project_json();
    v["tags"].as_array_mut().unwrap().push(json!({
        "id":"wide", "name":"Wide", "dataType":"uint64", "access":"readWrite",
        "range":{"min":0,"max":9007199254740992_u64}
    }));
    let r = rig_with(&project_from(v), ScanMode::Manual);
    let t = tid("wide");
    r.engine
        .write(&t, TagValue::UInt64(9007199254740992))
        .unwrap();
    assert!(matches!(
        r.engine.write(&t, TagValue::UInt64(9007199254740993)),
        Err(TagError::OutOfRange { .. })
    ));
    assert_eq!(
        r.engine.read(&t).unwrap().value,
        TagValue::UInt64(9007199254740992)
    );
}

#[test]
fn regression_default_values_bound_expanded_array_size() {
    let r = rig();
    let types = r.engine.types();
    let huge = DataType::Array {
        element: Box::new(DataType::Bool),
        length: u32::MAX,
    };
    assert_eq!(types.default_value(&huge), Err(ValueError::TooLarge));
    let nested = DataType::Array {
        element: Box::new(DataType::Array {
            element: Box::new(DataType::Bool),
            length: 1000,
        }),
        length: 1000,
    };
    assert_eq!(types.default_value(&nested), Err(ValueError::TooLarge));
    let small = DataType::Array {
        element: Box::new(DataType::Bool),
        length: 2,
    };
    assert_eq!(
        types.default_value(&small).unwrap(),
        TagValue::Array(vec![TagValue::Bool(false); 2])
    );
}
