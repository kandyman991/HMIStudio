//! Simulated PLC: connection lifecycle, reads/writes, latency, failures,
//! recovery, generators, scan groups and deterministic scenarios.
#![allow(clippy::unwrap_used)]

mod common;

use std::time::Duration;

use common::*;
use hmi_tag_engine::*;
use serde_json::json;

#[test]
fn connection_lifecycle_and_initial_seed() {
    let r = rig();
    let st = || r.engine.connection_status(&cid("conn-sim")).unwrap();
    assert_eq!(st(), ConnectionStatus::Disconnected);
    // The simulator was seeded with the tag's initial (simulation) value.
    assert_eq!(r.plc.get("DB1.Speed"), Some(TagValue::Float32(42.5)));
    let n = r.engine.poll_due();
    assert_eq!(st(), ConnectionStatus::Connected);
    assert_eq!(
        n, 7,
        "seven readable simulator tags (sim-cmd is write-only)"
    );
    let s = r.engine.read(&tid("sim-speed")).unwrap();
    assert_eq!(
        (s.value, s.quality),
        (TagValue::Float32(42.5), Quality::GOOD)
    );
    assert_eq!(r.plc.stats().connects, 1);
    let caps = r.engine.capabilities(&cid("conn-sim")).unwrap();
    assert!(!caps.physical && caps.member_writes && caps.grouped_read);
}

#[test]
fn plc_changes_appear_after_poll_and_writes_reach_the_plc() {
    let r = connected();
    r.plc.set("DB1.Count", TagValue::UInt32(1234));
    assert_eq!(
        r.engine.read(&tid("sim-count")).unwrap().value,
        TagValue::UInt32(0)
    );
    r.engine.poll_all();
    assert_eq!(
        r.engine.read(&tid("sim-count")).unwrap().value,
        TagValue::UInt32(1234)
    );
    // Development/simulation writes: no approval needed.
    r.engine
        .write(&tid("sim-speed"), TagValue::Float64(300.0))
        .unwrap();
    assert_eq!(r.plc.get("DB1.Speed"), Some(TagValue::Float32(300.0)));
    r.engine
        .write(&tid("sim-cmd"), TagValue::Bool(true))
        .unwrap();
    assert_eq!(r.plc.get("DB1.Cmd"), Some(TagValue::Bool(true)));
    r.engine
        .write_json(&tid("sim-arr"), None, &json!([1, -2, 3]))
        .unwrap();
    assert_eq!(r.plc.get("DB1.Arr").unwrap().to_json(), json!([1, -2, 3]));
    // Range check applies to device tags too, before anything is sent.
    let writes_before = r.plc.stats().write_requests;
    assert!(matches!(
        r.engine.write(&tid("sim-speed"), TagValue::Float32(601.0)),
        Err(TagError::OutOfRange { .. })
    ));
    assert_eq!(r.plc.stats().write_requests, writes_before);
    // Individual device read.
    r.plc.set("DB1.Count", TagValue::UInt32(5));
    let fresh = r
        .engine
        .read_device(&[tid("sim-count"), tid("int-ro"), tid("sim-cmd")]);
    assert_eq!(fresh[0].as_ref().unwrap().value, TagValue::UInt32(5));
    assert_eq!(fresh[1].as_ref().unwrap().value, TagValue::Int16(5));
    assert!(matches!(fresh[2], Err(TagError::AccessDenied { .. })));
}

#[test]
fn grouped_writes_preserve_order_and_report_per_item() {
    let r = connected();
    let res = r.engine.write_many(vec![
        WriteRequest {
            tag: tid("sim-speed"),
            member: MemberPath::root(),
            value: TagValue::Float32(10.0),
        },
        WriteRequest {
            tag: tid("int-ro"),
            member: MemberPath::root(),
            value: TagValue::Int16(1),
        },
        WriteRequest {
            tag: tid("int-bool"),
            member: MemberPath::root(),
            value: TagValue::Bool(true),
        },
        WriteRequest {
            tag: tid("sim-arr"),
            member: MemberPath::parse("[1]").unwrap(),
            value: TagValue::Int16(9),
        },
    ]);
    assert!(res[0].is_ok());
    assert!(matches!(res[1], Err(TagError::AccessDenied { .. })));
    assert!(res[2].is_ok());
    assert!(res[3].is_ok());
    assert_eq!(
        r.plc.stats().write_requests,
        1,
        "device items grouped in one request"
    );
    assert_eq!(r.plc.get("DB1.Arr").unwrap().to_json(), json!([0, 9, 0]));
}

#[test]
fn latency_within_timeout_succeeds_beyond_times_out() {
    let r = connected();
    r.plc.set_latency(Duration::from_millis(20));
    r.plc.set("DB1.Count", TagValue::UInt32(77));
    r.engine.poll_all();
    assert_eq!(
        r.engine.read(&tid("sim-count")).unwrap().value,
        TagValue::UInt32(77)
    );
    // Connection timeout is 200 ms.
    r.plc.set_latency(Duration::from_millis(400));
    r.engine.poll_all();
    let s = r.engine.read(&tid("sim-count")).unwrap();
    assert_eq!(s.quality, Quality::bad(QualityReason::Timeout));
    assert_eq!(s.value, TagValue::UInt32(77), "last known value kept");
    assert!(matches!(
        r.engine.write(&tid("sim-speed"), TagValue::Float32(1.0)),
        Err(TagError::Driver {
            error: DriverError {
                kind: DriverErrorKind::Timeout,
                ..
            },
            ..
        })
    ));
    // A timeout does not drop the connection.
    assert_eq!(
        r.engine.connection_status(&cid("conn-sim")),
        Some(ConnectionStatus::Connected)
    );
    r.plc.set_latency(Duration::ZERO);
    r.engine.poll_all();
    assert!(r.engine.read(&tid("sim-count")).unwrap().quality.is_good());
}

#[test]
fn connection_loss_and_recovery() {
    let r = connected();
    r.plc.set_online(false);
    r.engine.poll_all();
    assert!(matches!(
        r.engine.connection_status(&cid("conn-sim")),
        Some(ConnectionStatus::Faulted { .. })
    ));
    for t in ["sim-speed", "sim-count", "sim-motor"] {
        assert_eq!(
            r.engine.read(&tid(t)).unwrap().quality,
            Quality::bad(QualityReason::ConnectionLost),
            "{t}"
        );
    }
    // Writes fail fast while disconnected; internal tags unaffected.
    assert!(matches!(
        r.engine.write(&tid("sim-speed"), TagValue::Float32(1.0)),
        Err(TagError::NotConnected { .. })
    ));
    r.engine
        .write(&tid("int-bool"), TagValue::Bool(true))
        .unwrap();
    // Reconnect attempts fail while offline...
    r.engine.poll_all();
    assert!(matches!(
        r.engine.connection_status(&cid("conn-sim")),
        Some(ConnectionStatus::Faulted { .. })
    ));
    // ...and succeed after recovery; values become good again.
    r.plc.set("DB1.Count", TagValue::UInt32(9));
    r.plc.set_online(true);
    r.engine.poll_due();
    assert_eq!(
        r.engine.connection_status(&cid("conn-sim")),
        Some(ConnectionStatus::Connected)
    );
    let s = r.engine.read(&tid("sim-count")).unwrap();
    assert_eq!((s.value, s.quality), (TagValue::UInt32(9), Quality::GOOD));
    assert_eq!(r.plc.stats().connects, 2);
}

#[test]
fn reconnect_respects_the_reconnect_delay() {
    let mut v = project_json();
    v["connections"][0]["parameters"]["reconnectMs"] = json!(5000);
    let r = rig_with(&project_from(v), ScanMode::Manual);
    r.engine.poll_due();
    r.plc.set_online(false);
    r.engine.poll_due(); // detects loss
    r.plc.set_online(true);
    r.engine.poll_due();
    assert!(
        matches!(
            r.engine.connection_status(&cid("conn-sim")),
            Some(ConnectionStatus::Faulted { .. })
        ),
        "too early"
    );
    r.clock.advance_millis(5000);
    r.engine.poll_due();
    assert_eq!(
        r.engine.connection_status(&cid("conn-sim")),
        Some(ConnectionStatus::Connected)
    );
}

#[test]
fn injected_and_item_errors() {
    let r = connected();
    r.plc.apply(&[SimStep::FailNext {
        kind: DriverErrorKind::Protocol,
        count: 1,
    }]);
    r.engine.poll_all();
    assert_eq!(
        r.engine.read(&tid("sim-count")).unwrap().quality,
        Quality::bad(QualityReason::CommunicationError)
    );
    r.engine.poll_all();
    assert!(r.engine.read(&tid("sim-count")).unwrap().quality.is_good());
    // Item-level error affects only that tag.
    r.plc.apply(&[SimStep::FailAddress {
        address: "DB1.Status".into(),
        kind: Some(DriverErrorKind::AddressInvalid),
    }]);
    r.engine.poll_all();
    assert_eq!(
        r.engine.read(&tid("sim-status")).unwrap().quality,
        Quality::bad(QualityReason::ConfigError)
    );
    assert!(r.engine.read(&tid("sim-count")).unwrap().quality.is_good());
    r.plc.apply(&[SimStep::FailAddress {
        address: "DB1.Status".into(),
        kind: None,
    }]);
    r.engine.poll_all();
    assert!(r.engine.read(&tid("sim-status")).unwrap().quality.is_good());
    // Injected connect failure.
    r.plc.set_online(false);
    r.engine.poll_all();
    r.plc.set_online(true);
    r.plc.apply(&[SimStep::FailNext {
        kind: DriverErrorKind::ConnectFailed,
        count: 1,
    }]);
    r.engine.poll_all();
    assert!(matches!(
        r.engine.connection_status(&cid("conn-sim")),
        Some(ConnectionStatus::Faulted { .. })
    ));
    r.engine.poll_all();
    assert_eq!(
        r.engine.connection_status(&cid("conn-sim")),
        Some(ConnectionStatus::Connected)
    );
    // Type mismatch on the PLC side.
    r.plc.set("DB1.Count", TagValue::String("oops".into()));
    r.engine.poll_all();
    assert_eq!(
        r.engine.read(&tid("sim-count")).unwrap().quality,
        Quality::bad(QualityReason::TypeMismatch)
    );
}

#[test]
fn generator_addresses_are_deterministic_and_read_only() {
    let r = connected();
    // t = START (an exact multiple of the 4 s period): sine = offset.
    let s = r.engine.read(&tid("sim-sine")).unwrap();
    let TagValue::Float64(x) = s.value else {
        panic!()
    };
    assert!((x - 10.0).abs() < 1e-9, "{x}");
    r.clock.advance_millis(1000); // quarter period -> peak
    r.engine.poll_all();
    let TagValue::Float64(x) = r.engine.read(&tid("sim-sine")).unwrap().value else {
        panic!()
    };
    assert!((x - 12.0).abs() < 1e-9, "{x}");
    let mut v = project_json();
    v["tags"][14] = json!({ "id": "opc-tag", "name": "gen", "dataType": "float64", "connection": "conn-sim", "address": "sine:", "access": "readWrite" });
    let r2 = rig_with(&project_from(v), ScanMode::Manual);
    r2.engine.poll_all();
    assert!(matches!(
        r2.engine.write(&tid("opc-tag"), TagValue::Float64(1.0)),
        Err(TagError::Driver {
            error: DriverError {
                kind: DriverErrorKind::AccessDenied,
                ..
            },
            ..
        })
    ));
}

#[test]
fn strict_addresses_reject_unknown_memory_at_driver_level() {
    use hmi_project_model::model::{Connection, DataType, DriverKind};
    use hmi_tag_engine::driver::{DriverFactory, OpContext, ReadItem};
    let clock: std::sync::Arc<dyn Clock> = std::sync::Arc::new(ManualClock::new(START));
    let hub = SimulatorHub::new(clock);
    let conn: Connection = serde_json::from_value(json!({
        "id": "conn-x", "name": "x", "driver": "simulator", "parameters": { "strictAddresses": true }
    }))
    .unwrap();
    assert_eq!(conn.driver, DriverKind::Simulator);
    let mut d = hub
        .factory()
        .create(&conn, &[], &TypeContext::default())
        .unwrap();
    let ctx = OpContext {
        timeout: Duration::from_millis(100),
        cancel: CancelToken::new(),
    };
    let item = ReadItem {
        tag: tid("t"),
        address: "DB1.Nope".into(),
        data_type: DataType::Int16,
    };
    // Not connected yet.
    assert_eq!(
        d.read(std::slice::from_ref(&item), &ctx).unwrap_err().kind,
        DriverErrorKind::NotConnected
    );
    d.connect(&ctx).unwrap();
    let r = d.read(&[item], &ctx).unwrap();
    assert_eq!(
        r[0].as_ref().unwrap_err().kind,
        DriverErrorKind::AddressInvalid
    );
    // Cancellation interrupts latency promptly.
    hub.plc("conn-x").set_latency(Duration::from_secs(30));
    ctx.cancel.cancel();
    let t0 = std::time::Instant::now();
    let item = ReadItem {
        tag: tid("t"),
        address: "sine:".into(),
        data_type: DataType::Float64,
    };
    let long = OpContext {
        timeout: Duration::from_secs(60),
        cancel: ctx.cancel.clone(),
    };
    assert_eq!(
        d.read(&[item], &long).unwrap_err().kind,
        DriverErrorKind::Cancelled
    );
    assert!(t0.elapsed() < Duration::from_secs(5));
    // Bad parameters are a factory error.
    let bad: Connection = serde_json::from_value(json!({
        "id": "conn-y", "name": "y", "driver": "simulator", "parameters": { "latencyMs": "slow" }
    }))
    .unwrap();
    assert!(hub
        .factory()
        .create(&bad, &[], &TypeContext::default())
        .is_err());
}

#[test]
fn scan_groups_poll_at_their_intervals() {
    let r = connected();
    let reads = || r.plc.stats().items_read;
    // sim-motor and sim-speed scan every 100 ms; the rest every 1000 ms.
    let base = reads();
    r.clock.advance_millis(100);
    r.engine.poll_due();
    assert_eq!(reads() - base, 2, "only the fast group");
    let base = reads();
    r.engine.poll_due();
    assert_eq!(reads() - base, 0, "nothing due");
    let base = reads();
    r.clock.advance_millis(900);
    r.engine.poll_due();
    let slow_readable = 6; // count, status, level, arr, sine (+ motor/speed fast)
    assert!(
        reads() - base >= slow_readable as u64 - 1,
        "{}",
        reads() - base
    );
}

#[test]
fn large_scan_groups_are_split_by_driver_request_size() {
    let mut v = project_json();
    let tags = v["tags"].as_array_mut().unwrap();
    for i in 0..150 {
        tags.push(
            json!({ "id": format!("bulk-{i}"), "name": format!("Bulk/{i}"), "dataType": "int32",
                          "connection": "conn-sim", "address": format!("DB9.X{i}") }),
        );
    }
    let r = rig_with(&project_from(v), ScanMode::Manual);
    r.engine.poll_all();
    let st = r.plc.stats();
    // 150 bulk + other readable tags, max 64 per request.
    assert!(st.read_requests >= 3, "{st:?}");
    assert!(r.engine.read(&tid("bulk-149")).unwrap().quality.is_good());
}

/// Same scenario twice → identical event logs (deterministic simulation).
#[test]
fn deterministic_scenarios_are_repeatable() {
    let run = || {
        let r = rig();
        let (_sub, rx) = r.engine.subscribe(None).unwrap();
        let script: Vec<Vec<SimStep>> = vec![
            vec![SimStep::Set {
                address: "DB1.Count".into(),
                value: TagValue::UInt32(1),
            }],
            vec![SimStep::Online(false)],
            vec![
                SimStep::Online(true),
                SimStep::Set {
                    address: "DB1.Level".into(),
                    value: TagValue::Int16(-3),
                },
            ],
            vec![SimStep::FailNext {
                kind: DriverErrorKind::Protocol,
                count: 1,
            }],
            vec![],
        ];
        let mut log = Vec::new();
        for (i, steps) in script.iter().enumerate() {
            r.plc.apply(steps);
            r.clock.advance_millis(1000);
            r.engine.poll_due();
            if i == 2 {
                r.engine
                    .write(&tid("sim-speed"), TagValue::Float32(5.0))
                    .unwrap();
            }
            for e in drain(&rx) {
                log.push(serde_json::to_string(&e).unwrap());
            }
        }
        log
    };
    let a = run();
    let b = run();
    assert!(a.len() > 20, "{}", a.len());
    assert_eq!(a, b);
}

#[test]
fn unavailable_driver_is_reported_and_inert() {
    let r = connected();
    assert!(matches!(
        r.engine.connection_status(&cid("conn-opc")),
        Some(ConnectionStatus::Unavailable { .. })
    ));
    assert!(matches!(
        r.engine.write(&tid("opc-tag"), TagValue::Float64(1.0)),
        Err(TagError::AccessDenied { .. })
    ));
    let res = r.engine.read_device(&[tid("opc-tag")]);
    assert!(matches!(res[0], Err(TagError::Unavailable { .. })));
}

/// Regression: generator phase computed as `sin(2π·t/period)` with epoch
/// seconds lost ~2e-7 of precision; phase is now integer-microsecond based.
#[test]
fn regression_generator_phase_is_exact_at_epoch_scale() {
    let r = connected();
    for k in 0..8u64 {
        let TagValue::Float64(x) = r.engine.read(&tid("sim-sine")).unwrap().value else {
            panic!()
        };
        let expected = 10.0 + 2.0 * (std::f64::consts::TAU * (k % 4) as f64 / 4.0).sin();
        assert!((x - expected).abs() < 1e-12, "k={k}: {x} vs {expected}");
        r.clock.advance_millis(1000);
        r.engine.poll_all();
    }
}

/// Regression: tags with an engineering range and no initial value were
/// seeded with 0 even when 0 lies outside the range.
#[test]
fn regression_simulator_seeds_inside_the_engineering_range() {
    let mut v = project_json();
    v["tags"].as_array_mut().unwrap().push(json!({
        "id": "sim-volume", "name": "Volume", "dataType": "int32", "connection": "conn-sim",
        "address": "DB2.Vol", "access": "readWrite", "range": { "min": 250.5, "max": 1500 }
    }));
    let r = rig_with(&project_from(v), ScanMode::Manual);
    assert_eq!(r.plc.get("DB2.Vol"), Some(TagValue::Int32(251)));
    r.engine.poll_all();
    assert!(r.engine.read(&tid("sim-volume")).unwrap().quality.is_good());
}
