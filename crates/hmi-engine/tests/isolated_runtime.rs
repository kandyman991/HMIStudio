//! Exercise the real isolated engine dispatch and memory driver without production arming.
use hmi_engine::protocol::*;
use std::time::Duration;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::time::timeout;

struct Harness {
    tx: Sender<Request>,
    rx: Receiver<Message>,
}

impl Harness {
    fn new() -> Self {
        let (tx, requests) = channel(256);
        let (out, rx) = channel(256);
        tokio::spawn(hmi_engine::engine::run_isolated(requests, out));
        Self { tx, rx }
    }

    async fn next(&mut self, accept: impl Fn(&Message) -> bool) -> Message {
        timeout(Duration::from_secs(5), async {
            loop {
                let message = self.rx.recv().await.expect("engine stopped");
                if accept(&message) {
                    return message;
                }
            }
        })
        .await
        .expect("timed out waiting for isolated engine")
    }

    async fn ask(&mut self, request: Request, id: u64) -> Message {
        self.tx.send(request).await.unwrap();
        self.next(|m| match m {
            Message::HelloOk { id: got, .. }
            | Message::Ok { id: got }
            | Message::Error { id: got, .. }
            | Message::WriteResult { id: got, .. } => *got == id,
            _ => false,
        })
        .await
    }

    async fn hello(&mut self) {
        let reply = self
            .ask(Request::Hello { id: 1, protocol: PROTOCOL_VERSION }, 1)
            .await;
        assert!(matches!(reply, Message::HelloOk { drivers, .. } if drivers == vec!["loopback"]));
    }

    async fn configure(&mut self, id: u64, config: RuntimeConfig) -> Message {
        self.ask(Request::Configure { id, config, credentials: Default::default() }, id).await
    }
}

fn config() -> RuntimeConfig {
    RuntimeConfig {
        version: CONFIG_VERSION,
        sources: vec![RuntimeSource {
            id: "sim".into(),
            driver: "loopback".into(),
            settings: Default::default(),
        }],
        points: vec![RuntimePoint {
            mapping_id: "reset".into(),
            variable_id: "reset".into(),
            source_id: "sim".into(),
            address: "reset".into(),
            value_type: ValueType::Bool,
            access: Access::ReadWrite,
            write_confirmed: false,
            limits: None,
            pulse_ms: Some(100),
        }, RuntimePoint {
            mapping_id: "setpoint".into(),
            variable_id: "setpoint".into(),
            source_id: "sim".into(),
            address: "setpoint".into(),
            value_type: ValueType::Int,
            access: Access::ReadWrite,
            write_confirmed: false,
            limits: Some(WriteLimits { min: 0.0, max: 10.0 }),
            pulse_ms: None,
        }],
    }
}

#[tokio::test]
async fn isolated_reads_writes_and_pulses_need_no_production_arming() {
    let mut h = Harness::new();
    h.hello().await;
    assert!(matches!(h.configure(2, config()).await, Message::Ok { .. }));
    assert!(matches!(h.ask(Request::Start { id: 3 }, 3).await, Message::Ok { .. }));
    h.next(|m| matches!(m, Message::Values { values } if values.iter().any(|v| v.variable_id == "setpoint" && v.value == Some(Value::Int(0))))).await;
    let write = |id, value| Request::Write {
        id,
        variable_id: "setpoint".into(),
        value: Value::Int(value),
        actor: "simulation".into(),
    };
    assert!(matches!(h.ask(write(4, 5), 4).await, Message::WriteResult { accepted: true, .. }));
    h.next(|m| matches!(m, Message::Values { values } if values.iter().any(|v| v.variable_id == "setpoint" && v.value == Some(Value::Int(5))))).await;
    assert!(matches!(h.ask(write(5, 11), 5).await, Message::WriteResult { refusal: Some(WriteRefusal::OutOfLimits), .. }));
    assert!(matches!(h.ask(Request::Pulse { id: 6, variable_id: "reset".into(), actor: "simulation".into() }, 6).await, Message::WriteResult { accepted: true, .. }));
    h.next(|m| matches!(m, Message::PulseReleased { released: true, .. })).await;
    assert!(matches!(h.ask(Request::Shutdown { id: 7 }, 7).await, Message::Ok { .. }));
}

#[tokio::test]
async fn isolated_configure_rejects_physical_drivers_credentials_and_settings() {
    let mut h = Harness::new();
    h.hello().await;
    for (id, driver) in [(2, "s7"), (4, "opcua"), (6, "custom")] {
        let mut bad = config();
        bad.sources[0].driver = driver.into();
        assert!(matches!(h.configure(id, bad).await, Message::Error { code, .. } if code == "INVALID_CONFIG"));
        assert!(matches!(h.ask(Request::Start { id: id + 1 }, id + 1).await, Message::Error { code, .. } if code == "NOT_CONFIGURED"));
    }
    let mut bad = config();
    bad.sources[0].settings.insert("endpoint".into(), "synthetic.invalid".into());
    assert!(matches!(h.configure(8, bad).await, Message::Error { code, .. } if code == "INVALID_CONFIG"));
    assert!(matches!(h.configure(9, config()).await, Message::Ok { .. }));
    let credentials = std::collections::BTreeMap::from([("sim".into(), SourceCredential { username: "synthetic".into(), password: "synthetic".into() })]);
    assert!(matches!(h.ask(Request::Configure { id: 10, config: config(), credentials }, 10).await, Message::Error { code, .. } if code == "INVALID_CONFIG"));
    assert!(matches!(h.ask(Request::Start { id: 11 }, 11).await, Message::Error { code, .. } if code == "NOT_CONFIGURED"));
}
