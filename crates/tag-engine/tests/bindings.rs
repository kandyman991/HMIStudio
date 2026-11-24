//! Binding evaluation, inverse transformations and compile-time checks.
#![allow(clippy::unwrap_used)]

mod common;

use common::*;
use hmi_project_model::model::{Element, Node, Project, PropertyBinding};
use hmi_tag_engine::*;
use serde_json::{json, Value};

fn node(kind: &str) -> Node {
    serde_json::from_value(json!({ "id": "n-x", "element": { "type": "component", "kind": kind } }))
        .unwrap()
}

fn binding(v: Value) -> PropertyBinding {
    serde_json::from_value(v).unwrap()
}

fn compile_on(r: &Rig, p: &Project, kind: &str, b: Value) -> Result<CompiledBinding, BindingError> {
    compile_for_node(
        p,
        &node(kind),
        &binding(b),
        &BindingScope::default(),
        &r.engine,
    )
}

#[test]
fn read_bindings_with_every_transform() {
    let r = connected();
    let p = project();
    let read = |kind: &str, b: Value| {
        compile_on(&r, &p, kind, b)
            .unwrap()
            .read(&r.engine)
            .unwrap()
    };
    r.plc.set("DB1.Speed", TagValue::Float32(300.0));
    r.plc
        .set("DB1.Status", TagValue::UInt16(0b1000_0000_0000_1000));
    r.plc.set("DB1.Level", TagValue::Int16(-1));
    r.engine.poll_all();
    // Direct.
    let v = read(
        "numericDisplay",
        json!({ "property": "value", "source": { "tag": "sim-speed" } }),
    );
    assert_eq!((v.value, v.quality), (json!(300.0), Quality::GOOD));
    // Linear.
    let v = read(
        "progressBar",
        json!({ "property": "value", "source": { "tag": "sim-speed" }, "transform": { "type": "linear", "factor": 0.5, "offset": -50 } }),
    );
    assert_eq!(v.value, json!(100.0));
    // Invert.
    let v = read(
        "indicator",
        json!({ "property": "active", "source": { "tag": "int-bool" }, "transform": { "type": "invert" } }),
    );
    assert_eq!(v.value, json!(true));
    // Bit extraction (unsigned and two's complement).
    for (tag, bit, expect) in [
        ("sim-status", 3, true),
        ("sim-status", 15, true),
        ("sim-status", 4, false),
        ("sim-level", 15, true),
    ] {
        let v = read(
            "indicator",
            json!({ "property": "active", "source": { "tag": tag }, "transform": { "type": "bit", "index": bit } }),
        );
        assert_eq!(v.value, json!(expect), "{tag} bit {bit}");
    }
    // Map (numeric-aware key match) with default.
    let map = json!({ "property": "text", "source": { "tag": "int-ro" }, "transform": { "type": "map", "entries": [ { "when": 4, "value": "four" }, { "when": 5, "value": { "textKey": "five" } } ], "default": "other" } });
    assert_eq!(read("text", map).value, json!({ "textKey": "five" }));
    let color = json!({ "property": "fill", "source": { "tag": "int-bool" }, "transform": { "type": "map", "entries": [ { "when": true, "value": "#00FF00" } ], "default": { "token": "inactive" } } });
    assert_eq!(
        read("rectangle", color).value,
        json!({ "token": "inactive" })
    );
    // Structured member access.
    r.plc.set(
        "DB1.Motor",
        TagValue::Struct(
            [
                ("running".to_owned(), TagValue::Bool(true)),
                ("speed".to_owned(), TagValue::Float32(55.5)),
                (
                    "codes".to_owned(),
                    TagValue::Array(vec![
                        TagValue::UInt16(1),
                        TagValue::UInt16(2),
                        TagValue::UInt16(3),
                        TagValue::UInt16(4),
                    ]),
                ),
            ]
            .into_iter()
            .collect(),
        ),
    );
    r.engine.poll_all();
    assert_eq!(
        read(
            "indicator",
            json!({ "property": "active", "source": { "tag": "sim-motor" }, "member": "running" })
        )
        .value,
        json!(true)
    );
    assert_eq!(
        read(
            "numericDisplay",
            json!({ "property": "value", "source": { "tag": "sim-motor" }, "member": "codes[2]" })
        )
        .value,
        json!(3)
    );
    assert_eq!(
        read(
            "numericDisplay",
            json!({ "property": "value", "source": { "tag": "sim-motor" }, "member": "speed" })
        )
        .value,
        json!(55.5)
    );
}

#[test]
fn map_without_match_or_default_is_an_evaluation_error() {
    let r = rig();
    let b = compile_on(&r, &project(), "text", json!({ "property": "text", "source": { "tag": "int-ro" }, "transform": { "type": "map", "entries": [ { "when": 1, "value": "one" } ] } })).unwrap();
    assert!(matches!(
        b.read(&r.engine),
        Err(BindingError::NoMapping { .. })
    ));
}

#[test]
fn write_and_bidirectional_bindings_with_inverse_transforms() {
    let r = connected();
    let p = project();
    // Read/write direct.
    let b = compile_on(
        &r,
        &p,
        "numericInput",
        json!({ "property": "value", "source": { "tag": "sim-speed" }, "mode": "readWrite" }),
    )
    .unwrap();
    b.write(&json!(123.5), &r.engine).unwrap();
    assert_eq!(r.plc.get("DB1.Speed"), Some(TagValue::Float32(123.5)));
    r.engine.poll_all();
    assert_eq!(b.read(&r.engine).unwrap().value, json!(123.5));
    // Linear inverse into an integer member, rounded to nearest.
    let b = compile_on(&r, &p, "slider", json!({ "property": "value", "source": { "tag": "sim-arr" }, "member": "[0]", "mode": "readWrite", "transform": { "type": "linear", "factor": 0.1 } })).unwrap();
    assert_eq!(
        b.inverse_value(&json!(12.34), &r.engine).unwrap(),
        TagValue::Int16(123)
    );
    b.write(&json!(12.34), &r.engine).unwrap();
    assert_eq!(r.plc.get("DB1.Arr").unwrap().to_json(), json!([123, 0, 0]));
    r.engine.poll_all();
    let back = b.read(&r.engine).unwrap().value.as_f64().unwrap();
    assert!((back - 12.3).abs() < 1e-9, "{back}");
    // Inverse with offset.
    let b = compile_on(&r, &p, "numericInput", json!({ "property": "value", "source": { "tag": "int-real" }, "mode": "write", "transform": { "type": "linear", "factor": 2, "offset": 10 } })).unwrap();
    b.write(&json!(110), &r.engine).unwrap();
    assert_eq!(
        r.engine.read(&tid("int-real")).unwrap().value,
        TagValue::Float32(50.0)
    );
    // Inverted switch.
    let b = compile_on(&r, &p, "switch", json!({ "property": "checked", "source": { "tag": "int-bool" }, "mode": "readWrite", "transform": { "type": "invert" } })).unwrap();
    b.write(&json!(true), &r.engine).unwrap();
    assert_eq!(
        r.engine.read(&tid("int-bool")).unwrap().value,
        TagValue::Bool(false)
    );
    assert_eq!(b.read(&r.engine).unwrap().value, json!(true));
    // Engineering range still enforced through bindings.
    let b = compile_on(
        &r,
        &p,
        "numericInput",
        json!({ "property": "value", "source": { "tag": "int-real" }, "mode": "readWrite" }),
    )
    .unwrap();
    assert!(matches!(
        b.write(&json!(101), &r.engine),
        Err(BindingError::Tag(TagError::OutOfRange { .. }))
    ));
    // Wrong input shape and out-of-type values.
    assert!(matches!(
        b.write(&json!("fast"), &r.engine),
        Err(BindingError::InvalidInput { .. })
    ));
    let ib = compile_on(&r, &p, "slider", json!({ "property": "value", "source": { "tag": "sim-arr" }, "member": "[1]", "mode": "readWrite" })).unwrap();
    assert!(matches!(
        ib.write(&json!(40000), &r.engine),
        Err(BindingError::Member { .. })
    ));
    assert!(matches!(
        ib.write(&json!(1.5), &r.engine),
        Err(BindingError::Member { .. })
    ));
    // Read-only bindings refuse writes.
    let ro = compile_on(
        &r,
        &p,
        "numericInput",
        json!({ "property": "value", "source": { "tag": "sim-speed" } }),
    )
    .unwrap();
    assert!(matches!(
        ro.write(&json!(1), &r.engine),
        Err(BindingError::ReadOnly { .. })
    ));
}

#[test]
fn incompatible_and_non_invertible_bindings_are_rejected_at_compile_time() {
    let r = rig();
    let p = project();
    let err = |kind: &str, b: Value| compile_on(&r, &p, kind, b).unwrap_err();
    let ni = |e: BindingError| assert!(matches!(e, BindingError::NotInvertible { .. }), "{e:?}");
    let inc = |e: BindingError| assert!(matches!(e, BindingError::Incompatible { .. }), "{e:?}");
    ni(err(
        "numericInput",
        json!({ "property": "value", "source": { "tag": "sim-speed" }, "mode": "readWrite", "transform": { "type": "linear", "factor": 0 } }),
    ));
    ni(err(
        "switch",
        json!({ "property": "checked", "source": { "tag": "sim-arr" }, "member": "[0]", "mode": "readWrite", "transform": { "type": "bit", "index": 0 } }),
    ));
    ni(err(
        "switch",
        json!({ "property": "checked", "source": { "tag": "int-bool" }, "mode": "write", "transform": { "type": "map", "entries": [ { "when": true, "value": false } ] } }),
    ));
    inc(err(
        "numericDisplay",
        json!({ "property": "value", "source": { "tag": "int-text" } }),
    ));
    inc(err(
        "indicator",
        json!({ "property": "active", "source": { "tag": "sim-speed" }, "transform": { "type": "bit", "index": 0 } }),
    ));
    inc(err(
        "indicator",
        json!({ "property": "active", "source": { "tag": "sim-status" }, "transform": { "type": "bit", "index": 16 } }),
    ));
    inc(err(
        "indicator",
        json!({ "property": "active", "source": { "tag": "int-real" }, "transform": { "type": "invert" } }),
    ));
    inc(err(
        "indicator",
        json!({ "property": "active", "source": { "tag": "int-bool" }, "transform": { "type": "map", "entries": [ { "when": true, "value": "yes" } ] } }),
    ));
    inc(err(
        "indicator",
        json!({ "property": "active", "source": { "tag": "int-bool" }, "transform": { "type": "map", "entries": [ { "when": 3, "value": true } ] } }),
    ));
    inc(err(
        "indicator",
        json!({ "property": "activeColor", "source": { "tag": "int-ro" } }),
    ));
    inc(err(
        "progressBar",
        json!({ "property": "value", "source": { "tag": "int-text" }, "transform": { "type": "linear", "factor": 1 } }),
    ));
    // Access, property and source checks.
    assert!(matches!(
        err(
            "numericInput",
            json!({ "property": "value", "source": { "tag": "int-ro" }, "mode": "readWrite" })
        ),
        BindingError::AccessDenied {
            operation: "write",
            ..
        }
    ));
    assert!(matches!(
        err(
            "numericDisplay",
            json!({ "property": "value", "source": { "tag": "int-wo" } })
        ),
        BindingError::AccessDenied {
            operation: "read",
            ..
        }
    ));
    assert!(matches!(
        err(
            "numericDisplay",
            json!({ "property": "decimals", "source": { "tag": "int-ro" } })
        ),
        BindingError::Property { .. }
    ));
    assert!(matches!(
        err(
            "numericDisplay",
            json!({ "property": "value", "source": { "tag": "int-real" }, "mode": "readWrite" })
        ),
        BindingError::Property { .. }
    ));
    assert!(matches!(
        err(
            "numericDisplay",
            json!({ "property": "value", "source": { "tag": "nope" } })
        ),
        BindingError::UnknownTag { .. }
    ));
    assert!(matches!(
        err(
            "numericDisplay",
            json!({ "property": "value", "source": { "tag": "sim-motor" }, "member": "torque" })
        ),
        BindingError::Member { .. }
    ));
    assert!(matches!(
        err(
            "text",
            json!({ "property": "text", "source": { "parameter": "caption" } })
        ),
        BindingError::NotATagBinding { .. }
    ));
    assert!(matches!(
        err(
            "indicator",
            json!({ "property": "active", "source": { "tagParameter": "motor" } })
        ),
        BindingError::UnboundTagParameter { .. }
    ));
}

#[test]
fn bad_quality_propagates_through_bindings() {
    let r = connected();
    let b = compile_on(
        &r,
        &project(),
        "numericDisplay",
        json!({ "property": "value", "source": { "tag": "sim-count" } }),
    )
    .unwrap();
    r.plc.set_online(false);
    r.engine.poll_all();
    let v = b.read(&r.engine).unwrap();
    assert_eq!(v.quality, Quality::bad(QualityReason::ConnectionLost));
}

/// Every tag binding in the RUST-03A industrial fixture compiles and
/// evaluates, with the PLC connection redirected to the simulator.
#[test]
fn industrial_fixture_bindings_compile_and_evaluate_on_the_simulator() {
    let mut v: Value = serde_json::from_str(
        &std::fs::read_to_string(fixture_path("bottling-line.hmi.json")).unwrap(),
    )
    .unwrap();
    for c in v["connections"].as_array_mut().unwrap() {
        c["driver"] = json!("simulator");
        c["parameters"] = json!({ "reconnectMs": 0 });
    }
    let p = project_from(v);
    let r = rig_with(&p, ScanMode::Manual);
    assert!(r.report.issues.is_empty(), "{:?}", r.report);
    r.engine.poll_all();
    let mut compiled = 0;
    let mut value_params = 0;
    let mut visit = |n: &Node, scope: &BindingScope| {
        for b in &n.bindings {
            match compile_for_node(&p, n, b, scope, &r.engine) {
                Ok(c) => {
                    // Freshly seeded simulator: zero/initial values are valid.
                    let out = c.read(&r.engine);
                    match out {
                        Ok(bv) => assert!(bv.quality.is_good(), "{} {:?}", n.id, bv),
                        Err(BindingError::NoMapping { .. }) => {}
                        Err(e) => panic!("{}: {e}", n.id),
                    }
                    compiled += 1;
                }
                Err(BindingError::NotATagBinding { .. }) => value_params += 1,
                Err(e) => panic!("{} {}: {e}", n.id, b.property),
            }
        }
    };
    fn walk(n: &Node, f: &mut dyn FnMut(&Node)) {
        f(n);
        for c in &n.children {
            walk(c, f);
        }
    }
    // Screens (direct tag bindings and instance-level value-parameter bindings).
    for s in &p.screens {
        walk(&s.root, &mut |n| visit(n, &BindingScope::default()));
    }
    // Definition trees, once per instance scope.
    for s in &p.screens {
        walk(&s.root, &mut |n| {
            if let Element::Instance { definition, .. } = &n.element {
                let def = p.components.iter().find(|d| &d.id == definition).unwrap();
                let scope = BindingScope::for_instance(def, n);
                walk(&def.root, &mut |inner| visit(inner, &scope));
            }
        });
    }
    assert!(compiled >= 25, "{compiled}");
    assert!(value_params >= 3, "{value_params}");
    // A faceplate command through an instance scope reaches the PLC member.
    r.engine
        .write_json(&tid("tag-conveyor-motor"), Some("cmdStart"), &json!(true))
        .unwrap();
    let plc = r.hub.plc("conn-plc1");
    let m = plc.get("ns=3;s=\"DB_Line3\".\"Conveyor\"").unwrap();
    assert_eq!(
        m.get(&MemberPath::parse("cmdStart").unwrap()),
        Some(&TagValue::Bool(true))
    );
}

/// Regression: integer sources bound to number properties were emitted as
/// floats (`3.0`); they now stay integers, including full-range u64.
#[test]
fn regression_integer_sources_stay_integers() {
    let r = connected();
    r.plc.set("DB1.Count", TagValue::UInt32(u32::MAX));
    r.engine.poll_all();
    let b = compile_on(
        &r,
        &project(),
        "numericDisplay",
        json!({ "property": "value", "source": { "tag": "sim-count" } }),
    )
    .unwrap();
    let v = b.read(&r.engine).unwrap().value;
    assert!(v.is_u64() || v.is_i64(), "{v}");
    assert_eq!(v, json!(u32::MAX));
}
