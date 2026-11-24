//! IDE-07: momentary commands are pulsed by the engine itself — TRUE now, FALSE after the point's
//! own pulse time — and a started pulse is always released, armed or not, also on stop.

use std::time::Duration;

use hmi_engine::engine;
use hmi_engine::protocol::{
    Access, Message, Request, RuntimeConfig, RuntimePoint, RuntimeSource, Value, ValueType,
    WriteLimits, WriteRefusal, PROTOCOL_VERSION,
};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

struct Harness {
    requests: UnboundedSender<Request>,
    messages: UnboundedReceiver<Message>,
    next_id: u64,
}

impl Harness {
    fn start() -> Self {
        let (requests, request_rx) = unbounded_channel();
        let (out, messages) = unbounded_channel();
        tokio::spawn(engine::run(request_rx, out));
        Harness {
            requests,
            messages,
            next_id: 1,
        }
    }

    fn id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    /// Next message matching [pick], failing after two seconds.
    async fn expect<T>(&mut self, what: &str, mut pick: impl FnMut(&Message) -> Option<T>) -> T {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        loop {
            let message = tokio::time::timeout_at(deadline, self.messages.recv())
                .await
                .unwrap_or_else(|_| panic!("timed out waiting for {what}"))
                .expect("engine stopped");
            if let Some(found) = pick(&message) {
                return found;
            }
        }
    }

    async fn ok(&mut self, request: Request, id: u64) {
        self.requests.send(request).unwrap();
        self.expect("ok", |m| match m {
            Message::Ok { id: got } if *got == id => Some(()),
            Message::Error {
                id: got, message, ..
            } if *got == id => panic!("refused: {message}"),
            _ => None,
        })
        .await
    }

    async fn result(&mut self, request: Request, id: u64) -> (bool, Option<WriteRefusal>) {
        self.requests.send(request).unwrap();
        self.expect("writeResult", |m| match m {
            Message::WriteResult {
                id: got,
                accepted,
                refusal,
                ..
            } if *got == id => Some((*accepted, *refusal)),
            _ => None,
        })
        .await
    }

    async fn cell(&mut self, variable: &str) -> Value {
        let variable = variable.to_string();
        self.expect("a value", |m| match m {
            Message::Values { values } => values
                .iter()
                .find(|v| v.variable_id == variable)
                .and_then(|v| v.value.clone()),
            _ => None,
        })
        .await
    }
}

fn point(id: &str, variable: &str, value_type: ValueType, access: Access) -> RuntimePoint {
    RuntimePoint {
        mapping_id: id.into(),
        variable_id: variable.into(),
        source_id: "rig".into(),
        address: variable.into(),
        value_type,
        access,
        write_confirmed: access != Access::Read,
        limits: None,
        pulse_ms: None,
    }
}

async fn configured(pulse_ms: u32) -> Harness {
    let mut h = Harness::start();
    let id = h.id();
    h.requests
        .send(Request::Hello {
            id,
            protocol: PROTOCOL_VERSION,
        })
        .unwrap();
    h.expect("helloOk", |m| {
        matches!(m, Message::HelloOk { .. }).then_some(())
    })
    .await;
    let config = RuntimeConfig {
        version: 1,
        sources: vec![RuntimeSource {
            id: "rig".into(),
            driver: "loopback".into(),
            settings: Default::default(),
        }],
        points: vec![
            RuntimePoint {
                pulse_ms: Some(pulse_ms),
                ..point("reset", "line.reset", ValueType::Bool, Access::ReadWrite)
            },
            RuntimePoint {
                limits: Some(WriteLimits {
                    min: 0.0,
                    max: 100.0,
                }),
                ..point("sp", "line.setpoint", ValueType::Double, Access::ReadWrite)
            },
        ],
    };
    let id = h.id();
    h.ok(
        Request::Configure {
            id,
            config,
            credentials: Default::default(),
        },
        id,
    )
    .await;
    let id = h.id();
    h.ok(Request::Start { id }, id).await;
    let id = h.id();
    h.ok(
        Request::ArmWrites {
            id,
            actor: "olga".into(),
            ttl_seconds: 60,
        },
        id,
    )
    .await;
    h
}

#[tokio::test]
async fn a_pulse_is_true_then_false_after_its_own_time_and_reported() {
    let mut h = configured(150).await;
    let id = h.id();
    let started = tokio::time::Instant::now();
    assert_eq!(
        h.result(
            Request::Pulse {
                id,
                variable_id: "line.reset".into(),
                actor: "olga".into()
            },
            id
        )
        .await,
        (true, None)
    );
    assert_eq!(h.cell("line.reset").await, Value::Bool(true));

    // A second pulse while the first runs is refused.
    let id = h.id();
    assert_eq!(
        h.result(
            Request::Pulse {
                id,
                variable_id: "line.reset".into(),
                actor: "olga".into()
            },
            id
        )
        .await,
        (false, Some(WriteRefusal::Busy))
    );

    let released = h
        .expect("pulseReleased", |m| match m {
            Message::PulseReleased {
                variable_id,
                released,
                actor,
                ..
            } if variable_id == "line.reset" => Some((*released, actor.clone())),
            _ => None,
        })
        .await;
    assert_eq!(released, (true, "olga".to_string()));
    assert!(
        started.elapsed() >= Duration::from_millis(150),
        "released after the pulse time, not before"
    );
    assert_eq!(h.cell("line.reset").await, Value::Bool(false));
}

#[tokio::test]
async fn plain_writes_to_a_momentary_command_and_values_outside_limits_are_refused() {
    let mut h = configured(150).await;
    let id = h.id();
    let refused = h
        .result(
            Request::Write {
                id,
                variable_id: "line.reset".into(),
                value: Value::Bool(true),
                actor: "olga".into(),
            },
            id,
        )
        .await;
    assert_eq!(refused, (false, Some(WriteRefusal::WrongCommand)));
    let id = h.id();
    let pulse = h
        .result(
            Request::Pulse {
                id,
                variable_id: "line.setpoint".into(),
                actor: "olga".into(),
            },
            id,
        )
        .await;
    assert_eq!(
        pulse,
        (false, Some(WriteRefusal::TypeMismatch)),
        "a pulse is a BOOL; the type rule comes first"
    );
    let id = h.id();
    let high = h
        .result(
            Request::Write {
                id,
                variable_id: "line.setpoint".into(),
                value: Value::Double(100.5),
                actor: "olga".into(),
            },
            id,
        )
        .await;
    assert_eq!(high, (false, Some(WriteRefusal::OutOfLimits)));
    let id = h.id();
    let fine = h
        .result(
            Request::Write {
                id,
                variable_id: "line.setpoint".into(),
                value: Value::Double(55.0),
                actor: "olga".into(),
            },
            id,
        )
        .await;
    assert_eq!(fine, (true, None));
}

#[tokio::test]
async fn a_running_pulse_is_released_when_writes_are_disarmed_and_on_stop() {
    let mut h = configured(5_000).await;
    let id = h.id();
    assert_eq!(
        h.result(
            Request::Pulse {
                id,
                variable_id: "line.reset".into(),
                actor: "olga".into()
            },
            id
        )
        .await,
        (true, None)
    );
    // Disarm forces FALSE now, without waiting for the five-second pulse timer.
    let id = h.id();
    h.requests.send(Request::DisarmWrites { id }).unwrap();
    let released = h
        .expect("pulseReleased on disarm", |m| match m {
            Message::PulseReleased { released, .. } => Some(*released),
            _ => None,
        })
        .await;
    assert!(released, "FALSE written immediately when disarmed");
    // Stopping after a successful release is also safe.
    let id = h.id();
    h.ok(Request::Stop { id }, id).await;
}

#[tokio::test]
async fn only_the_actor_who_armed_the_engine_may_write_or_pulse() {
    let mut h = configured(500).await;
    let id = h.id();
    assert_eq!(
        h.result(
            Request::Write {
                id,
                variable_id: "line.setpoint".into(),
                value: Value::Double(25.0),
                actor: "someone-else".into(),
            },
            id
        )
        .await,
        (false, Some(WriteRefusal::WritesDisarmed)),
    );
    let id = h.id();
    assert_eq!(
        h.result(
            Request::Pulse {
                id,
                variable_id: "line.reset".into(),
                actor: "someone-else".into(),
            },
            id
        )
        .await,
        (false, Some(WriteRefusal::WritesDisarmed)),
    );
}
