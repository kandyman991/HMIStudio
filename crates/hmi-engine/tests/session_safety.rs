//! REUSE-02C.1: session invalidation while a driver operation is in progress, uncertain write
//! outcomes, pulse release, and no automatic replay or re-arming. Uses a scripted simulated
//! driver only (no device, no network); every blocking point is released by the test.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use hmi_engine::driver::{
    Driver, DriverError, DriverEvent, DriverFactory, EventSink, OUTCOME_UNKNOWN,
};
use hmi_engine::engine;
use hmi_engine::protocol::{
    Access, BrowseNode, Message, Request, RuntimeConfig, RuntimePoint, RuntimeSource, SourceState,
    Value, ValueType, WriteRefusal, PROTOCOL_VERSION,
};
use tokio::sync::mpsc::{channel, unbounded_channel, Receiver, Sender, UnboundedReceiver};
use tokio::task::JoinHandle;

enum Step {
    Ok,
    /// Never completes by itself; dropping it is recorded.
    Hang,
    /// The device may have applied it.
    Unknown,
}

#[derive(Default)]
struct Log {
    writes: Vec<(String, Value)>,
    abandoned: usize,
    stops: usize,
}

struct Rig {
    log: Arc<Mutex<Log>>,
    steps: Arc<Mutex<VecDeque<Step>>>,
}

struct Scripted {
    source: String,
    sink: EventSink,
    log: Arc<Mutex<Log>>,
    steps: Arc<Mutex<VecDeque<Step>>>,
    entered: tokio::sync::mpsc::UnboundedSender<()>,
}

struct DropMark(Arc<Mutex<Log>>);
impl Drop for DropMark {
    fn drop(&mut self) {
        self.0.lock().unwrap().abandoned += 1;
    }
}

#[async_trait]
impl Driver for Scripted {
    async fn start(&mut self) -> Result<(), DriverError> {
        let _ = self.sink.send((
            self.source.clone(),
            DriverEvent::State(SourceState::Connected, None),
        ));
        Ok(())
    }
    async fn write(&mut self, point: &RuntimePoint, value: &Value) -> Result<(), DriverError> {
        self.log
            .lock()
            .unwrap()
            .writes
            .push((point.variable_id.clone(), value.clone()));
        let step = self.steps.lock().unwrap().pop_front().unwrap_or(Step::Ok);
        let _ = self.entered.send(());
        match step {
            Step::Ok => Ok(()),
            Step::Unknown => Err(DriverError::outcome_unknown("simulated transport failure")),
            Step::Hang => {
                let _mark = DropMark(self.log.clone());
                std::future::pending::<()>().await;
                unreachable!()
            }
        }
    }
    async fn set_demand(&mut self, _: Option<&std::collections::BTreeSet<String>>) {}
    async fn browse(&mut self, _: Option<&str>) -> Result<Vec<BrowseNode>, DriverError> {
        Ok(Vec::new())
    }
    async fn stop(&mut self) {
        self.log.lock().unwrap().stops += 1;
        let _ = self.sink.send((
            self.source.clone(),
            DriverEvent::State(SourceState::Disconnected, None),
        ));
    }
}

struct Session {
    tx: Option<Sender<Request>>,
    rx: Receiver<Message>,
    engine: JoinHandle<()>,
    entered: UnboundedReceiver<()>,
    next: u64,
    rig: Rig,
}

fn point(variable: &str, value_type: ValueType, pulse_ms: Option<u32>) -> RuntimePoint {
    RuntimePoint {
        mapping_id: format!("m-{variable}"),
        variable_id: variable.into(),
        source_id: "sim".into(),
        address: variable.into(),
        value_type,
        access: Access::ReadWrite,
        write_confirmed: true,
        limits: None,
        pulse_ms,
    }
}

impl Session {
    async fn start() -> Self {
        let log = Arc::new(Mutex::new(Log::default()));
        let steps = Arc::new(Mutex::new(VecDeque::new()));
        let (entered_tx, entered) = unbounded_channel();
        let factory: DriverFactory = {
            let (log, steps) = (log.clone(), steps.clone());
            Arc::new(move |source, _points, _credential, sink| {
                Ok(Box::new(Scripted {
                    source: source.id.clone(),
                    sink,
                    log: log.clone(),
                    steps: steps.clone(),
                    entered: entered_tx.clone(),
                }) as Box<dyn Driver>)
            })
        };
        let (tx, requests) = channel(256);
        let (out, rx) = channel(256);
        let engine = tokio::spawn(engine::run_with_drivers(requests, out, factory));
        let mut s = Session {
            tx: Some(tx),
            rx,
            engine,
            entered,
            next: 0,
            rig: Rig { log, steps },
        };
        let id = s.send(|id| Request::Hello {
            id,
            protocol: PROTOCOL_VERSION,
        });
        s.expect(
            "helloOk",
            |m| matches!(m, Message::HelloOk { id: got, .. } if *got == id),
        )
        .await;
        s.configure().await;
        let id = s.send(|id| Request::Start { id });
        s.ok(id).await;
        s.expect("connected", |m| {
            matches!(
                m,
                Message::SourceState {
                    state: SourceState::Connected,
                    ..
                }
            )
        })
        .await;
        s
    }

    async fn configure(&mut self) {
        let config = RuntimeConfig {
            version: 1,
            sources: vec![RuntimeSource {
                id: "sim".into(),
                driver: "loopback".into(), // a valid name; the factory supplies the simulation
                settings: Default::default(),
            }],
            points: vec![
                point("line.setpoint", ValueType::Double, None),
                point("line.reset", ValueType::Bool, Some(200)),
            ],
        };
        let id = self.send(|id| Request::Configure {
            id,
            config,
            credentials: Default::default(),
        });
        self.ok(id).await;
    }

    fn send(&mut self, make: impl FnOnce(u64) -> Request) -> u64 {
        self.next += 1;
        let id = self.next;
        self.tx
            .as_ref()
            .expect("client connected")
            .try_send(make(id))
            .unwrap();
        id
    }

    async fn expect(&mut self, what: &str, mut pick: impl FnMut(&Message) -> bool) -> Message {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        loop {
            let message = tokio::time::timeout_at(deadline, self.rx.recv())
                .await
                .unwrap_or_else(|_| panic!("timed out waiting for {what}"))
                .unwrap_or_else(|| panic!("engine output closed waiting for {what}"));
            if pick(&message) {
                return message;
            }
        }
    }

    async fn ok(&mut self, id: u64) {
        self.expect("ok", |m| match m {
            Message::Ok { id: got } => *got == id,
            Message::Error {
                id: got, message, ..
            } if *got == id => panic!("refused: {message}"),
            _ => false,
        })
        .await;
    }

    async fn arm(&mut self) {
        let id = self.send(|id| Request::ArmWrites {
            id,
            actor: "op".into(),
            ttl_seconds: 60,
        });
        self.ok(id).await;
    }

    async fn result(&mut self, id: u64) -> (bool, Option<WriteRefusal>, Option<String>) {
        match self
            .expect(
                "writeResult",
                |m| matches!(m, Message::WriteResult { id: got, .. } if *got == id),
            )
            .await
        {
            Message::WriteResult {
                accepted,
                refusal,
                message,
                ..
            } => (accepted, refusal, message),
            _ => unreachable!(),
        }
    }

    async fn entered(&mut self) {
        tokio::time::timeout(Duration::from_secs(3), self.entered.recv())
            .await
            .expect("the driver was never asked to write")
            .unwrap();
    }

    fn script(&self, step: Step) {
        self.rig.steps.lock().unwrap().push_back(step);
    }

    fn writes(&self) -> Vec<(String, Value)> {
        self.rig.log.lock().unwrap().writes.clone()
    }

    async fn ended(&mut self) {
        tokio::time::timeout(Duration::from_secs(3), &mut self.engine)
            .await
            .expect("the engine did not end its session")
            .unwrap();
    }

    fn write(&mut self, variable: &str, value: Value) -> u64 {
        let variable = variable.to_string();
        self.send(|id| Request::Write {
            id,
            variable_id: variable,
            value,
            actor: "op".into(),
        })
    }
}

fn unknown(message: &Option<String>) -> bool {
    message
        .as_deref()
        .is_some_and(|m| m.starts_with(OUTCOME_UNKNOWN))
}

#[tokio::test]
async fn client_disconnect_during_a_write_ends_the_session_and_drops_queued_requests() {
    let mut s = Session::start().await;
    s.arm().await;
    s.script(Step::Hang);
    s.write("line.setpoint", Value::Double(10.0));
    s.entered().await;
    // Queued behind the active write, then the client goes away.
    s.write("line.setpoint", Value::Double(20.0));
    drop(s.tx.take());
    s.ended().await;
    let log = s.rig.log.lock().unwrap();
    assert_eq!(
        log.abandoned, 1,
        "the engine stopped waiting for the active write"
    );
    assert_eq!(
        log.writes.len(),
        1,
        "the queued write of the departed client never ran"
    );
    assert_eq!(log.stops, 1, "the source was disconnected");
}

#[tokio::test]
async fn shutdown_is_not_queued_behind_an_active_write() {
    let mut s = Session::start().await;
    s.arm().await;
    s.script(Step::Hang);
    let first = s.write("line.setpoint", Value::Double(10.0));
    s.entered().await;
    s.write("line.setpoint", Value::Double(20.0)); // deferred; must not run after shutdown
    let shutdown = s.send(|id| Request::Shutdown { id });
    let (accepted, refusal, message) = s.result(first).await;
    assert!(!accepted);
    assert_eq!(refusal, Some(WriteRefusal::DriverError));
    assert!(
        unknown(&message),
        "abandoned write reported as outcome unknown: {message:?}"
    );
    s.ok(shutdown).await;
    s.ended().await;
    assert_eq!(
        s.writes().len(),
        1,
        "nothing queued before shutdown was executed"
    );
    assert_eq!(s.rig.log.lock().unwrap().stops, 1);
}

#[tokio::test]
async fn an_unknown_outcome_is_reported_and_never_retried() {
    let mut s = Session::start().await;
    s.arm().await;
    s.script(Step::Unknown);
    let id = s.write("line.setpoint", Value::Double(42.0));
    let (accepted, refusal, message) = s.result(id).await;
    assert!(!accepted);
    assert_eq!(refusal, Some(WriteRefusal::DriverError));
    assert!(unknown(&message), "{message:?}");
    // Give the engine ticks and a reconnection in which it could retry; it must not.
    tokio::time::sleep(Duration::from_millis(700)).await;
    assert_eq!(s.writes().len(), 1, "no automatic replay");
}

#[tokio::test]
async fn a_pulse_start_with_unknown_outcome_is_still_released() {
    let mut s = Session::start().await;
    s.arm().await;
    s.script(Step::Unknown); // TRUE: maybe applied
    let id = s.send(|id| Request::Pulse {
        id,
        variable_id: "line.reset".into(),
        actor: "op".into(),
    });
    let (accepted, _, message) = s.result(id).await;
    assert!(!accepted);
    assert!(unknown(&message), "{message:?}");
    let released = s
        .expect("pulseReleased", |m| {
            matches!(m, Message::PulseReleased { .. })
        })
        .await;
    let Message::PulseReleased {
        released, message, ..
    } = released
    else {
        unreachable!()
    };
    assert!(released, "the FALSE was written");
    assert!(message.unwrap_or_default().contains("unknown"));
    assert_eq!(
        s.writes(),
        vec![
            ("line.reset".into(), Value::Bool(true)),
            ("line.reset".into(), Value::Bool(false)),
        ],
        "TRUE once, never repeated, then the release"
    );
}

#[tokio::test]
async fn a_client_disconnect_during_a_pulse_start_still_releases_it() {
    let mut s = Session::start().await;
    s.arm().await;
    s.script(Step::Hang); // TRUE in progress when the client goes
    s.send(|id| Request::Pulse {
        id,
        variable_id: "line.reset".into(),
        actor: "op".into(),
    });
    s.entered().await;
    drop(s.tx.take());
    s.ended().await;
    assert_eq!(
        s.writes(),
        vec![
            ("line.reset".into(), Value::Bool(true)),
            ("line.reset".into(), Value::Bool(false)),
        ],
        "a possibly started pulse is released before the source is disconnected"
    );
    assert_eq!(s.rig.log.lock().unwrap().stops, 1);
}

#[tokio::test]
async fn arming_is_never_restored_automatically() {
    let mut s = Session::start().await;
    s.arm().await;
    let id = s.write("line.setpoint", Value::Double(1.0));
    assert!(s.result(id).await.0);
    // Stop and start again: a new control period, not armed.
    let id = s.send(|id| Request::Stop { id });
    s.ok(id).await;
    let id = s.send(|id| Request::Start { id });
    s.ok(id).await;
    s.expect("connected", |m| {
        matches!(
            m,
            Message::SourceState {
                state: SourceState::Connected,
                ..
            }
        )
    })
    .await;
    let id = s.write("line.setpoint", Value::Double(2.0));
    assert_eq!(s.result(id).await.1, Some(WriteRefusal::WritesDisarmed));
    // A new configuration is also disarmed.
    s.arm().await;
    s.configure().await;
    let id = s.send(|id| Request::Start { id });
    s.ok(id).await;
    let id = s.write("line.setpoint", Value::Double(3.0));
    assert_eq!(s.result(id).await.1, Some(WriteRefusal::WritesDisarmed));
    assert_eq!(
        s.writes().len(),
        1,
        "only the armed write reached the driver"
    );
}
