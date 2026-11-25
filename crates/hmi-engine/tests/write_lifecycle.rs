//! REUSE-02C.4A: write lifecycle and termination safety. A scripted simulated driver only (no
//! device, no network). Engine time bounds are shortened through `run_with_limits`; every
//! blocking point is either scripted or released by the test.

use std::collections::{BTreeSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use hmi_engine::driver::{
    Driver, DriverError, DriverEvent, DriverFactory, EventSink, NOT_TRANSMITTED, OUTCOME_UNKNOWN,
};
use hmi_engine::engine::{self, Limits};
use hmi_engine::protocol::{
    Access, BrowseNode, Message, Request, RuntimeConfig, RuntimePoint, RuntimeSource, SourceState,
    Value, ValueType, WriteRefusal, PROTOCOL_VERSION,
};
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::task::JoinHandle;

const LIMITS: Limits = Limits {
    write_timeout: Duration::from_millis(300),
    release_timeout: Duration::from_millis(300),
    stop_timeout: Duration::from_millis(300),
};

#[derive(Clone, Copy, Debug)]
enum Step {
    Ok,
    /// Never answers; dropping it is recorded.
    Hang,
    /// Transmission failed after the request left: the device may have applied it.
    Unknown,
    /// Certainly not transmitted.
    Refused,
}

#[derive(Default)]
struct Shared {
    /// Every write the driver was asked to perform, in order.
    writes: Vec<(String, Value)>,
    /// Write futures dropped before answering.
    withdrawn: usize,
    stops: usize,
    steps: VecDeque<Step>,
    /// `stop` never completes.
    stop_hangs: bool,
    sink: Option<(String, EventSink)>,
}

struct Scripted {
    source: String,
    sink: EventSink,
    shared: Arc<Mutex<Shared>>,
}

struct Withdrawn(Arc<Mutex<Shared>>);
impl Drop for Withdrawn {
    fn drop(&mut self) {
        self.0.lock().unwrap().withdrawn += 1;
    }
}

#[async_trait]
impl Driver for Scripted {
    async fn start(&mut self) -> Result<(), DriverError> {
        self.shared.lock().unwrap().sink = Some((self.source.clone(), self.sink.clone()));
        let _ = self.sink.send((
            self.source.clone(),
            DriverEvent::State(SourceState::Connected, None),
        ));
        Ok(())
    }
    async fn write(&mut self, point: &RuntimePoint, value: &Value) -> Result<(), DriverError> {
        let step = {
            let mut shared = self.shared.lock().unwrap();
            shared
                .writes
                .push((point.variable_id.clone(), value.clone()));
            shared.steps.pop_front().unwrap_or(Step::Ok)
        };
        match step {
            Step::Ok => Ok(()),
            Step::Unknown => Err(DriverError::outcome_unknown("simulated transport failure")),
            Step::Refused => Err(DriverError::not_transmitted("simulated: not connected")),
            Step::Hang => {
                let _mark = Withdrawn(self.shared.clone());
                std::future::pending::<()>().await;
                unreachable!()
            }
        }
    }
    async fn set_demand(&mut self, _: Option<&BTreeSet<String>>) {}
    async fn browse(&mut self, _: Option<&str>) -> Result<Vec<BrowseNode>, DriverError> {
        Ok(Vec::new())
    }
    async fn stop(&mut self) {
        let hangs = {
            let mut shared = self.shared.lock().unwrap();
            shared.stops += 1;
            shared.stop_hangs
        };
        if hangs {
            std::future::pending::<()>().await;
        }
    }
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

const SETPOINT: &str = "line.setpoint";
const RESET: &str = "line.reset";

struct Session {
    tx: Option<Sender<Request>>,
    rx: Receiver<Message>,
    engine: JoinHandle<()>,
    next: u64,
    shared: Arc<Mutex<Shared>>,
}

impl Session {
    /// Hello, configure, start, connected and armed by "op".
    async fn armed(pulse_ms: u32) -> Self {
        let shared = Arc::new(Mutex::new(Shared::default()));
        let factory: DriverFactory = {
            let shared = shared.clone();
            Arc::new(move |source, _points, _credential, sink| {
                Ok(Box::new(Scripted {
                    source: source.id.clone(),
                    sink,
                    shared: shared.clone(),
                }) as Box<dyn Driver>)
            })
        };
        let (tx, requests) = channel(256);
        let (out, rx) = channel(256);
        let engine = tokio::spawn(engine::run_with_limits(requests, out, factory, LIMITS));
        let mut s = Session {
            tx: Some(tx),
            rx,
            engine,
            next: 0,
            shared,
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
        let config = RuntimeConfig {
            version: 1,
            sources: vec![RuntimeSource {
                id: "sim".into(),
                driver: "loopback".into(), // a valid name; the factory supplies the simulation
                settings: Default::default(),
            }],
            points: vec![
                point(SETPOINT, ValueType::Double, None),
                point(RESET, ValueType::Bool, Some(pulse_ms)),
            ],
        };
        let id = s.send(|id| Request::Configure {
            id,
            config,
            credentials: Default::default(),
        });
        s.ok(id).await;
        let id = s.send(|id| Request::Start { id });
        s.ok(id).await;
        s.connected().await;
        s.arm().await;
        s
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

    async fn connected(&mut self) {
        self.expect("connected", |m| {
            matches!(
                m,
                Message::SourceState {
                    state: SourceState::Connected,
                    ..
                }
            )
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

    fn script(&self, steps: &[Step]) {
        self.shared
            .lock()
            .unwrap()
            .steps
            .extend(steps.iter().copied());
    }

    fn writes(&self) -> Vec<(String, Value)> {
        self.shared.lock().unwrap().writes.clone()
    }

    /// Emits a source state from the simulated driver, as a real driver would.
    fn source_state(&self, state: SourceState) {
        let (source, sink) = self.shared.lock().unwrap().sink.clone().expect("started");
        sink.send((source, DriverEvent::State(state, None)))
            .map_err(|_| ())
            .unwrap();
    }

    fn write(&mut self, value: f64) -> u64 {
        self.send(|id| Request::Write {
            id,
            variable_id: SETPOINT.into(),
            value: Value::Double(value),
            actor: "op".into(),
        })
    }

    fn pulse(&mut self) -> u64 {
        self.send(|id| Request::Pulse {
            id,
            variable_id: RESET.into(),
            actor: "op".into(),
        })
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

    async fn released(&mut self) -> (bool, Option<String>) {
        match self
            .expect("pulseReleased", |m| {
                matches!(m, Message::PulseReleased { .. })
            })
            .await
        {
            Message::PulseReleased {
                released, message, ..
            } => (released, message),
            _ => unreachable!(),
        }
    }

    async fn ended_within(&mut self, bound: Duration) {
        tokio::time::timeout(bound, &mut self.engine)
            .await
            .expect("the engine did not terminate within its bound")
            .unwrap();
    }
}

fn starts(message: &Option<String>, prefix: &str) -> bool {
    message.as_deref().is_some_and(|m| m.starts_with(prefix))
}

fn t(v: bool) -> (String, Value) {
    (RESET.into(), Value::Bool(v))
}

// ---- normal operation -------------------------------------------------------------------

#[tokio::test]
async fn a_normal_write_is_acknowledged_once_and_audited() {
    let mut s = Session::armed(200).await;
    let id = s.write(12.5);
    let audit = s
        .expect("writeAudit", |m| matches!(m, Message::WriteAudit { .. }))
        .await;
    assert!(
        matches!(audit, Message::WriteAudit { accepted: true, ref actor, .. } if actor == "op")
    );
    assert_eq!(s.result(id).await, (true, None, None));
    assert_eq!(s.writes(), vec![(SETPOINT.into(), Value::Double(12.5))]);
}

#[tokio::test]
async fn a_momentary_command_is_true_then_false_after_its_pulse_time() {
    let mut s = Session::armed(150).await;
    let started = Instant::now();
    let id = s.pulse();
    assert!(s.result(id).await.0);
    assert_eq!(s.released().await, (true, None));
    assert!(started.elapsed() >= Duration::from_millis(150));
    assert_eq!(s.writes(), vec![t(true), t(false)]);
}

// ---- cancellation and timeouts -------------------------------------------------------------

#[tokio::test]
async fn a_write_without_an_answer_times_out_as_outcome_unknown_and_is_withdrawn() {
    let mut s = Session::armed(200).await;
    s.script(&[Step::Hang]);
    let started = Instant::now();
    let id = s.write(1.0);
    let (accepted, refusal, message) = s.result(id).await;
    let took = started.elapsed();
    assert!(!accepted);
    assert_eq!(refusal, Some(WriteRefusal::DriverError));
    assert!(starts(&message, OUTCOME_UNKNOWN), "{message:?}");
    assert!(
        took >= LIMITS.write_timeout && took < LIMITS.write_timeout * 4,
        "{took:?}"
    );
    assert_eq!(
        s.shared.lock().unwrap().withdrawn,
        1,
        "the driver call was dropped"
    );
    // The engine is usable afterwards and repeats nothing on its own.
    let id = s.write(2.0);
    assert!(s.result(id).await.0);
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert_eq!(
        s.writes(),
        vec![
            (SETPOINT.into(), Value::Double(1.0)),
            (SETPOINT.into(), Value::Double(2.0))
        ],
        "no automatic replay of the timed-out write"
    );
}

#[tokio::test]
async fn a_pulse_start_that_times_out_is_released_at_once() {
    let mut s = Session::armed(5000).await;
    s.script(&[Step::Hang]);
    let id = s.pulse();
    let (accepted, _, message) = s.result(id).await;
    assert!(!accepted);
    assert!(starts(&message, OUTCOME_UNKNOWN), "{message:?}");
    let started = Instant::now();
    let (released, note) = s.released().await;
    assert!(released);
    assert!(note.unwrap_or_default().contains("unknown"));
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "not after the 5 s pulse time"
    );
    assert_eq!(s.writes(), vec![t(true), t(false)]);
}

// ---- uncertain outcomes --------------------------------------------------------------------

#[tokio::test]
async fn certain_refusal_and_unknown_outcome_are_distinguished() {
    let mut s = Session::armed(200).await;
    s.script(&[Step::Refused, Step::Unknown]);
    let a = s.write(1.0);
    let b = s.write(2.0);
    assert!(starts(&s.result(a).await.2, NOT_TRANSMITTED));
    assert!(starts(&s.result(b).await.2, OUTCOME_UNKNOWN));
    assert_eq!(s.writes().len(), 2, "neither is retried");
}

// ---- connection loss, no automatic re-arming (F-1) -----------------------------------------

#[tokio::test]
async fn connection_loss_disarms_and_reconnection_never_rearms() {
    let mut s = Session::armed(200).await;
    s.script(&[Step::Unknown]); // the write is cut off by the connection loss
    let id = s.write(5.0);
    assert!(starts(&s.result(id).await.2, OUTCOME_UNKNOWN));
    s.source_state(SourceState::Connecting);
    let disarmed = s
        .expect("writesArmed false", |m| {
            matches!(m, Message::WritesArmed { armed: false, .. })
        })
        .await;
    assert!(matches!(disarmed, Message::WritesArmed { actor: Some(ref a), .. } if a == "op"));
    s.source_state(SourceState::Connected);
    s.connected().await;
    let id = s.write(6.0);
    assert_eq!(s.result(id).await.1, Some(WriteRefusal::WritesDisarmed));
    assert_eq!(s.writes().len(), 1, "nothing replayed after reconnection");
    // An explicit arming restores writes.
    s.arm().await;
    let id = s.write(7.0);
    assert!(s.result(id).await.0);
}

#[tokio::test]
async fn connection_loss_during_a_pulse_releases_it_and_disarms() {
    let mut s = Session::armed(5000).await;
    let id = s.pulse();
    assert!(s.result(id).await.0);
    s.script(&[Step::Refused]); // the source is going away: the FALSE cannot be written
    s.source_state(SourceState::Connecting);
    let (released, message) = s.released().await;
    assert!(!released);
    assert!(starts(&message, NOT_TRANSMITTED), "{message:?}");
    s.expect("writesArmed false", |m| {
        matches!(m, Message::WritesArmed { armed: false, .. })
    })
    .await;
    assert_eq!(
        s.writes(),
        vec![t(true), t(false)],
        "one release attempt, no retry"
    );
}

// ---- failed or unconfirmed release (F-2) ---------------------------------------------------

#[tokio::test]
async fn an_unconfirmed_release_blocks_the_command_until_writes_are_armed_again() {
    let mut s = Session::armed(100).await;
    s.script(&[Step::Ok, Step::Unknown]); // TRUE fine, FALSE uncertain
    let id = s.pulse();
    assert!(s.result(id).await.0);
    let (released, message) = s.released().await;
    assert!(!released);
    assert!(starts(&message, OUTCOME_UNKNOWN), "{message:?}");
    // The engine never writes FALSE again by itself, and refuses a new pulse.
    tokio::time::sleep(Duration::from_millis(600)).await;
    let id = s.pulse();
    let (accepted, refusal, message) = s.result(id).await;
    assert!(!accepted);
    assert_eq!(refusal, Some(WriteRefusal::Busy));
    assert!(message.unwrap_or_default().contains("not confirmed"));
    assert_eq!(s.writes(), vec![t(true), t(false)]);
    // Arming again is the explicit acknowledgement; the command works normally afterwards.
    s.arm().await;
    let id = s.pulse();
    assert!(s.result(id).await.0);
    assert_eq!(s.released().await, (true, None));
    assert_eq!(s.writes(), vec![t(true), t(false), t(true), t(false)]);
}

#[tokio::test]
async fn a_release_without_an_answer_is_bounded_and_reported() {
    let mut s = Session::armed(100).await;
    s.script(&[Step::Ok, Step::Hang]);
    let id = s.pulse();
    assert!(s.result(id).await.0);
    let started = Instant::now();
    let (released, message) = s.released().await;
    assert!(!released);
    assert!(starts(&message, OUTCOME_UNKNOWN), "{message:?}");
    assert!(started.elapsed() < LIMITS.release_timeout * 4);
    assert_eq!(s.shared.lock().unwrap().withdrawn, 1);
}

// ---- shutdown and termination --------------------------------------------------------------

#[tokio::test]
async fn shutdown_with_a_running_pulse_releases_it_before_stopping() {
    let mut s = Session::armed(5000).await;
    let id = s.pulse();
    assert!(s.result(id).await.0);
    let id = s.send(|id| Request::Shutdown { id });
    assert_eq!(s.released().await, (true, None));
    s.ok(id).await;
    s.ended_within(Duration::from_secs(2)).await;
    assert_eq!(s.writes(), vec![t(true), t(false)]);
    assert_eq!(s.shared.lock().unwrap().stops, 1);
}

#[tokio::test]
async fn shutdown_is_bounded_when_the_release_and_the_driver_hang() {
    let mut s = Session::armed(5000).await;
    let id = s.pulse();
    assert!(s.result(id).await.0);
    s.script(&[Step::Hang]); // the release never answers
    s.shared.lock().unwrap().stop_hangs = true; // nor does stop
    let started = Instant::now();
    let id = s.send(|id| Request::Shutdown { id });
    let (released, message) = s.released().await;
    assert!(!released);
    assert!(starts(&message, OUTCOME_UNKNOWN));
    s.ok(id).await;
    s.ended_within(Duration::from_secs(2)).await;
    let took = started.elapsed();
    // release bound + stop bound, plus scheduling margin.
    assert!(
        took < LIMITS.release_timeout + LIMITS.stop_timeout + Duration::from_millis(700),
        "{took:?}"
    );
    assert_eq!(s.shared.lock().unwrap().stops, 1);
}

#[tokio::test]
async fn a_client_disconnect_terminates_within_bounds_with_a_hung_driver() {
    let mut s = Session::armed(5000).await;
    let id = s.pulse();
    assert!(s.result(id).await.0);
    s.script(&[Step::Hang]);
    s.shared.lock().unwrap().stop_hangs = true;
    drop(s.tx.take());
    s.ended_within(Duration::from_secs(2)).await;
    let shared = s.shared.lock().unwrap();
    assert_eq!(
        shared.writes,
        vec![t(true), t(false)],
        "release attempted once"
    );
    assert_eq!(shared.stops, 1);
    assert_eq!(
        shared.withdrawn, 1,
        "the hung release was dropped, not leaked"
    );
}

#[tokio::test]
async fn shutdown_during_a_pending_write_is_immediate_and_nothing_follows() {
    let mut s = Session::armed(200).await;
    s.script(&[Step::Hang]);
    let first = s.write(1.0);
    tokio::time::sleep(Duration::from_millis(50)).await; // the write is in progress
    s.write(2.0); // deferred behind it; must never run
    let started = Instant::now();
    let shutdown = s.send(|id| Request::Shutdown { id });
    let (accepted, _, message) = s.result(first).await;
    assert!(!accepted);
    assert!(starts(&message, OUTCOME_UNKNOWN));
    s.ok(shutdown).await;
    s.ended_within(Duration::from_secs(2)).await;
    assert!(
        started.elapsed() < LIMITS.write_timeout,
        "did not wait for the write timeout"
    );
    assert_eq!(s.writes().len(), 1);
}
