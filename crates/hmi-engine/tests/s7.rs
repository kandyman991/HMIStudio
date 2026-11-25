//! The engine with its S7 driver against an independent S7 implementation (`snap7-server`),
//! plus a byte-recording fake PLC for what the simulator does not model (single-bit writes).

mod common;

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::Harness;
use hmi_engine::protocol::*;
use hmi_engine::testserver_s7::{self, S7TestServer};

fn point(variable: &str, address: &str, value_type: ValueType, access: Access) -> RuntimePoint {
    RuntimePoint {
        mapping_id: format!("m-{variable}"),
        variable_id: variable.into(),
        source_id: "plc".into(),
        address: address.into(),
        value_type,
        access,
        write_confirmed: access != Access::Read,
        limits: None,
        pulse_ms: None,
    }
}

fn config(port: u16, points: Vec<RuntimePoint>) -> RuntimeConfig {
    let mut settings = BTreeMap::new();
    settings.insert("host".to_owned(), "127.0.0.1".to_owned());
    settings.insert("port".to_owned(), port.to_string());
    settings.insert("pollIntervalMs".to_owned(), "50".to_owned());
    settings.insert("timeoutMs".to_owned(), "1000".to_owned());
    RuntimeConfig {
        version: CONFIG_VERSION,
        sources: vec![RuntimeSource {
            id: "plc".into(),
            driver: "s7".into(),
            settings,
        }],
        points,
    }
}

fn line_points() -> Vec<RuntimePoint> {
    vec![
        point(
            "line.running",
            "DB10.X0.0",
            ValueType::Bool,
            Access::ReadWrite,
        ),
        point("line.fault", "DB10.DBX0.1", ValueType::Bool, Access::Read),
        point(
            "line.setpoint",
            "DB10.INT2",
            ValueType::Int,
            Access::ReadWrite,
        ),
        point(
            "line.speed",
            "DB10.REAL4",
            ValueType::Float,
            Access::ReadWrite,
        ),
        point(
            "line.name",
            "DB10.STRING8.20",
            ValueType::String,
            Access::ReadWrite,
        ),
        point("line.total", "DB10.UDINT30", ValueType::Long, Access::Read),
        point(
            "line.ratio",
            "DB10.LREAL34",
            ValueType::Double,
            Access::ReadWrite,
        ),
        point("line.counter", "DB11.DINT0", ValueType::Int, Access::Read),
        point("plc.flag", "M0.0", ValueType::Bool, Access::Read),
    ]
}

async fn started(server: &S7TestServer, points: Vec<RuntimePoint>) -> Harness {
    let mut engine = Harness::new().await;
    let cfg = config(server.port, points);
    engine
        .ok(|id| Request::Configure {
            id,
            config: cfg,
            credentials: Default::default(),
        })
        .await;
    engine.ok(|id| Request::Start { id }).await;
    engine.connected("plc").await;
    engine
}

fn refusal(message: &Message) -> Option<WriteRefusal> {
    match message {
        Message::WriteResult { refusal, .. } => *refusal,
        other => panic!("expected a write result, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn reads_every_supported_type_and_follows_changes_by_polling() {
    let server = testserver_s7::start(0).await.unwrap();
    let mut engine = started(&server, line_points()).await;

    assert_eq!(
        engine.value("line.setpoint", |_| true).await,
        Value::Int(100)
    );
    assert_eq!(
        engine.value("line.speed", |_| true).await,
        Value::Float(12.5)
    );
    assert_eq!(
        engine.value("line.name", |_| true).await,
        Value::String("line-1".into())
    );
    assert_eq!(
        engine.value("line.total", |_| true).await,
        Value::Long(4_000_000_000)
    );
    assert_eq!(
        engine.value("line.ratio", |_| true).await,
        Value::Double(0.25)
    );
    assert_eq!(engine.value("plc.flag", |_| true).await, Value::Bool(true));

    // The "PLC program" increments DB11.DINT0: polling must follow it.
    let first = match engine.value("line.counter", |_| true).await {
        Value::Int(v) => v,
        other => panic!("{other:?}"),
    };
    engine
        .value(
            "line.counter",
            |v| matches!(v, Value::Int(n) if *n > first + 3),
        )
        .await;

    // A change made in the PLC (not by us) is reported.
    server.store.write_bytes(10, 2, &(-7i16).to_be_bytes());
    assert_eq!(
        engine
            .value("line.setpoint", |v| *v == Value::Int(-7))
            .await,
        Value::Int(-7)
    );
    assert_eq!(
        server.writes(),
        1,
        "only the simulated program change above; reading never writes"
    );
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn guarded_writes_land_in_plc_memory_exactly() {
    let server = testserver_s7::start(0).await.unwrap();
    let mut engine = started(&server, line_points()).await;
    engine.value("line.setpoint", |_| true).await;

    // Disarmed: nothing reaches the PLC.
    assert_eq!(
        refusal(&engine.write("line.setpoint", Value::Int(1200)).await),
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
        refusal(&engine.write("line.total", Value::Long(1)).await),
        Some(WriteRefusal::NotWritable)
    );
    // INT is 16-bit in the PLC: 40000 does not fit and is refused, never wrapped.
    let too_big = engine.write("line.setpoint", Value::Int(40_000)).await;
    assert_eq!(refusal(&too_big), Some(WriteRefusal::DriverError));
    let too_long = engine
        .write(
            "line.name",
            Value::String("a name that is far too long".into()),
        )
        .await;
    assert_eq!(refusal(&too_long), Some(WriteRefusal::DriverError));
    assert_eq!(server.writes(), 0, "no refused write reached the PLC");
    assert_eq!(server.db10(2, 2), 100i16.to_be_bytes());

    // Accepted writes: exact big-endian bytes at exact offsets.
    assert_eq!(
        refusal(&engine.write("line.setpoint", Value::Int(1200)).await),
        None
    );
    assert_eq!(server.db10(2, 2), vec![0x04, 0xB0]);
    assert_eq!(
        refusal(&engine.write("line.speed", Value::Float(42.5)).await),
        None
    );
    assert_eq!(server.db10(4, 4), 42.5f32.to_be_bytes());
    assert_eq!(
        refusal(&engine.write("line.ratio", Value::Double(-1.5)).await),
        None
    );
    assert_eq!(server.db10(34, 8), (-1.5f64).to_be_bytes());
    assert_eq!(
        refusal(&engine.write("line.name", Value::String("OK".into())).await),
        None
    );
    assert_eq!(server.db10(8, 4), vec![20, 2, b'O', b'K']);
    assert_eq!(server.writes(), 4);
    // Neighbours untouched.
    assert_eq!(server.db10(30, 4), 4_000_000_000u32.to_be_bytes());

    // Read back through polling.
    assert_eq!(
        engine
            .value("line.setpoint", |v| *v == Value::Int(1200))
            .await,
        Value::Int(1200)
    );
    assert_eq!(
        engine
            .value("line.name", |v| *v == Value::String("OK".into()))
            .await,
        Value::String("OK".into())
    );

    // A BOOL write is a single-bit request (its telegram is checked in the next test).
    assert_eq!(
        refusal(&engine.write("line.running", Value::Bool(true)).await),
        None
    );
    assert_eq!(server.writes(), 5);

    engine.ok(|id| Request::DisarmWrites { id }).await;
    assert_eq!(
        refusal(&engine.write("line.setpoint", Value::Int(5)).await),
        Some(WriteRefusal::WritesDisarmed)
    );
    assert_eq!(server.writes(), 5);
    engine.shutdown().await;
}

/// A fake PLC that answers the S7 handshake and records every write telegram. It proves what
/// actually goes on the wire for a BOOL: a single-bit write (transport size BIT) at bit address
/// byte * 8 + bit, which a real PLC applies to that bit only.
fn recording_plc() -> (u16, Arc<Mutex<Vec<Vec<u8>>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let frames = Arc::new(Mutex::new(Vec::new()));
    let recorded = frames.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let recorded = recorded.clone();
            std::thread::spawn(move || loop {
                let mut header = [0u8; 4];
                if stream.read_exact(&mut header).is_err() {
                    return;
                }
                let length = u16::from_be_bytes([header[2], header[3]]) as usize;
                let mut body = vec![0u8; length - 4];
                if stream.read_exact(&mut body).is_err() {
                    return;
                }
                let mut frame = header.to_vec();
                frame.extend(&body);
                let reply: Vec<u8> = if frame[5] == 0xE0 {
                    // COTP connection confirm
                    let mut cc = frame.clone();
                    cc[5] = 0xD0;
                    cc
                } else if frame.len() == 25 && frame[17] == 0xF0 {
                    // Setup communication: PDU 480
                    let mut r = vec![
                        0x03, 0x00, 0x00, 27, 0x02, 0xF0, 0x80, 0x32, 0x03, 0, 0, 0x04, 0, 0, 0x08,
                        0, 0, 0x00, 0, 0xF0, 0, 0, 1, 0, 1,
                    ];
                    r.extend_from_slice(&480u16.to_be_bytes());
                    r
                } else if frame[17] == 0x05 {
                    recorded.lock().unwrap().push(frame.clone());
                    vec![
                        0x03, 0x00, 0x00, 22, 0x02, 0xF0, 0x80, 0x32, 0x03, 0, 0, 0x05, 0, 0, 0x02,
                        0, 0x01, 0, 0, 0x05, 0x01, 0xFF,
                    ]
                } else {
                    return; // no reads expected in this test
                };
                if stream.write_all(&reply).is_err() {
                    return;
                }
            });
        }
    });
    (port, frames)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_bool_write_is_a_single_bit_telegram_at_the_right_bit_address() {
    let (port, frames) = recording_plc();
    let mut engine = Harness::new().await;
    let cfg = config(
        port,
        vec![point(
            "line.start",
            "DB10.X12.3",
            ValueType::Bool,
            Access::Write,
        )],
    );
    engine
        .ok(|id| Request::Configure {
            id,
            config: cfg,
            credentials: Default::default(),
        })
        .await;
    engine.ok(|id| Request::Start { id }).await;
    engine.connected("plc").await;
    engine
        .ok(|id| Request::ArmWrites {
            id,
            actor: "tester".into(),
            ttl_seconds: 60,
        })
        .await;
    assert_eq!(
        refusal(&engine.write("line.start", Value::Bool(true)).await),
        None
    );

    let frames = frames.lock().unwrap().clone();
    assert_eq!(frames.len(), 1);
    let f = &frames[0];
    assert_eq!(f[17], 0x05, "write var");
    assert_eq!(f[22], 0x01, "transport size BIT, not BYTE");
    assert_eq!(u16::from_be_bytes([f[25], f[26]]), 10, "DB10");
    assert_eq!(f[27], 0x84, "area DB");
    let bit_address = ((f[28] as u32) << 16) | ((f[29] as u32) << 8) | f[30] as u32;
    assert_eq!(bit_address, 12 * 8 + 3, "bit address of X12.3");
    assert_eq!(f[32], 0x03, "data transport size BIT");
    assert_eq!(u16::from_be_bytes([f[33], f[34]]), 1, "one bit of payload");
    assert_eq!(f[35], 1, "value true");
    assert_eq!(f.len(), 36, "nothing else is written");
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn ambiguous_or_unsafe_addresses_are_refused_at_configure_time() {
    let server = testserver_s7::start(0).await.unwrap();
    let mut engine = Harness::new().await;
    let cases = [
        (
            point("a", "DB10.DBW2", ValueType::Int, Access::Read),
            "DB10.INT2",
        ),
        (
            point("a", "DB10.UDINT30", ValueType::Int, Access::Read),
            "without wrapping",
        ),
        (
            point("a", "DB10.REAL4", ValueType::Int, Access::Read),
            "without wrapping",
        ),
        (
            point("a", "DB0.INT0", ValueType::Int, Access::Read),
            "DB numbers start at 1",
        ),
        (
            point("a", "DB10.X0", ValueType::Bool, Access::Read),
            "bit number",
        ),
    ];
    for (bad, hint) in cases {
        let cfg = config(server.port, vec![bad]);
        let id = engine.send(|id| Request::Configure {
            id,
            config: cfg,
            credentials: Default::default(),
        });
        match engine.reply(id).await {
            Message::Error { code, message, .. } => {
                assert_eq!(code, "INVALID_CONFIG");
                assert!(
                    message.contains(hint),
                    "'{message}' should mention '{hint}'"
                );
            }
            other => panic!("expected INVALID_CONFIG, got {other:?}"),
        }
    }
    let mut no_host = config(server.port, vec![]);
    no_host.sources[0].settings.remove("host");
    let id = engine.send(|id| Request::Configure {
        id,
        config: no_host,
        credentials: Default::default(),
    });
    assert!(
        matches!(engine.reply(id).await, Message::Error { code, .. } if code == "INVALID_CONFIG")
    );
    assert_eq!(server.writes(), 0);
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn connection_loss_turns_values_bad_and_an_absent_plc_never_blocks_the_engine() {
    let server = testserver_s7::start(0).await.unwrap();
    let mut engine = started(&server, line_points()).await;
    engine.value("line.setpoint", |_| true).await;
    // The PLC goes offline mid-session: values must turn BAD, not stay at their last value.
    server.stop();
    engine
        .wait_for("bad after loss", |m| match m {
            Message::Values { values } => values
                .iter()
                .any(|v| v.variable_id == "line.setpoint" && v.quality == Quality::Bad),
            _ => false,
        })
        .await;
    engine.shutdown().await;

    let dead = {
        let probe = TcpListener::bind("127.0.0.1:0").unwrap();
        probe.local_addr().unwrap().port()
    };
    let mut engine = Harness::new().await;
    let cfg = config(dead, line_points());
    engine
        .ok(|id| Request::Configure {
            id,
            config: cfg,
            credentials: Default::default(),
        })
        .await;
    engine.ok(|id| Request::Start { id }).await;
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
    engine
        .wait_for("bad values", |m| match m {
            Message::Values { values } => values
                .iter()
                .any(|v| v.variable_id == "line.setpoint" && v.quality == Quality::Bad),
            _ => false,
        })
        .await;
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_plc_that_drops_the_connection_mid_session_is_reported_and_values_turn_bad() {
    let (port, _) = recording_plc(); // answers the handshake, then hangs up on the first read
    let mut engine = Harness::new().await;
    let cfg = config(
        port,
        vec![point(
            "line.setpoint",
            "DB10.INT2",
            ValueType::Int,
            Access::Read,
        )],
    );
    engine
        .ok(|id| Request::Configure {
            id,
            config: cfg,
            credentials: Default::default(),
        })
        .await;
    engine.ok(|id| Request::Start { id }).await;
    engine.connected("plc").await;
    engine
        .wait_for("connection lost", |m| {
            matches!(m, Message::SourceState { state: SourceState::Connecting, message: Some(text), .. } if text.contains("connection lost"))
        })
        .await;
    engine
        .wait_for("bad value", |m| match m {
            Message::Values { values } => values
                .iter()
                .any(|v| v.variable_id == "line.setpoint" && v.quality == Quality::Bad),
            _ => false,
        })
        .await;
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn only_the_variables_in_use_are_read_from_the_plc() {
    let server = testserver_s7::start(0).await.unwrap();
    let mut engine = started(&server, line_points()).await;
    engine.value("line.counter", |_| true).await;
    engine.value("line.setpoint", |_| true).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let all = server.take_reads();
    assert!(
        all.contains(&10) && all.contains(&11) && all.contains(&0),
        "{all:?}"
    );

    // Only the counter is on screen now: DB10 and the markers are left alone.
    engine
        .ok(|id| Request::SetDemand {
            id,
            variable_ids: Some(vec!["line.counter".into()]),
        })
        .await;
    let unused = engine
        .wait_for("not polled", |m| match m {
            Message::Values { values } => values
                .iter()
                .any(|v| v.variable_id == "line.setpoint" && v.status.is_some()),
            _ => false,
        })
        .await;
    let Message::Values { values } = unused else {
        unreachable!()
    };
    assert_eq!(values.len(), 8, "every other readable variable: {values:?}");
    assert!(values.iter().all(|v| v.quality == Quality::Uncertain
        && v.value.is_none()
        && v.status.as_deref() == Some("NotPolled")));
    tokio::time::sleep(Duration::from_millis(300)).await; // requests already under way
    server.take_reads();
    engine.seen.clear();
    let first = match engine.value("line.counter", |_| true).await {
        Value::Int(v) => v,
        other => panic!("{other:?}"),
    };
    engine
        .value(
            "line.counter",
            |v| matches!(v, Value::Int(n) if *n > first + 5),
        )
        .await;
    let reads = server.take_reads();
    assert!(
        reads.len() > 3 && reads.iter().all(|db| *db == 11),
        "{reads:?}"
    );
    assert!(
        engine.seen.iter().all(|m| match m {
            Message::Values { values } => values.iter().all(|v| v.variable_id == "line.counter"),
            _ => true,
        }),
        "nothing but the counter is reported"
    );

    // A value that changed while unused is reported the moment it is in use again.
    server.store.write_bytes(10, 2, &777i16.to_be_bytes());
    engine
        .ok(|id| Request::SetDemand {
            id,
            variable_ids: Some(vec!["line.setpoint".into()]),
        })
        .await;
    assert_eq!(
        engine
            .value("line.setpoint", |v| *v == Value::Int(777))
            .await,
        Value::Int(777)
    );
    tokio::time::sleep(Duration::from_millis(300)).await;
    server.take_reads();
    tokio::time::sleep(Duration::from_millis(400)).await;
    let reads = server.take_reads();
    assert!(
        !reads.is_empty() && reads.iter().all(|db| *db == 10),
        "{reads:?}"
    );

    // Writes do not depend on what is being read.
    engine
        .ok(|id| Request::ArmWrites {
            id,
            actor: "tester".into(),
            ttl_seconds: 60,
        })
        .await;
    assert_eq!(
        refusal(&engine.write("line.speed", Value::Float(3.5)).await),
        None
    );
    assert_eq!(server.db10(4, 4), 3.5f32.to_be_bytes());

    // Everything in use again.
    engine
        .ok(|id| Request::SetDemand {
            id,
            variable_ids: None,
        })
        .await;
    assert_eq!(
        engine
            .value("line.speed", |v| *v == Value::Float(3.5))
            .await,
        Value::Float(3.5)
    );
    engine
        .value("line.counter", |v| matches!(v, Value::Int(_)))
        .await;
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_variable_coming_into_use_while_the_plc_is_away_is_bad_not_silent() {
    let port = {
        let socket = TcpListener::bind("127.0.0.1:0").unwrap();
        socket.local_addr().unwrap().port() // closed again: nothing listens here
    };
    let mut engine = Harness::new().await;
    let cfg = config(port, line_points());
    engine
        .ok(|id| Request::Configure {
            id,
            config: cfg,
            credentials: Default::default(),
        })
        .await;
    engine.ok(|id| Request::Start { id }).await;
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
    engine
        .ok(|id| Request::SetDemand {
            id,
            variable_ids: Some(vec![]),
        })
        .await;
    engine.seen.clear();
    engine
        .ok(|id| Request::SetDemand {
            id,
            variable_ids: Some(vec!["line.setpoint".into()]),
        })
        .await;
    let report = engine
        .wait_for("bad", |m| match m {
            Message::Values { values } => values
                .iter()
                .any(|v| v.variable_id == "line.setpoint" && v.quality == Quality::Bad),
            _ => false,
        })
        .await;
    let Message::Values { values } = report else {
        unreachable!()
    };
    assert_eq!(values.len(), 1);
    assert_eq!(values[0].quality, Quality::Bad);
    assert_eq!(values[0].status.as_deref(), Some("NotConnected"));
    engine.shutdown().await;
}

/// A PLC that answers reads by block: DB1 with zeros, DB99 "does not exist" (0x0A), DB98
/// "invalid address" (0x05), anything else "access denied" (0x03).
fn refusing_plc() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            std::thread::spawn(move || loop {
                let mut header = [0u8; 4];
                if stream.read_exact(&mut header).is_err() {
                    return;
                }
                let length = u16::from_be_bytes([header[2], header[3]]) as usize;
                let mut frame = header.to_vec();
                frame.resize(length, 0);
                if stream.read_exact(&mut frame[4..]).is_err() {
                    return;
                }
                let reply: Vec<u8> = if frame[5] == 0xE0 {
                    let mut cc = frame.clone();
                    cc[5] = 0xD0;
                    cc
                } else if frame.len() == 25 && frame[17] == 0xF0 {
                    let mut r = vec![
                        0x03, 0x00, 0x00, 27, 0x02, 0xF0, 0x80, 0x32, 0x03, 0, 0, 0x04, 0, 0, 0x08,
                        0, 0, 0x00, 0, 0xF0, 0, 0, 1, 0, 1,
                    ];
                    r.extend_from_slice(&480u16.to_be_bytes());
                    r
                } else if frame[17] == 0x04 {
                    let count = u16::from_be_bytes([frame[23], frame[24]]) as usize;
                    let data: Vec<u8> = match u16::from_be_bytes([frame[25], frame[26]]) {
                        1 => {
                            let mut d = vec![0xFF, 0x04];
                            d.extend_from_slice(&((count * 8) as u16).to_be_bytes());
                            d.extend(std::iter::repeat_n(0u8, count));
                            d
                        }
                        99 => vec![0x0A, 0x00, 0x00, 0x00],
                        98 => vec![0x05, 0x00, 0x00, 0x00],
                        _ => vec![0x03, 0x00, 0x00, 0x00],
                    };
                    let mut r = vec![0x03, 0x00, 0, 0, 0x02, 0xF0, 0x80, 0x32, 0x03, 0, 0];
                    r.extend_from_slice(&frame[11..13]);
                    r.extend_from_slice(&[0, 2]);
                    r.extend_from_slice(&(data.len() as u16).to_be_bytes());
                    r.extend_from_slice(&[0, 0, 0x04, 0x01]);
                    r.extend(data);
                    let total = (r.len() as u16).to_be_bytes();
                    r[2] = total[0];
                    r[3] = total[1];
                    r
                } else {
                    return;
                };
                if stream.write_all(&reply).is_err() {
                    return;
                }
            });
        }
    });
    port
}

#[tokio::test(flavor = "multi_thread")]
async fn a_point_the_plc_refuses_gets_a_status_code_and_the_others_keep_working() {
    let port = refusing_plc();
    let mut engine = Harness::new().await;
    let cfg = config(
        port,
        vec![
            point("ok", "DB1.INT0", ValueType::Int, Access::Read),
            point("missing", "DB99.INT0", ValueType::Int, Access::Read),
            point("outside", "DB98.INT0", ValueType::Int, Access::Read),
            point("denied", "DB97.INT0", ValueType::Int, Access::Read),
        ],
    );
    engine
        .ok(|id| Request::Configure {
            id,
            config: cfg,
            credentials: Default::default(),
        })
        .await;
    engine.ok(|id| Request::Start { id }).await;
    engine.connected("plc").await;
    assert_eq!(engine.value("ok", |_| true).await, Value::Int(0));
    engine
        .wait_for("all refusals", |m| match m {
            Message::Values { values } => values.iter().any(|v| v.variable_id == "denied"),
            _ => false,
        })
        .await;
    let status = |variable: &str| {
        let report = &engine.latest[variable];
        assert_eq!(report.quality, Quality::Bad, "{variable}");
        assert!(report.value.is_none());
        report.status.clone().unwrap()
    };
    // Stable codes, not sentences: the client turns them into text and advice.
    assert_eq!(status("missing"), "AddressNotFound");
    assert_eq!(status("outside"), "AddressRefused");
    assert_eq!(status("denied"), "AccessDenied");
    assert_eq!(engine.latest["ok"].quality, Quality::Good);
    engine.shutdown().await;
}
