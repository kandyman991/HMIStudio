//! REUSE-02C.4A: write lifecycle and termination safety. A scripted simulated driver only (no
//! device, no network). Engine time bounds are shortened through `run_with_limits`; every
//! blocking point is either scripted or released by the test.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use hmi_engine::driver::{
    Driver, DriverError, DriverEvent, DriverFactory, EventSink, NOT_TRANSMITTED, OUTCOME_UNKNOWN,
};
use hmi_engine::engine::{self, Limits};
use hmi_engine::protocol::{
    Access, BrowseNode, Message, PointValue, Quality, Request, RuntimeConfig, RuntimePoint,
    RuntimeSource, SourceState, Value, ValueType, WriteRefusal, PROTOCOL_VERSION,
};
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::task::JoinHandle;

const LIMITS: Limits = Limits {
    write_timeout: Duration::from_millis(300),
    lifecycle_timeout: Duration::from_millis(300),
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
    starts: usize,
    demand_calls: usize,
    hang_start: bool,
    fail_start: bool,
    hang_demand: bool,
    steps: VecDeque<Step>,
    /// `stop` never completes.
    stop_hangs: bool,
    /// Event sink of every started source, by source id.
    sinks: BTreeMap<String, EventSink>,
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
        let (hang, fail) = {
            let mut shared = self.shared.lock().unwrap();
            shared.starts += 1;
            shared.sinks.insert(self.source.clone(), self.sink.clone());
            (
                shared.hang_start && self.source == "sim",
                shared.fail_start && self.source == "sim",
            )
        };
        if hang {
            std::future::pending::<()>().await;
        }
        if fail {
            return Err(DriverError("scripted start failure".into()));
        }
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
    async fn set_demand(&mut self, _: Option<&BTreeSet<String>>) {
        let hangs = {
            let mut shared = self.shared.lock().unwrap();
            shared.demand_calls += 1;
            shared.hang_demand
        };
        if hangs {
            std::future::pending::<()>().await;
        }
    }
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
    on("sim", variable, value_type, pulse_ms)
}

fn on(source: &str, variable: &str, value_type: ValueType, pulse_ms: Option<u32>) -> RuntimePoint {
    RuntimePoint {
        mapping_id: format!("m-{variable}"),
        variable_id: variable.into(),
        source_id: source.into(),
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
/// A second momentary command on the same source, never affected by RESET's state.
const START: &str = "line.start";
/// A readable point on a second, healthy source.
const LEVEL: &str = "aux.level";

struct Session {
    tx: Option<Sender<Request>>,
    rx: Receiver<Message>,
    engine: JoinHandle<()>,
    next: u64,
    shared: Arc<Mutex<Shared>>,
    config: Option<RuntimeConfig>,
}

impl Session {
    /// Hello, configure, start, connected and armed by "op".
    async fn armed(pulse_ms: u32) -> Self {
        let mut s = Self::configured(pulse_ms).await;
        s.start().await;
        s.arm().await;
        s
    }

    async fn configured(pulse_ms: u32) -> Self {
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
            config: None,
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
            sources: ["sim", "aux"]
                .into_iter()
                .map(|id| RuntimeSource {
                    id: id.into(),
                    driver: "loopback".into(), // a valid name; the factory supplies the simulation
                    settings: Default::default(),
                })
                .collect(),
            points: vec![
                point(SETPOINT, ValueType::Double, None),
                point(RESET, ValueType::Bool, Some(pulse_ms)),
                point(START, ValueType::Bool, Some(pulse_ms)),
                RuntimePoint {
                    access: Access::Read,
                    write_confirmed: false,
                    ..on("aux", LEVEL, ValueType::Double, None)
                },
            ],
        };
        let id = s.send(|id| Request::Configure {
            id,
            config: config.clone(),
            credentials: Default::default(),
        });
        s.ok(id).await;
        s.config = Some(config);
        s
    }

    /// `start`, then both sources connected.
    async fn start(&mut self) {
        let id = self.send(|id| Request::Start { id });
        self.ok(id).await;
        self.connected().await;
        self.connected().await;
    }

    async fn reconfigure(&mut self) {
        let config = self.config.clone().expect("configured");
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

    /// Emits an event from a simulated driver, as a real driver would.
    fn emit(&self, source: &str, event: DriverEvent) {
        let sink = self.shared.lock().unwrap().sinks[source].clone();
        sink.send((source.into(), event)).map_err(|_| ()).unwrap();
    }

    /// State change of the main source "sim".
    fn source_state(&self, state: SourceState) {
        self.emit("sim", DriverEvent::State(state, None));
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
        self.pulse_of(RESET)
    }

    fn pulse_of(&mut self, variable: &str) -> u64 {
        let variable = variable.to_string();
        self.send(|id| Request::Pulse {
            id,
            variable_id: variable,
            actor: "op".into(),
        })
    }

    /// Asserts that a RESET pulse is refused as blocked, without reaching the driver.
    async fn reset_is_blocked(&mut self) {
        let before = self.writes().len();
        let id = self.pulse();
        let (accepted, refusal, message) = self.result(id).await;
        assert!(!accepted);
        assert_eq!(refusal, Some(WriteRefusal::Busy));
        assert!(starts(&message, "COMMAND_UNCONFIRMED:"), "{message:?}");
        assert_eq!(self.writes().len(), before, "nothing reached the driver");
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

#[tokio::test]
async fn a_healthy_source_keeps_being_monitored_after_another_source_is_lost() {
    let mut s = Session::armed(200).await;
    s.source_state(SourceState::Connecting);
    s.expect("writesArmed false", |m| {
        matches!(m, Message::WritesArmed { armed: false, .. })
    })
    .await;
    // Disarming is global under protocol 3, but monitoring of the healthy source continues.
    s.emit(
        "aux",
        DriverEvent::Values(vec![PointValue {
            variable_id: LEVEL.into(),
            value: Some(Value::Double(41.5)),
            quality: Quality::Good,
            status: None,
            source_timestamp_ms: None,
        }]),
    );
    let values = s
        .expect("aux values", |m| matches!(m, Message::Values { .. }))
        .await;
    let Message::Values { values } = values else {
        unreachable!()
    };
    assert_eq!(values.len(), 1);
    assert_eq!(values[0].variable_id, LEVEL);
    assert_eq!(values[0].value, Some(Value::Double(41.5)));
    assert_eq!(values[0].quality, Quality::Good);
    assert_eq!(s.shared.lock().unwrap().stops, 0, "no source was stopped");
}

// ---- failed or unconfirmed release (F-2) ---------------------------------------------------

/// Leaves RESET with an unconfirmed release: TRUE written, FALSE outcome unknown.
async fn unconfirmed_reset() -> Session {
    let mut s = Session::armed(100).await;
    s.script(&[Step::Ok, Step::Unknown]);
    let id = s.pulse();
    assert!(s.result(id).await.0);
    let (released, message) = s.released().await;
    assert!(!released);
    assert!(starts(&message, OUTCOME_UNKNOWN), "{message:?}");
    s
}

#[tokio::test]
async fn an_unconfirmed_release_blocks_the_command_and_is_never_rewritten() {
    let mut s = unconfirmed_reset().await;
    tokio::time::sleep(Duration::from_millis(600)).await;
    s.reset_is_blocked().await;
    assert_eq!(
        s.writes(),
        vec![t(true), t(false)],
        "FALSE never written again by itself"
    );
}

#[tokio::test]
async fn re_arming_does_not_clear_an_unconfirmed_command() {
    // D-02C4A-2 as decided: communication authorization and command recovery are separate.
    let mut s = unconfirmed_reset().await;
    s.arm().await;
    s.reset_is_blocked().await;
    let id = s.send(|id| Request::DisarmWrites { id });
    s.ok(id).await;
    s.arm().await;
    s.reset_is_blocked().await;
    assert_eq!(s.writes(), vec![t(true), t(false)]);
}

#[tokio::test]
async fn stop_start_configure_and_reconnection_do_not_clear_an_unconfirmed_command() {
    let mut s = unconfirmed_reset().await;
    let id = s.send(|id| Request::Stop { id });
    s.ok(id).await;
    s.start().await;
    s.arm().await;
    s.reset_is_blocked().await;
    s.reconfigure().await;
    s.start().await;
    s.arm().await;
    s.reset_is_blocked().await;
    s.source_state(SourceState::Connecting);
    s.source_state(SourceState::Connected);
    s.connected().await;
    s.arm().await;
    s.reset_is_blocked().await;
    assert_eq!(s.writes(), vec![t(true), t(false)]);
}

#[tokio::test]
async fn only_the_affected_command_is_blocked() {
    let mut s = unconfirmed_reset().await;
    let id = s.pulse_of(START);
    assert!(
        s.result(id).await.0,
        "another momentary command still works"
    );
    assert_eq!(s.released().await, (true, None));
    let id = s.write(3.0);
    assert!(s.result(id).await.0, "ordinary writes still work");
    s.reset_is_blocked().await;
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
    assert_eq!(s.shared.lock().unwrap().stops, 2, "both sources stopped");
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
    // One release bound plus one stop bound per source (two hung drivers), plus margin.
    assert!(
        took < LIMITS.release_timeout + LIMITS.stop_timeout * 2 + Duration::from_millis(700),
        "{took:?}"
    );
    assert_eq!(s.shared.lock().unwrap().stops, 2, "both sources stopped");
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
    assert_eq!(shared.stops, 2, "both sources stopped");
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

// Observe a scripted operation entering the driver, without relying on a fixed sleep.
async fn wait_shared(s: &Session, ready: impl Fn(&Shared) -> bool) {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if ready(&s.shared.lock().unwrap()) {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("driver operation did not start");
}

#[tokio::test]
async fn queued_arm_write_and_pulse_are_invalidated_by_loss_even_after_reconnect() {
    let mut s = Session::armed(5000).await;
    s.script(&[Step::Hang]);
    let active = s.write(1.0);
    wait_shared(&s, |x| x.writes.len() == 1).await;
    let arm = s.send(|id| Request::ArmWrites {
        id,
        actor: "op".into(),
        ttl_seconds: 60,
    });
    let queued = s.write(2.0);
    let pulse = s.pulse();
    let backlog: Vec<_> = (0..engine::DEFERRED_MAX + 4)
        .map(|_| s.write(2.0))
        .collect();
    s.source_state(SourceState::Connecting);
    s.source_state(SourceState::Connected);
    assert!(starts(&s.result(active).await.2, OUTCOME_UNKNOWN));
    s.expect("disarmed", |m| {
        matches!(m, Message::WritesArmed { armed: false, .. })
    })
    .await;
    s.expect(
        "stale arm refused",
        |m| matches!(m, Message::Error { id, code, .. } if *id == arm && code == "WRITES_DISARMED"),
    )
    .await;
    assert_eq!(s.result(queued).await.1, Some(WriteRefusal::WritesDisarmed));
    assert_eq!(s.result(pulse).await.1, Some(WriteRefusal::WritesDisarmed));
    for id in backlog {
        assert_eq!(s.result(id).await.1, Some(WriteRefusal::WritesDisarmed));
    }
    assert_eq!(s.writes().len(), 1);
    s.arm().await;
    let fresh = s.write(3.0);
    assert!(s.result(fresh).await.0);
}

#[tokio::test]
async fn ambiguous_pulse_start_is_released_once_and_failed_release_blocks() {
    let mut s = Session::armed(5000).await;
    s.script(&[Step::Unknown, Step::Unknown]);
    let id = s.pulse();
    assert!(starts(&s.result(id).await.2, OUTCOME_UNKNOWN));
    let (released, reason) = s.released().await;
    assert!(!released);
    assert!(starts(&reason, OUTCOME_UNKNOWN));
    s.arm().await;
    s.reset_is_blocked().await;
    assert_eq!(s.writes(), vec![t(true), t(false)]);
}

#[tokio::test]
async fn hung_start_and_failure_cleanup_are_bounded_and_healthy_source_starts() {
    for hang in [true, false] {
        let mut s = Session::configured(100).await;
        {
            let mut state = s.shared.lock().unwrap();
            state.hang_start = hang;
            state.fail_start = !hang;
            state.stop_hangs = true;
        }
        let start = Instant::now();
        let id = s.send(|id| Request::Start { id });
        s.ok(id).await;
        s.expect("sim failed", |m| matches!(m, Message::SourceState { source_id, state: SourceState::Failed, .. } if source_id == "sim")).await;
        s.expect("aux connected", |m| matches!(m, Message::SourceState { source_id, state: SourceState::Connected, .. } if source_id == "aux")).await;
        assert!(
            start.elapsed()
                < LIMITS.lifecycle_timeout + LIMITS.stop_timeout + Duration::from_millis(900)
        );
        assert_eq!(s.shared.lock().unwrap().stops, 1);
        let shutdown = s.send(|id| Request::Shutdown { id });
        s.ok(shutdown).await;
        s.ended_within(Duration::from_secs(2)).await;
    }
}

#[tokio::test]
async fn shutdown_during_hung_start_cancels_it_without_starting_another_source() {
    let mut s = Session::configured(100).await;
    s.shared.lock().unwrap().hang_start = true;
    let id = s.send(|id| Request::Start { id });
    s.ok(id).await;
    wait_shared(&s, |x| x.starts == 1).await;
    let shutdown = s.send(|id| Request::Shutdown { id });
    s.ok(shutdown).await;
    s.ended_within(Duration::from_secs(2)).await;
    let state = s.shared.lock().unwrap();
    assert_eq!(state.starts, 1);
    assert_eq!(state.stops, 1);
}

#[tokio::test]
async fn hung_demand_is_bounded_and_shutdown_cancels_pending_demand() {
    let mut s = Session::armed(100).await;
    s.shared.lock().unwrap().hang_demand = true;
    let started = Instant::now();
    let id = s.send(|id| Request::SetDemand {
        id,
        variable_ids: Some(vec![LEVEL.into()]),
    });
    s.ok(id).await;
    // Both drivers time out; a following control request can complete.
    let write = s.write(7.0);
    assert!(s.result(write).await.0);
    assert!(started.elapsed() < LIMITS.lifecycle_timeout * 2 + Duration::from_millis(900));
    assert_eq!(s.shared.lock().unwrap().demand_calls, 2);
    let id = s.send(|id| Request::SetDemand {
        id,
        variable_ids: None,
    });
    s.ok(id).await;
    wait_shared(&s, |x| x.demand_calls == 3).await;
    let shutdown = s.send(|id| Request::Shutdown { id });
    s.ok(shutdown).await;
    s.ended_within(Duration::from_secs(2)).await;
    assert_eq!(s.shared.lock().unwrap().demand_calls, 3);
}
