//! The engine with its real OPC UA driver against a real (in-process) OPC UA server.

use std::time::Duration;

mod common;

use common::{tempdir, Harness};
use hmi_engine::protocol::*;
use hmi_engine::testserver::{self, TestServer, PASSWORD, USER};
use opcua::types::Variant;

fn point(
    server: &TestServer,
    variable: &str,
    node: &str,
    value_type: ValueType,
    access: Access,
) -> RuntimePoint {
    RuntimePoint {
        mapping_id: format!("m-{variable}"),
        variable_id: variable.into(),
        source_id: "plc".into(),
        address: server.node(node),
        value_type,
        access,
        write_confirmed: access != Access::Read,
        limits: None,
        pulse_ms: None,
    }
}

fn config(server: &TestServer, extra: &[(&str, &str)], points: Vec<RuntimePoint>) -> RuntimeConfig {
    let mut settings = std::collections::BTreeMap::new();
    settings.insert("endpoint".to_owned(), server.endpoint());
    settings.insert("publishingIntervalMs".to_owned(), "100".to_owned());
    for (k, v) in extra {
        settings.insert(k.to_string(), v.to_string());
    }
    RuntimeConfig {
        version: CONFIG_VERSION,
        sources: vec![RuntimeSource {
            id: "plc".into(),
            driver: "opcua".into(),
            settings,
        }],
        points,
    }
}

fn standard_points(server: &TestServer) -> Vec<RuntimePoint> {
    vec![
        point(
            server,
            "line.running",
            "Demo.Running",
            ValueType::Bool,
            Access::ReadWrite,
        ),
        point(
            server,
            "line.counter",
            "Demo.Counter",
            ValueType::Int,
            Access::Read,
        ),
        point(
            server,
            "line.speed",
            "Demo.Speed",
            ValueType::Float,
            Access::ReadWrite,
        ),
        point(
            server,
            "line.setpoint",
            "Demo.Setpoint",
            ValueType::Int,
            Access::ReadWrite,
        ),
        point(
            server,
            "line.name",
            "Demo.Name",
            ValueType::String,
            Access::ReadWrite,
        ),
    ]
}

async fn server() -> (TestServer, tempdir::Dir) {
    let dir = tempdir::Dir::new();
    let server = testserver::start(0, dir.path.join("server"))
        .await
        .expect("test server");
    (server, dir)
}

#[tokio::test(flavor = "multi_thread")]
async fn reads_initial_values_and_subscribes_to_changes() {
    let (server, dir) = server().await;
    let mut engine = Harness::new().await;
    let pki = dir.path.join("client").display().to_string();
    engine
        .ok(|id| Request::Configure {
            id,
            config: config(&server, &[("pkiDir", &pki)], standard_points(&server)),
            credentials: Default::default(),
        })
        .await;
    engine.ok(|id| Request::Start { id }).await;
    engine.connected("plc").await;

    assert_eq!(
        engine.value("line.name", |_| true).await,
        Value::String("line-1".into())
    );
    // The counter keeps changing on the server: we must see it move through the subscription.
    let first = match engine.value("line.counter", |_| true).await {
        Value::Int(v) => v,
        other => panic!("{other:?}"),
    };
    let later = engine
        .value(
            "line.counter",
            |v| matches!(v, Value::Int(n) if *n > first + 3),
        )
        .await;
    assert!(matches!(later, Value::Int(_)));
    assert!(server.writes().is_empty(), "reading never writes");
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn guarded_writes_reach_the_device_only_when_every_rule_passes() {
    let (server, dir) = server().await;
    let mut engine = Harness::new().await;
    let pki = dir.path.join("client").display().to_string();
    let mut points = standard_points(&server);
    points.push(point(
        &server,
        "line.locked",
        "Demo.Locked",
        ValueType::Double,
        Access::Read,
    ));
    engine
        .ok(|id| Request::Configure {
            id,
            config: config(&server, &[("pkiDir", &pki)], points),
            credentials: Default::default(),
        })
        .await;
    engine.ok(|id| Request::Start { id }).await;
    engine.connected("plc").await;
    engine.value("line.setpoint", |_| true).await; // node types are known after the initial read

    let refused = |m: &Message| match m {
        Message::WriteResult {
            accepted: false,
            refusal,
            ..
        } => *refusal,
        other => panic!("expected a refusal, got {other:?}"),
    };
    // Not armed yet.
    assert_eq!(
        refused(&engine.write("line.running", Value::Bool(true)).await),
        Some(WriteRefusal::WritesDisarmed)
    );

    engine
        .ok(|id| Request::ArmWrites {
            id,
            actor: "tester".into(),
            ttl_seconds: 60,
        })
        .await;
    assert_eq!(
        refused(&engine.write("line.counter", Value::Int(5)).await),
        Some(WriteRefusal::NotWritable)
    );
    assert_eq!(
        refused(&engine.write("line.speed", Value::Int(5)).await),
        Some(WriteRefusal::TypeMismatch)
    );
    assert_eq!(
        refused(&engine.write("line.speed", Value::Float(f32::NAN)).await),
        Some(WriteRefusal::NotFinite)
    );
    assert_eq!(
        refused(&engine.write("nothing.here", Value::Bool(true)).await),
        Some(WriteRefusal::UnknownVariable)
    );
    // Int16 node: a value that does not fit is refused, not truncated.
    let too_big = engine.write("line.setpoint", Value::Int(70_000)).await;
    assert_eq!(refused(&too_big), Some(WriteRefusal::DriverError));
    assert!(
        server.writes().is_empty(),
        "no refused write reached the device: {:?}",
        server.writes()
    );

    // Accepted writes, each converted to the node's own type, and read back by subscription.
    assert!(matches!(
        engine.write("line.running", Value::Bool(true)).await,
        Message::WriteResult { accepted: true, .. }
    ));
    assert!(matches!(
        engine.write("line.setpoint", Value::Int(1200)).await,
        Message::WriteResult { accepted: true, .. }
    ));
    assert!(matches!(
        engine.write("line.speed", Value::Float(42.5)).await,
        Message::WriteResult { accepted: true, .. }
    ));
    assert_eq!(
        server.writes(),
        vec![
            ("Demo.Running".to_owned(), Variant::Boolean(true)),
            ("Demo.Setpoint".to_owned(), Variant::Int16(1200)),
            ("Demo.Speed".to_owned(), Variant::Float(42.5)),
        ]
    );
    assert_eq!(
        engine
            .value("line.setpoint", |v| *v == Value::Int(1200))
            .await,
        Value::Int(1200)
    );

    // Every attempt was audited, accepted or refused.
    let audits: Vec<(bool, Option<WriteRefusal>)> = engine
        .all
        .iter()
        .filter_map(|m| match m {
            Message::WriteAudit {
                accepted, refusal, ..
            } => Some((*accepted, *refusal)),
            _ => None,
        })
        .collect();
    assert_eq!(audits.len(), 9, "every attempt audited: {audits:?}");
    assert_eq!(audits.iter().filter(|(accepted, _)| *accepted).count(), 3);

    // Disarming stops writes again.
    engine.ok(|id| Request::DisarmWrites { id }).await;
    assert_eq!(
        refused(&engine.write("line.running", Value::Bool(false)).await),
        Some(WriteRefusal::WritesDisarmed)
    );
    assert_eq!(server.writes().len(), 3);
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn arming_expires_and_a_new_configuration_disarms() {
    let (server, dir) = server().await;
    let mut engine = Harness::new().await;
    let pki = dir.path.join("client").display().to_string();
    let cfg = config(&server, &[("pkiDir", &pki)], standard_points(&server));
    engine
        .ok(|id| Request::Configure {
            id,
            config: cfg.clone(),
            credentials: Default::default(),
        })
        .await;
    engine.ok(|id| Request::Start { id }).await;
    engine.connected("plc").await;

    engine
        .ok(|id| Request::ArmWrites {
            id,
            actor: "tester".into(),
            ttl_seconds: 1,
        })
        .await;
    engine
        .wait_for("disarm", |m| {
            matches!(m, Message::WritesArmed { armed: false, .. })
        })
        .await;
    let reply = engine.write("line.running", Value::Bool(true)).await;
    assert!(matches!(
        reply,
        Message::WriteResult {
            refusal: Some(WriteRefusal::WritesDisarmed),
            ..
        }
    ));

    engine
        .ok(|id| Request::ArmWrites {
            id,
            actor: "tester".into(),
            ttl_seconds: 60,
        })
        .await;
    engine
        .ok(|id| Request::Configure {
            id,
            config: cfg.clone(),
            credentials: Default::default(),
        })
        .await;
    let reply = engine.write("line.running", Value::Bool(true)).await;
    assert!(matches!(
        reply,
        Message::WriteResult {
            refusal: Some(WriteRefusal::WritesDisarmed | WriteRefusal::SourceNotConnected),
            ..
        }
    ));

    let id = engine.send(|id| Request::ArmWrites {
        id,
        actor: "tester".into(),
        ttl_seconds: 10_000,
    });
    assert!(
        matches!(engine.reply(id).await, Message::Error { .. }),
        "over-long arming is refused"
    );
    assert!(server.writes().is_empty());
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn connects_with_encryption_and_user_credentials() {
    let (server, dir) = server().await;
    let mut engine = Harness::new().await;
    let pki = dir.path.join("client").display().to_string();
    let mut credentials = std::collections::BTreeMap::new();
    credentials.insert(
        "plc".to_owned(),
        SourceCredential {
            username: USER.into(),
            password: PASSWORD.into(),
        },
    );
    let cfg = config(
        &server,
        &[
            ("pkiDir", &pki),
            ("securityPolicy", "Basic256Sha256"),
            ("securityMode", "SignAndEncrypt"),
            ("trustServerCertificate", "true"),
        ],
        standard_points(&server),
    );
    engine
        .ok(|id| Request::Configure {
            id,
            config: cfg,
            credentials,
        })
        .await;
    engine.ok(|id| Request::Start { id }).await;
    engine.connected("plc").await;
    assert_eq!(
        engine.value("line.name", |_| true).await,
        Value::String("line-1".into())
    );
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn browse_lists_the_device_address_space() {
    let (server, dir) = server().await;
    let mut engine = Harness::new().await;
    let pki = dir.path.join("client").display().to_string();
    engine
        .ok(|id| Request::Configure {
            id,
            config: config(&server, &[("pkiDir", &pki)], vec![]),
            credentials: Default::default(),
        })
        .await;
    engine.ok(|id| Request::Start { id }).await;
    engine.connected("plc").await;

    let id = engine.send(|id| Request::Browse {
        id,
        source_id: "plc".into(),
        node_id: None,
    });
    let Message::BrowseResult { nodes, .. } = engine.reply(id).await else {
        panic!("browse failed")
    };
    let demo = nodes
        .iter()
        .find(|n| n.display_name == "Demo")
        .expect("Demo folder at the root");
    let node_id = demo.node_id.clone();
    let id = engine.send(|id| Request::Browse {
        id,
        source_id: "plc".into(),
        node_id: Some(node_id),
    });
    let Message::BrowseResult { nodes, .. } = engine.reply(id).await else {
        panic!("browse failed")
    };
    let names: Vec<&str> = nodes.iter().map(|n| n.display_name.as_str()).collect();
    for expected in ["Running", "Counter", "Speed", "Setpoint", "Name", "Locked"] {
        assert!(names.contains(&expected), "{expected} in {names:?}");
    }
    assert!(nodes.iter().all(|n| n.node_class == "VARIABLE"));
    // Enough detail to import tags without typing anything: type, write permission and an
    // address that survives namespace renumbering.
    let named = |name: &str| nodes.iter().find(|n| n.display_name == name).unwrap();
    assert_eq!(named("Running").value_type, Some(ValueType::Bool));
    assert_eq!(named("Counter").value_type, Some(ValueType::Int));
    assert_eq!(named("Speed").value_type, Some(ValueType::Float));
    assert_eq!(named("Name").value_type, Some(ValueType::String));
    assert_eq!(named("Speed").writable, Some(true));
    assert_eq!(named("Counter").writable, Some(false));
    assert_eq!(
        named("Speed").stable_node_id.as_deref(),
        Some(format!("nsu={};s=Demo.Speed", testserver::NAMESPACE).as_str())
    );
    assert!(nodes.iter().all(|n| !n.has_children));
    assert!(server.writes().is_empty(), "browsing never writes");
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unreachable_device_fails_visibly_and_values_turn_bad_when_the_link_drops() {
    let (server, dir) = server().await;
    let pki = dir.path.join("client").display().to_string();

    // Nothing listening: the source reports FAILED and is retried, nothing crashes.
    let mut engine = Harness::new().await;
    let mut cfg = config(&server, &[("pkiDir", &pki)], standard_points(&server));
    cfg.sources[0]
        .settings
        .insert("endpoint".into(), "opc.tcp://127.0.0.1:1".into());
    engine
        .ok(|id| Request::Configure {
            id,
            config: cfg,
            credentials: Default::default(),
        })
        .await;
    engine.ok(|id| Request::Start { id }).await;
    // Regression: connecting must never block the engine. While the device is unreachable the
    // engine still answers immediately, including arming/disarming.
    let started = std::time::Instant::now();
    engine
        .ok(|id| Request::ArmWrites {
            id,
            actor: "tester".into(),
            ttl_seconds: 30,
        })
        .await;
    engine.ok(|id| Request::DisarmWrites { id }).await;
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "engine blocked for {:?}",
        started.elapsed()
    );
    engine
        .wait_for("failed", |m| {
            matches!(
                m,
                Message::SourceState {
                    state: SourceState::Failed,
                    ..
                }
            )
        })
        .await;
    let reply = engine.write("line.running", Value::Bool(true)).await;
    assert!(matches!(
        reply,
        Message::WriteResult {
            refusal: Some(WriteRefusal::WritesDisarmed),
            ..
        }
    ));
    engine.shutdown().await;

    // Connected, then the server goes away: values must not stay GOOD.
    let mut engine = Harness::new().await;
    engine
        .ok(|id| Request::Configure {
            id,
            config: config(&server, &[("pkiDir", &pki)], standard_points(&server)),
            credentials: Default::default(),
        })
        .await;
    engine.ok(|id| Request::Start { id }).await;
    engine.connected("plc").await;
    engine.value("line.counter", |_| true).await;
    server.stop();
    let started = std::time::Instant::now();
    engine.seen.clear();
    engine
        .wait_for("bad values", |m| match m {
            Message::Values { values } => values
                .iter()
                .any(|v| v.variable_id == "line.counter" && v.quality == Quality::Bad),
            _ => false,
        })
        .await;
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "stale values stayed GOOD for {:?}",
        started.elapsed()
    );
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn invalid_configurations_are_refused_before_anything_connects() {
    let (server, dir) = server().await;
    let mut engine = Harness::new().await;
    let pki = dir.path.join("client").display().to_string();
    let base = config(&server, &[("pkiDir", &pki)], standard_points(&server));

    let mut unconfirmed = base.clone();
    unconfirmed.points[0].write_confirmed = false;
    let mut double_read = base.clone();
    double_read.points.push(RuntimePoint {
        mapping_id: "dup".into(),
        ..base.points[1].clone()
    });
    let mut unknown_driver = base.clone();
    unknown_driver.sources[0].driver = "modbus".into();
    let mut future = base.clone();
    future.version = 99;

    for bad in [unconfirmed, double_read, unknown_driver, future] {
        let id = engine.send(|id| Request::Configure {
            id,
            config: bad,
            credentials: Default::default(),
        });
        assert!(
            matches!(engine.reply(id).await, Message::Error { code, .. } if code == "INVALID_CONFIG")
        );
    }
    let id = engine.send(|id| Request::Start { id });
    assert!(
        matches!(engine.reply(id).await, Message::Error { code, .. } if code == "NOT_CONFIGURED")
    );
    assert!(server.writes().is_empty());
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn namespace_uri_addresses_resolve_per_session_and_unknown_uris_fail_safe() {
    // Siemens servers publish their namespaces by URI; the index may differ per server, so
    // `nsu=<uri>;<id>` must be resolved against the live session, never assumed.
    let (server, dir) = server().await;
    let mut engine = Harness::new().await;
    let pki = dir.path.join("client").display().to_string();
    let by_uri = |variable: &str, address: String, value_type: ValueType| RuntimePoint {
        mapping_id: format!("m-{variable}"),
        variable_id: variable.into(),
        source_id: "plc".into(),
        address,
        value_type,
        access: Access::ReadWrite,
        write_confirmed: true,
        limits: None,
        pulse_ms: None,
    };
    let points = vec![
        by_uri(
            "line.speed",
            format!("nsu={};s=Demo.Speed", testserver::NAMESPACE),
            ValueType::Float,
        ),
        by_uri(
            "ghost",
            "nsu=urn:not-on-this-server;s=Demo.Speed".into(),
            ValueType::Float,
        ),
    ];
    engine
        .ok(|id| Request::Configure {
            id,
            config: config(&server, &[("pkiDir", &pki)], points),
            credentials: Default::default(),
        })
        .await;
    engine.ok(|id| Request::Start { id }).await;
    engine.connected("plc").await;

    assert!(matches!(
        engine.value("line.speed", |_| true).await,
        Value::Float(_)
    ));
    let ghost = engine
        .wait_for("ghost bad", |m| match m {
            Message::Values { values } => values.iter().any(|v| {
                v.variable_id == "ghost"
                    && v.quality == Quality::Bad
                    && v.status.as_deref() == Some("NamespaceNotFound")
            }),
            _ => false,
        })
        .await;
    assert!(matches!(ghost, Message::Values { .. }));

    engine
        .ok(|id| Request::ArmWrites {
            id,
            actor: "tester".into(),
            ttl_seconds: 60,
        })
        .await;
    match engine.write("ghost", Value::Float(1.0)).await {
        Message::WriteResult {
            accepted: false, ..
        } => {}
        other => panic!("an unresolved node must not be written: {other:?}"),
    }
    assert!(server.writes().is_empty());
    match engine.write("line.speed", Value::Float(42.5)).await {
        Message::WriteResult { accepted: true, .. } => {}
        other => panic!("{other:?}"),
    }
    assert_eq!(
        engine
            .value("line.speed", |v| *v == Value::Float(42.5))
            .await,
        Value::Float(42.5)
    );
    assert_eq!(server.writes().len(), 1);
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn only_the_variables_in_use_are_monitored() {
    let (server, dir) = server().await;
    let mut engine = Harness::new().await;
    let pki = dir.path.join("client").display().to_string();
    engine
        .ok(|id| Request::Configure {
            id,
            config: config(&server, &[("pkiDir", &pki)], standard_points(&server)),
            credentials: Default::default(),
        })
        .await;
    // Demand may be set before starting: the first subscription already honours it.
    engine
        .ok(|id| Request::SetDemand {
            id,
            variable_ids: Some(vec!["line.speed".into()]),
        })
        .await;
    engine.ok(|id| Request::Start { id }).await;
    engine.connected("plc").await;
    assert!(matches!(
        engine.value("line.speed", |_| true).await,
        Value::Float(_)
    ));
    tokio::time::sleep(Duration::from_millis(800)).await;
    engine.value("line.speed", |_| true).await; // drain what has arrived
    let counter_reports: Vec<&PointValue> = engine
        .all
        .iter()
        .filter_map(|m| match m {
            Message::Values { values } => Some(values),
            _ => None,
        })
        .flatten()
        .filter(|v| v.variable_id == "line.counter")
        .collect();
    assert_eq!(counter_reports.len(), 1, "{counter_reports:?}");
    assert_eq!(counter_reports[0].status.as_deref(), Some("NotPolled"));
    assert_eq!(counter_reports[0].quality, Quality::Uncertain);

    // In use again: the current value arrives at once and keeps following the device.
    engine
        .ok(|id| Request::SetDemand {
            id,
            variable_ids: Some(vec!["line.counter".into(), "line.speed".into()]),
        })
        .await;
    let first = match engine.value("line.counter", |_| true).await {
        Value::Int(v) => v,
        other => panic!("{other:?}"),
    };
    engine
        .value(
            "line.counter",
            |v| matches!(v, Value::Int(n) if *n > first + 2),
        )
        .await;

    // Out of use again, then everything back.
    engine
        .ok(|id| Request::SetDemand {
            id,
            variable_ids: Some(vec![]),
        })
        .await;
    engine
        .wait_for("all unused", |m| match m {
            Message::Values { values } => {
                values.len() == 2
                    && values
                        .iter()
                        .all(|v| v.status.as_deref() == Some("NotPolled"))
            }
            _ => false,
        })
        .await;
    engine
        .ok(|id| Request::SetDemand {
            id,
            variable_ids: None,
        })
        .await;
    assert_eq!(
        engine.value("line.name", |_| true).await,
        Value::String("line-1".into())
    );
    assert!(server.writes().is_empty());
    engine.shutdown().await;
}
