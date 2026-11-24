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

use std::collections::{BTreeSet, HashMap};
use std::sync::mpsc::{self, RecvTimeoutError};
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
    },
    /// The demand changed: re-plan now instead of at the next poll.
    Wake,
    Stop,
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
    commands: Option<mpsc::Sender<Command>>,
    thread: Option<JoinHandle<()>>,
    demand: Demand,
}

impl S7Driver {
    pub fn new(
        source: RuntimeSource,
        points: Vec<RuntimePoint>,
        sink: EventSink,
    ) -> Result<Self, DriverError> {
        validate(&source, &points).map_err(DriverError)?;
        Ok(Self {
            source,
            points,
            sink,
            commands: None,
            thread: None,
            demand: Arc::new(Mutex::new((0, None))),
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
        let (tx, rx) = mpsc::channel();
        let link = Link {
            source_id: self.source.id.clone(),
            sink: self.sink.clone(),
        };
        let thread = std::thread::Builder::new()
            .name(format!("s7-{}", self.source.id))
            .spawn({
                let demand = self.demand.clone();
                move || run(settings, readers, rx, link, demand)
            })
            .map_err(|e| DriverError(format!("cannot start the S7 thread: {e}")))?;
        self.commands = Some(tx);
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
            .ok_or_else(|| DriverError("not connected".into()))?;
        let (reply, answer) = oneshot::channel();
        commands
            .send(Command::Write {
                address,
                value: value.clone(),
                reply,
            })
            .map_err(|_| DriverError("the S7 connection has stopped".into()))?;
        answer.await.map_err(|_| {
            DriverError("the S7 connection stopped during the write; its outcome is unknown".into())
        })?
    }

    async fn set_demand(&mut self, demand: Option<&BTreeSet<String>>) {
        {
            let mut current = self.demand.lock().unwrap();
            current.0 += 1;
            current.1 = demand.cloned();
        }
        if let Some(commands) = &self.commands {
            let _ = commands.send(Command::Wake);
        }
    }

    async fn browse(&mut self, _node_id: Option<&str>) -> Result<Vec<BrowseNode>, DriverError> {
        Err(DriverError(
            "S7 has no address browsing; use absolute addresses from TIA Portal (e.g. DB10.REAL4), or the OPC UA driver for symbolic browsing".into(),
        ))
    }

    async fn stop(&mut self) {
        if let Some(commands) = self.commands.take() {
            let _ = commands.send(Command::Stop);
        }
        if let Some(thread) = self.thread.take() {
            let _ = tokio::task::spawn_blocking(move || thread.join()).await;
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
fn back_off(commands: &mpsc::Receiver<Command>, delay: Duration) -> bool {
    let until = Instant::now() + delay;
    loop {
        match commands.recv_timeout(until.saturating_duration_since(Instant::now())) {
            Ok(Command::Stop) | Err(RecvTimeoutError::Disconnected) => return false,
            Ok(Command::Wake) => {} // picked up when the connection is back
            Ok(Command::Write { reply, .. }) => {
                let _ = reply.send(Err(DriverError(
                    "not connected; nothing was written".into(),
                )));
            }
            Err(RecvTimeoutError::Timeout) => return true,
        }
    }
}

fn run(
    settings: Settings,
    all_readers: Vec<Reader>,
    commands: mpsc::Receiver<Command>,
    link: Link,
    demand: Demand,
) {
    // Only the points in use are read; the plan follows the demand.
    let mut seen_demand = u64::MAX;
    let mut readers: Vec<Reader> = Vec::new();
    let mut spans = Vec::new();
    let mut last: HashMap<String, PointValue> = HashMap::new();
    let mut delay = RETRY_INITIAL;
    let mut client: Option<S7Client> = None;
    let mut next_poll = Instant::now();

    let lost = |client: &mut Option<S7Client>,
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
            match connect(&settings) {
                Ok(c) => {
                    client = Some(c);
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
                    if !back_off(&commands, delay) {
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
                if !back_off(&commands, delay) {
                    return;
                }
                delay = (delay * 2).min(RETRY_MAX);
                continue;
            }
            delay = RETRY_INITIAL; // a full poll succeeded: the link is healthy
        }

        // ---- commands until the next poll ----
        match commands.recv_timeout(next_poll.saturating_duration_since(Instant::now())) {
            Ok(Command::Stop) | Err(RecvTimeoutError::Disconnected) => {
                if let Some(mut c) = client.take() {
                    c.disconnect();
                }
                return;
            }
            Err(RecvTimeoutError::Timeout) | Ok(Command::Wake) => {}
            Ok(Command::Write {
                address,
                value,
                reply,
            }) => {
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
                        let _ = reply.send(Err(DriverError(format!(
                            "communication failed during the write; its outcome is unknown ({}). The connection is being re-established",
                            describe(&error)
                        ))));
                        lost(
                            &mut client,
                            &readers,
                            &mut last,
                            &format!("connection lost: {}", describe(&error)),
                            "ConnectionLost",
                        );
                        if !back_off(&commands, delay) {
                            return;
                        }
                        delay = (delay * 2).min(RETRY_MAX);
                    }
                }
            }
        }
    }
}
