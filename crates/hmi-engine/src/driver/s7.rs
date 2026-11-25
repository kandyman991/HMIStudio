//! Siemens S7 driver (classic S7 protocol over ISO-on-TCP, port 102) for S7-1200 and S7-1500.
//!
//! PLC requirements (TIA Portal): *Protection & Security → Connection mechanisms → Permit
//! access with PUT/GET communication* enabled, and *Optimized block access* **off** for every
//! data block the HMI addresses. Optimized blocks have no fixed offsets; use the OPC UA driver
//! for those.
//!
//! Source settings: `host` (required), `port` (102), `rack` (0), `slot` (0; S7-1200/1500 also
//! answer on slot 1), `pollIntervalMs` (200, 20–60000), `timeoutMs` (2000).
//! Addresses: see [`super::s7_address`].
//!
//! S7 has no subscriptions: readable points are polled, grouped into few contiguous reads, and
//! a value is reported when it changes. The client (`rust7`) is synchronous, so each source runs
//! on its own thread. After **any** communication error the connection is dropped and rebuilt,
//! so a late reply can never be taken for the answer to a later request.
//!
//! Command queue (REUSE-02C.1): the queue to the source thread is bounded ([COMMAND_QUEUE]);
//! a write that does not fit is refused as not transmitted. Stopping does not use the queue: a
//! stop flag is raised and the sender dropped, so a saturated queue cannot delay shutdown. A
//! queued write is transmitted only if, at that moment, the driver is not stopping, its
//! requester is still waiting, and it was queued on the current connection. Anything else is
//! refused as not transmitted, so a write is never replayed after a stop, a cancellation or a
//! reconnection. A write whose transmission failed is reported as outcome unknown.

use std::collections::{BTreeSet, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use rust7::client::{S7Client, S7Error, CT_OP, S7_WL_BIT, S7_WL_BYTE};
use tokio::sync::oneshot;

use super::s7_address::{plan_reads, S7Address, S7Type};
use super::{setting, Driver, DriverError, DriverEvent, EventSink};
use crate::protocol::{
    BrowseNode, PointValue, Quality, RuntimePoint, RuntimeSource, SourceState, Value,
};

const MAX_READ_GAP: u32 = 16;
const MAX_READ_SPAN: u32 = 400;
const RETRY_INITIAL: Duration = Duration::from_secs(1);
const RETRY_MAX: Duration = Duration::from_secs(15);
/// Capacity of the command queue to a source thread. The engine awaits each write, so in normal
/// operation the queue holds at most one write, one release and one coalesced wake.
pub const COMMAND_QUEUE: usize = 8;

/// The S7 operations the source thread uses; implemented by `rust7`'s client and, in tests, by
/// a simulated PLC.
pub(crate) trait Plc: Send {
    fn read_area(
        &mut self,
        area: u8,
        db: u16,
        start: u16,
        wordlen: u8,
        buffer: &mut [u8],
    ) -> Result<(), S7Error>;
    fn write_area(
        &mut self,
        area: u8,
        db: u16,
        start: u16,
        wordlen: u8,
        data: &[u8],
    ) -> Result<(), S7Error>;
    fn disconnect(&mut self);
}

impl Plc for S7Client {
    fn read_area(
        &mut self,
        area: u8,
        db: u16,
        start: u16,
        wordlen: u8,
        buffer: &mut [u8],
    ) -> Result<(), S7Error> {
        S7Client::read_area(self, area, db, start, wordlen, buffer)
    }
    fn write_area(
        &mut self,
        area: u8,
        db: u16,
        start: u16,
        wordlen: u8,
        data: &[u8],
    ) -> Result<(), S7Error> {
        S7Client::write_area(self, area, db, start, wordlen, data)
    }
    fn disconnect(&mut self) {
        S7Client::disconnect(self)
    }
}

type Connector = Arc<dyn Fn(&Settings) -> Result<Box<dyn Plc>, S7Error> + Send + Sync>;

fn real_connector() -> Connector {
    Arc::new(|settings: &Settings| connect(settings).map(|c| Box::new(c) as Box<dyn Plc>))
}

/// Validates every point of an S7 source without connecting.
pub fn validate(source: &RuntimeSource, points: &[RuntimePoint]) -> Result<(), String> {
    Settings::parse(source)?;
    for point in points {
        let address = S7Address::parse(&point.address)
            .map_err(|e| format!("mapping '{}': {e}", point.mapping_id))?;
        if !address.kind.readable_as(point.value_type) {
            return Err(format!(
                "mapping '{}': {:?} at '{}' cannot hold a {:?} variable without wrapping or truncating",
                point.mapping_id, address.kind, point.address, point.value_type
            ));
        }
    }
    Ok(())
}

#[derive(Clone)]
struct Settings {
    host: String,
    port: u16,
    rack: u16,
    slot: u16,
    poll: Duration,
    timeout: Duration,
}

impl Settings {
    fn parse(source: &RuntimeSource) -> Result<Self, String> {
        let number = |key: &str, default: u64, min: u64, max: u64| -> Result<u64, String> {
            match setting(&source.settings, key) {
                None => Ok(default),
                Some(text) => text
                    .parse::<u64>()
                    .ok()
                    .filter(|v| (min..=max).contains(v))
                    .ok_or_else(|| format!("setting '{key}' must be a number from {min} to {max}")),
            }
        };
        let host = setting(&source.settings, "host")
            .ok_or("the S7 source has no 'host' setting (PLC IP address or name)")?
            .to_owned();
        Ok(Self {
            host,
            port: number("port", 102, 1, 65535)? as u16,
            rack: number("rack", 0, 0, 7)? as u16,
            slot: number("slot", 0, 0, 31)? as u16,
            poll: Duration::from_millis(number("pollIntervalMs", 200, 20, 60_000)?),
            timeout: Duration::from_millis(number("timeoutMs", 2000, 100, 30_000)?),
        })
    }
}

enum Command {
    Write {
        address: S7Address,
        value: Value,
        reply: oneshot::Sender<Result<(), DriverError>>,
        queued_at: Instant,
    },
    /// The demand changed: re-plan now instead of at the next poll. Coalesced: at most one is
    /// queued at a time.
    Wake,
}

/// Flags shared by a driver and the thread of one start.
struct Control {
    stopping: AtomicBool,
    wake_pending: AtomicBool,
}

/// Which variables are in use (`None` = all), with a counter that changes on every update.
type Demand = Arc<Mutex<(u64, Option<BTreeSet<String>>)>>;

#[derive(Clone)]
struct Reader {
    point: RuntimePoint,
    address: S7Address,
}

pub struct S7Driver {
    source: RuntimeSource,
    points: Vec<RuntimePoint>,
    sink: EventSink,
    commands: Option<SyncSender<Command>>,
    control: Option<Arc<Control>>,
    thread: Option<JoinHandle<()>>,
    demand: Demand,
    connector: Connector,
}

impl S7Driver {
    pub fn new(
        source: RuntimeSource,
        points: Vec<RuntimePoint>,
        sink: EventSink,
    ) -> Result<Self, DriverError> {
        Self::with_connector(source, points, sink, real_connector())
    }

    fn with_connector(
        source: RuntimeSource,
        points: Vec<RuntimePoint>,
        sink: EventSink,
        connector: Connector,
    ) -> Result<Self, DriverError> {
        validate(&source, &points).map_err(DriverError)?;
        Ok(Self {
            source,
            points,
            sink,
            commands: None,
            control: None,
            thread: None,
            demand: Arc::new(Mutex::new((0, None))),
            connector,
        })
    }
}

#[async_trait]
impl Driver for S7Driver {
    async fn start(&mut self) -> Result<(), DriverError> {
        let settings = Settings::parse(&self.source).map_err(DriverError)?;
        let readers: Vec<Reader> = self
            .points
            .iter()
            .filter(|p| p.readable())
            .map(|p| Reader {
                point: p.clone(),
                address: S7Address::parse(&p.address).expect("validated"),
            })
            .collect();
        if self.thread.is_some() {
            return Err(DriverError("the S7 source is already started".into()));
        }
        let (tx, rx) = mpsc::sync_channel(COMMAND_QUEUE);
        let control = Arc::new(Control {
            stopping: AtomicBool::new(false),
            wake_pending: AtomicBool::new(false),
        });
        let link = Link {
            source_id: self.source.id.clone(),
            sink: self.sink.clone(),
        };
        let thread = std::thread::Builder::new()
            .name(format!("s7-{}", self.source.id))
            .spawn({
                let demand = self.demand.clone();
                let control = control.clone();
                let connector = self.connector.clone();
                move || run(settings, readers, rx, link, demand, control, connector)
            })
            .map_err(|e| DriverError(format!("cannot start the S7 thread: {e}")))?;
        self.commands = Some(tx);
        self.control = Some(control);
        self.thread = Some(thread);
        Ok(())
    }

    async fn write(&mut self, point: &RuntimePoint, value: &Value) -> Result<(), DriverError> {
        let address = S7Address::parse(&point.address).map_err(DriverError)?;
        if !matches!(address.kind, S7Type::Bool { .. }) {
            address.encode(value).map_err(DriverError)?; // refuse before queueing
        }
        let commands = self
            .commands
            .as_ref()
            .ok_or_else(|| DriverError::not_transmitted("the S7 source is not started"))?;
        let (reply, answer) = oneshot::channel();
        commands
            .try_send(Command::Write {
                address,
                value: value.clone(),
                reply,
                queued_at: Instant::now(),
            })
            .map_err(|error| match error {
                TrySendError::Full(_) => DriverError::not_transmitted(
                    "the S7 command queue is full; nothing was written",
                ),
                TrySendError::Disconnected(_) => DriverError::not_transmitted(
                    "the S7 connection has stopped; nothing was written",
                ),
            })?;
        // Dropping this future before the answer (the engine stopped waiting) withdraws the write
        // if it has not been transmitted yet; see `transmit_allowed`.
        answer.await.map_err(|_| {
            DriverError::outcome_unknown(
                "the S7 connection stopped during the write; its outcome is unknown",
            )
        })?
    }

    async fn set_demand(&mut self, demand: Option<&BTreeSet<String>>) {
        {
            let mut current = self.demand.lock().unwrap();
            current.0 += 1;
            current.1 = demand.cloned();
        }
        // The demand itself lives in shared state; the queue only carries one coalesced wake, so
        // demand changes can never fill the queue and crowd out writes.
        if let (Some(commands), Some(control)) = (&self.commands, &self.control) {
            if !control.wake_pending.swap(true, Ordering::AcqRel)
                && commands.try_send(Command::Wake).is_err()
            {
                control.wake_pending.store(false, Ordering::Release);
            }
        }
    }

    async fn browse(&mut self, _node_id: Option<&str>) -> Result<Vec<BrowseNode>, DriverError> {
        Err(DriverError(
            "S7 has no address browsing; use absolute addresses from TIA Portal (e.g. DB10.REAL4), or the OPC UA driver for symbolic browsing".into(),
        ))
    }

    async fn stop(&mut self) {
        // Never through the queue: raise the flag, then drop the only sender so the thread's
        // next receive ends. Queued writes still in the queue are refused, never transmitted.
        if let Some(control) = self.control.take() {
            control.stopping.store(true, Ordering::Release);
        }
        drop(self.commands.take());
        if let Some(thread) = self.thread.take() {
            // The thread finishes at most the one S7 call in progress (bounded by `timeoutMs`).
            // If even that overruns, stop returns anyway: the raised flag keeps the detached
            // thread from transmitting anything further.
            let bound = Settings::parse(&self.source)
                .map(|s| s.timeout * 3 + Duration::from_secs(1))
                .unwrap_or(Duration::from_secs(5));
            let _ = tokio::time::timeout(bound, tokio::task::spawn_blocking(move || thread.join()))
                .await;
        }
        let _ = self.sink.send((
            self.source.id.clone(),
            DriverEvent::State(SourceState::Disconnected, None),
        ));
    }
}

struct Link {
    source_id: String,
    sink: EventSink,
}

impl Link {
    fn state(&self, state: SourceState, message: Option<String>) {
        let _ = self
            .sink
            .send((self.source_id.clone(), DriverEvent::State(state, message)));
    }

    fn values(&self, values: Vec<PointValue>) {
        if !values.is_empty() {
            let _ = self
                .sink
                .send((self.source_id.clone(), DriverEvent::Values(values)));
        }
    }
}

fn bad(reader: &Reader, status: &str) -> PointValue {
    PointValue {
        variable_id: reader.point.variable_id.clone(),
        value: None,
        quality: Quality::Bad,
        status: Some(status.into()),
        source_timestamp_ms: None,
    }
}

fn connect(settings: &Settings) -> Result<S7Client, S7Error> {
    let mut client = S7Client::new();
    let ms = settings.timeout.as_millis() as u64;
    client.set_timeout(ms, ms, ms)?;
    client.set_connection_port(settings.port)?;
    client.set_connection_type(CT_OP)?;
    client.connect_rack_slot(&settings.host, settings.rack, settings.slot)?;
    Ok(client)
}

/// Errors that come from the PLC's own answer: the request was understood and refused, so
/// nothing was written and the connection is still in step.
fn is_plc_refusal(error: &S7Error) -> bool {
    matches!(
        error,
        S7Error::S7NotFound | S7Error::S7InvalidAddress | S7Error::S7Unspecified
    )
}

fn describe(error: &S7Error) -> String {
    match error {
        S7Error::S7InvalidAddress => {
            "the PLC refused the address (outside the block, or the data block has optimized access on)".into()
        }
        S7Error::S7NotFound => "the PLC reports the data block does not exist".into(),
        S7Error::S7Unspecified => "the PLC refused the request (check PUT/GET access is permitted)".into(),
        other => other.to_string(),
    }
}

/// Stable status code of a point the PLC refused to read (see docs/RUNTIME_ENGINE.md §3b).
fn status_code(error: &S7Error) -> &'static str {
    match error {
        S7Error::S7NotFound => "AddressNotFound",
        S7Error::S7InvalidAddress => "AddressRefused",
        _ => "AccessDenied",
    }
}

/// Waits `delay` while still answering commands (writes are refused: nothing is connected).
/// Returns false when the driver must stop.
fn back_off(commands: &mpsc::Receiver<Command>, delay: Duration, control: &Control) -> bool {
    let until = Instant::now() + delay;
    loop {
        if control.stopping.load(Ordering::Acquire) {
            refuse_queued(commands, "the S7 source is stopping; nothing was written");
            return false;
        }
        match commands.recv_timeout(until.saturating_duration_since(Instant::now())) {
            Err(RecvTimeoutError::Disconnected) => return false,
            Ok(Command::Wake) => control.wake_pending.store(false, Ordering::Release),
            Ok(Command::Write { reply, .. }) => {
                let _ = reply.send(Err(DriverError::not_transmitted(
                    "not connected; nothing was written",
                )));
            }
            Err(RecvTimeoutError::Timeout) => return true,
        }
    }
}

/// Answers every write still queued without transmitting it.
fn refuse_queued(commands: &mpsc::Receiver<Command>, why: &str) {
    while let Ok(command) = commands.try_recv() {
        if let Command::Write { reply, .. } = command {
            let _ = reply.send(Err(DriverError::not_transmitted(why)));
        }
    }
}

/// Why a dequeued write must not be transmitted now, if it must not. Checked immediately
/// before transmission: the last point at which a write can still be withdrawn.
fn transmit_refusal(
    control: &Control,
    reply: &oneshot::Sender<Result<(), DriverError>>,
    queued_at: Instant,
    connected_at: Instant,
) -> Option<&'static str> {
    if control.stopping.load(Ordering::Acquire) {
        Some("the S7 source is stopping; nothing was written")
    } else if reply.is_closed() {
        // Nobody is waiting for the answer any more (the engine abandoned the request).
        Some("the request was withdrawn before transmission; nothing was written")
    } else if queued_at < connected_at {
        // Queued against an earlier connection: never replayed on a new one.
        Some("queued before the current connection was established; it is not replayed")
    } else {
        None
    }
}

fn run(
    settings: Settings,
    all_readers: Vec<Reader>,
    commands: mpsc::Receiver<Command>,
    link: Link,
    demand: Demand,
    control: Arc<Control>,
    connector: Connector,
) {
    // Only the points in use are read; the plan follows the demand.
    let mut seen_demand = u64::MAX;
    let mut readers: Vec<Reader> = Vec::new();
    let mut spans = Vec::new();
    let mut last: HashMap<String, PointValue> = HashMap::new();
    let mut delay = RETRY_INITIAL;
    let mut client: Option<Box<dyn Plc>> = None;
    let mut next_poll = Instant::now();
    // When the current connection was established; writes queued earlier are stale.
    let mut connected_at = Instant::now();

    let lost = |client: &mut Option<Box<dyn Plc>>,
                readers: &[Reader],
                last: &mut HashMap<String, PointValue>,
                why: &str,
                status: &str| {
        if let Some(mut c) = client.take() {
            c.disconnect();
        }
        link.state(SourceState::Connecting, Some(why.to_owned()));
        // Values are unknown now: never leave the last good value on screen.
        let changed: Vec<PointValue> = readers
            .iter()
            .map(|r| bad(r, status))
            .filter(|v| last.get(&v.variable_id) != Some(v))
            .collect();
        for v in &changed {
            last.insert(v.variable_id.clone(), v.clone());
        }
        link.values(changed);
    };

    loop {
        if control.stopping.load(Ordering::Acquire) {
            if let Some(mut c) = client.take() {
                c.disconnect();
            }
            refuse_queued(&commands, "the S7 source is stopping; nothing was written");
            return;
        }
        // ---- demand ----
        {
            let current = demand.lock().unwrap();
            if current.0 != seen_demand {
                seen_demand = current.0;
                readers = all_readers
                    .iter()
                    .filter(|r| {
                        current
                            .1
                            .as_ref()
                            .is_none_or(|wanted| wanted.contains(&r.point.variable_id))
                    })
                    .cloned()
                    .collect();
                spans = plan_reads(
                    &readers.iter().map(|r| r.address).collect::<Vec<_>>(),
                    MAX_READ_GAP,
                    MAX_READ_SPAN,
                );
                // Forget what is no longer read, so it is reported afresh when wanted again.
                last.retain(|variable, _| readers.iter().any(|r| &r.point.variable_id == variable));
                next_poll = Instant::now();
            }
        }

        // ---- connect ----
        if client.is_none() {
            link.state(SourceState::Connecting, None);
            match connector(&settings) {
                Ok(c) => {
                    client = Some(c);
                    connected_at = Instant::now();
                    next_poll = Instant::now();
                    link.state(SourceState::Connected, None);
                }
                Err(error) => {
                    link.state(
                        SourceState::Failed,
                        Some(format!(
                            "cannot connect to {}:{}: {}",
                            settings.host,
                            settings.port,
                            describe(&error)
                        )),
                    );
                    let still_bad: Vec<PointValue> = readers
                        .iter()
                        .map(|r| bad(r, "NotConnected"))
                        .filter(|v| last.get(&v.variable_id) != Some(v))
                        .collect();
                    for v in &still_bad {
                        last.insert(v.variable_id.clone(), v.clone());
                    }
                    link.values(still_bad);
                    if !back_off(&commands, delay, &control) {
                        return;
                    }
                    delay = (delay * 2).min(RETRY_MAX);
                    continue;
                }
            }
        }

        // ---- poll ----
        if Instant::now() >= next_poll {
            next_poll = Instant::now() + settings.poll;
            let c = client.as_mut().expect("connected");
            let mut changed = Vec::new();
            let mut failure: Option<S7Error> = None;
            for span in &spans {
                if control.stopping.load(Ordering::Acquire) {
                    break; // the loop head disconnects and refuses what is queued
                }
                let mut buffer = vec![0u8; (span.end - span.start) as usize];
                match c.read_area(
                    span.area.code(),
                    span.area.db_number(),
                    span.start as u16,
                    S7_WL_BYTE,
                    &mut buffer,
                ) {
                    Ok(()) => {
                        for reader in readers.iter().filter(|r| {
                            r.address.area == span.area
                                && r.address.offset >= span.start
                                && r.address.bytes().1 <= span.end
                        }) {
                            let value = match reader.address.decode(
                                &buffer,
                                span.start,
                                reader.point.value_type,
                            ) {
                                Ok(v) => PointValue {
                                    variable_id: reader.point.variable_id.clone(),
                                    value: Some(v),
                                    quality: Quality::Good,
                                    status: None,
                                    source_timestamp_ms: None,
                                },
                                Err(status) => bad(reader, status),
                            };
                            if last.get(&value.variable_id) != Some(&value) {
                                last.insert(value.variable_id.clone(), value.clone());
                                changed.push(value);
                            }
                        }
                    }
                    Err(error) if is_plc_refusal(&error) => {
                        // This span is refused (e.g. optimized DB); the others still work.
                        for reader in readers.iter().filter(|r| {
                            r.address.area == span.area
                                && r.address.offset >= span.start
                                && r.address.bytes().1 <= span.end
                        }) {
                            let value = bad(reader, status_code(&error));
                            if last.get(&value.variable_id) != Some(&value) {
                                last.insert(value.variable_id.clone(), value.clone());
                                changed.push(value);
                            }
                        }
                    }
                    Err(error) => {
                        failure = Some(error);
                        break;
                    }
                }
            }
            link.values(changed);
            if let Some(error) = failure {
                lost(
                    &mut client,
                    &readers,
                    &mut last,
                    &format!("connection lost: {}", describe(&error)),
                    "ConnectionLost",
                );
                // A PLC that accepts connections but drops them must not be hammered.
                if !back_off(&commands, delay, &control) {
                    return;
                }
                delay = (delay * 2).min(RETRY_MAX);
                continue;
            }
            delay = RETRY_INITIAL; // a full poll succeeded: the link is healthy
        }

        // ---- commands until the next poll ----
        match commands.recv_timeout(next_poll.saturating_duration_since(Instant::now())) {
            Err(RecvTimeoutError::Disconnected) => {
                if let Some(mut c) = client.take() {
                    c.disconnect();
                }
                return;
            }
            Err(RecvTimeoutError::Timeout) => {}
            Ok(Command::Wake) => control.wake_pending.store(false, Ordering::Release),
            Ok(Command::Write {
                address,
                value,
                reply,
                queued_at,
            }) => {
                if let Some(why) = transmit_refusal(&control, &reply, queued_at, connected_at) {
                    let _ = reply.send(Err(DriverError::not_transmitted(why)));
                    continue;
                }
                let c = client.as_mut().expect("connected");
                let result = match (&address.kind, &value) {
                    (S7Type::Bool { bit }, Value::Bool(on)) => {
                        // A single-bit write: the PLC changes only this bit, never its neighbours.
                        let bit_address = (address.offset * 8 + *bit as u32) as u16;
                        c.write_area(
                            address.area.code(),
                            address.area.db_number(),
                            bit_address,
                            S7_WL_BIT,
                            &[*on as u8],
                        )
                    }
                    (S7Type::Bool { .. }, other) => {
                        let _ = reply.send(Err(DriverError(format!(
                            "a {:?} value cannot be written to a BOOL",
                            other.value_type()
                        ))));
                        continue;
                    }
                    _ => match address.encode(&value) {
                        Ok(bytes) => c.write_area(
                            address.area.code(),
                            address.area.db_number(),
                            address.offset as u16,
                            S7_WL_BYTE,
                            &bytes,
                        ),
                        Err(why) => {
                            let _ = reply.send(Err(DriverError(why)));
                            continue;
                        }
                    },
                };
                match result {
                    Ok(()) => {
                        let _ = reply.send(Ok(()));
                        next_poll = Instant::now(); // show the written value at once
                    }
                    Err(error) if is_plc_refusal(&error) => {
                        let _ = reply.send(Err(DriverError(format!(
                            "the PLC rejected the write: {}",
                            describe(&error)
                        ))));
                    }
                    Err(error) => {
                        // The request left this host: the PLC may have applied it.
                        let _ = reply.send(Err(DriverError::outcome_unknown(format!(
                            "communication failed during the write; its outcome is unknown ({}). The connection is being re-established; the write is not repeated",
                            describe(&error)
                        ))));
                        lost(
                            &mut client,
                            &readers,
                            &mut last,
                            &format!("connection lost: {}", describe(&error)),
                            "ConnectionLost",
                        );
                        if !back_off(&commands, delay, &control) {
                            return;
                        }
                        delay = (delay * 2).min(RETRY_MAX);
                    }
                }
            }
        }
    }
}

/// REUSE-02C.1 regression tests against a simulated PLC (no network, no device). Each blocking
/// point is a gate the test opens explicitly, so the interleavings are deterministic.
#[cfg(test)]
mod supervision_tests {
    use super::*;
    use crate::protocol::{Access, ValueType};
    use std::collections::{BTreeMap, VecDeque};
    use std::sync::mpsc::{Receiver as StdReceiver, Sender as StdSender};

    enum Step {
        /// Succeed at once.
        Ok,
        /// Announce entry, then wait for the gate before succeeding.
        Block(StdReceiver<()>),
        /// A transport failure after the request left: outcome unknown.
        Fail,
    }

    #[derive(Default)]
    struct Shared {
        /// Bytes of every write that reached the simulated PLC, in order.
        transmitted: Vec<Vec<u8>>,
        script: VecDeque<Step>,
        connects: usize,
    }

    struct SimPlc {
        shared: Arc<Mutex<Shared>>,
        entered: StdSender<()>,
    }

    impl Plc for SimPlc {
        fn read_area(
            &mut self,
            _: u8,
            _: u16,
            _: u16,
            _: u8,
            buffer: &mut [u8],
        ) -> Result<(), S7Error> {
            buffer.fill(0);
            Ok(())
        }
        fn write_area(&mut self, _: u8, _: u16, _: u16, _: u8, data: &[u8]) -> Result<(), S7Error> {
            let step = self
                .shared
                .lock()
                .unwrap()
                .script
                .pop_front()
                .unwrap_or(Step::Ok);
            self.shared.lock().unwrap().transmitted.push(data.to_vec());
            let _ = self.entered.send(());
            match step {
                Step::Ok => Ok(()),
                Step::Block(gate) => {
                    let _ = gate.recv();
                    Ok(())
                }
                Step::Fail => Err(S7Error::ConnectionClosed),
            }
        }
        fn disconnect(&mut self) {}
    }

    struct Rig {
        driver: S7Driver,
        shared: Arc<Mutex<Shared>>,
        entered: StdReceiver<()>,
        events: tokio::sync::mpsc::Receiver<(String, DriverEvent)>,
        point: RuntimePoint,
    }

    /// `connect_gate`: when given, the first connection waits for it.
    fn rig(connect_gate: Option<StdReceiver<()>>) -> Rig {
        let mut settings = BTreeMap::new();
        settings.insert("host".to_owned(), "sim".to_owned());
        settings.insert("timeoutMs".to_owned(), "100".to_owned());
        settings.insert("pollIntervalMs".to_owned(), "20".to_owned());
        let source = RuntimeSource {
            id: "plc".into(),
            driver: "s7".into(),
            settings,
        };
        let point = RuntimePoint {
            mapping_id: "m".into(),
            variable_id: "v".into(),
            source_id: "plc".into(),
            address: "DB10.INT2".into(),
            value_type: ValueType::Int,
            access: Access::ReadWrite,
            write_confirmed: true,
            limits: None,
            pulse_ms: None,
        };
        let shared = Arc::new(Mutex::new(Shared::default()));
        let (entered_tx, entered) = mpsc::channel();
        let gate = Mutex::new(connect_gate);
        let connector: Connector = {
            let shared = shared.clone();
            Arc::new(move |_: &Settings| {
                if let Some(gate) = gate.lock().unwrap().take() {
                    let _ = gate.recv();
                }
                shared.lock().unwrap().connects += 1;
                Ok(Box::new(SimPlc {
                    shared: shared.clone(),
                    entered: entered_tx.clone(),
                }) as Box<dyn Plc>)
            })
        };
        let (tx, events) = tokio::sync::mpsc::channel(4096);
        let driver =
            S7Driver::with_connector(source, vec![point.clone()], EventSink::new(tx), connector)
                .unwrap();
        Rig {
            driver,
            shared,
            entered,
            events,
            point,
        }
    }

    impl Rig {
        async fn connected(&mut self) {
            loop {
                let event = tokio::time::timeout(Duration::from_secs(5), self.events.recv())
                    .await
                    .expect("no Connected state")
                    .expect("sink closed");
                if matches!(event.1, DriverEvent::State(SourceState::Connected, _)) {
                    return;
                }
            }
        }
        fn transmitted(&self) -> usize {
            self.shared.lock().unwrap().transmitted.len()
        }
        fn script(&self, step: Step) {
            self.shared.lock().unwrap().script.push_back(step);
        }
        /// Queues a write directly, as a concurrent requester would, returning its answer.
        fn enqueue(&self, value: i32) -> oneshot::Receiver<Result<(), DriverError>> {
            let (reply, answer) = oneshot::channel();
            self.driver
                .commands
                .as_ref()
                .unwrap()
                .try_send(Command::Write {
                    address: S7Address::parse("DB10.INT2").unwrap(),
                    value: Value::Int(value),
                    reply,
                    queued_at: Instant::now(),
                })
                .map_err(|_| ())
                .expect("queue has room");
            answer
        }
        fn wait_entered(&self) {
            self.entered
                .recv_timeout(Duration::from_secs(5))
                .expect("the simulated PLC never received the write");
        }
    }

    fn not_transmitted(result: Result<(), DriverError>) -> bool {
        matches!(result, Err(e) if e.0.starts_with(crate::driver::NOT_TRANSMITTED))
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_command_queue_is_bounded_and_overflow_is_not_transmitted() {
        let mut rig = rig(None);
        rig.driver.start().await.unwrap();
        rig.connected().await;
        let (open, gate) = mpsc::channel();
        rig.script(Step::Block(gate));
        let first = rig.enqueue(1);
        rig.wait_entered(); // the thread is now inside the first write
        let queued: Vec<_> = (0..COMMAND_QUEUE)
            .map(|i| rig.enqueue(10 + i as i32))
            .collect();
        let point = rig.point.clone();
        let overflow = rig.driver.write(&point, &Value::Int(99)).await;
        assert!(
            not_transmitted(overflow),
            "a write beyond the bound must be refused at once"
        );
        open.send(()).unwrap();
        assert!(first.await.unwrap().is_ok());
        for answer in queued {
            assert!(answer.await.unwrap().is_ok());
        }
        assert_eq!(
            rig.transmitted(),
            1 + COMMAND_QUEUE,
            "overflow never reaches the PLC"
        );
        rig.driver.stop().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn stop_is_not_blocked_by_a_saturated_queue_and_drops_queued_writes() {
        let mut rig = rig(None);
        rig.driver.start().await.unwrap();
        rig.connected().await;
        let (open, gate) = mpsc::channel();
        rig.script(Step::Block(gate));
        let first = rig.enqueue(1);
        rig.wait_entered();
        let queued: Vec<_> = (0..COMMAND_QUEUE)
            .map(|i| rig.enqueue(10 + i as i32))
            .collect();
        // The queue is full and the thread is stuck in a transmission that never ends by itself.
        let started = Instant::now();
        rig.driver.stop().await;
        let took = started.elapsed();
        // timeoutMs 100 → bound 3 × 100 ms + 1 s.
        assert!(took < Duration::from_millis(2500), "stop took {took:?}");
        // Unblock the stuck transmission only now: nothing queued may follow it.
        open.send(()).unwrap();
        assert!(
            first.await.unwrap().is_ok(),
            "the write in progress completes normally"
        );
        for answer in queued {
            assert!(not_transmitted(answer.await.unwrap()));
        }
        assert_eq!(
            rig.transmitted(),
            1,
            "no stale queued write was transmitted after stop"
        );
        let point = rig.point.clone();
        assert!(not_transmitted(
            rig.driver.write(&point, &Value::Int(5)).await
        ));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn stop_with_a_full_idle_queue_returns_promptly() {
        let mut rig = rig(None);
        rig.driver.start().await.unwrap();
        rig.connected().await;
        let (open, gate) = mpsc::channel();
        rig.script(Step::Block(gate));
        let first = rig.enqueue(1);
        rig.wait_entered();
        let queued: Vec<_> = (0..COMMAND_QUEUE).map(|i| rig.enqueue(i as i32)).collect();
        open.send(()).unwrap();
        assert!(first.await.unwrap().is_ok());
        // Stop races the drain; whatever was not yet transmitted must be refused.
        let started = Instant::now();
        rig.driver.stop().await;
        assert!(started.elapsed() < Duration::from_millis(1500));
        let mut sent = 1;
        for answer in queued {
            match answer.await.unwrap() {
                Ok(()) => sent += 1,
                other => assert!(not_transmitted(other)),
            }
        }
        assert_eq!(
            rig.transmitted(),
            sent,
            "every success was transmitted, nothing else"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_withdrawn_request_is_not_transmitted() {
        let mut rig = rig(None);
        rig.driver.start().await.unwrap();
        rig.connected().await;
        let (open, gate) = mpsc::channel();
        rig.script(Step::Block(gate));
        let first = rig.enqueue(1);
        rig.wait_entered();
        // The requester gives up while its write is still queued (the engine abandoned it).
        let point = rig.point.clone();
        let gave_up = tokio::time::timeout(
            Duration::from_millis(50),
            rig.driver.write(&point, &Value::Int(2)),
        )
        .await;
        assert!(gave_up.is_err(), "the write was still waiting");
        open.send(()).unwrap();
        assert!(first.await.unwrap().is_ok());
        // Let the thread dequeue the withdrawn write and poll a few times.
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(
            rig.transmitted(),
            1,
            "the withdrawn write never reached the PLC"
        );
        rig.driver.stop().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_write_queued_before_the_connection_is_not_transmitted() {
        let (open, gate) = mpsc::channel();
        let mut rig = rig(Some(gate));
        rig.driver.start().await.unwrap();
        // The thread is inside connect(); this write predates the connection.
        let stale = rig.enqueue(7);
        open.send(()).unwrap();
        rig.connected().await;
        assert!(not_transmitted(stale.await.unwrap()));
        assert_eq!(rig.transmitted(), 0);
        // A write on the current connection goes through.
        let point = rig.point.clone();
        rig.driver.write(&point, &Value::Int(8)).await.unwrap();
        assert_eq!(rig.transmitted(), 1);
        rig.driver.stop().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_failed_transmission_is_outcome_unknown_and_never_replayed() {
        let mut rig = rig(None);
        rig.driver.start().await.unwrap();
        rig.connected().await;
        rig.script(Step::Fail);
        let point = rig.point.clone();
        let error = rig.driver.write(&point, &Value::Int(3)).await.unwrap_err();
        assert!(error.is_outcome_unknown(), "{error}");
        assert_eq!(rig.transmitted(), 1);
        // The connection is rebuilt after the back-off; nothing is sent again by itself.
        rig.connected().await;
        assert_eq!(rig.shared.lock().unwrap().connects, 2);
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(
            rig.transmitted(),
            1,
            "no automatic replay after reconnection"
        );
        rig.driver.stop().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn demand_changes_never_fill_the_queue() {
        let mut rig = rig(None);
        rig.driver.start().await.unwrap();
        rig.connected().await;
        let (open, gate) = mpsc::channel();
        rig.script(Step::Block(gate));
        let first = rig.enqueue(1);
        rig.wait_entered();
        for _ in 0..(COMMAND_QUEUE * 4) {
            rig.driver.set_demand(None).await;
        }
        // At most one coalesced wake is queued: room remains for writes.
        let room: Vec<_> = (0..COMMAND_QUEUE - 1)
            .map(|i| rig.enqueue(i as i32))
            .collect();
        open.send(()).unwrap();
        assert!(first.await.unwrap().is_ok());
        for answer in room {
            assert!(answer.await.unwrap().is_ok());
        }
        rig.driver.stop().await;
    }
}
