//! The engine's state machine: one task, driven by requests, driver events and a timer.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::time::{Duration, Instant};

use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

use crate::driver::{self, Driver, DriverEvent};
use crate::guard::{self, GuardState, MAX_ARM_SECONDS};
use crate::protocol::{
    Message, PointValue, Quality, Request, RuntimeConfig, SourceCredential, SourceState,
    WriteRefusal, PROTOCOL_VERSION,
};

struct Armed {
    actor: String,
    until: Instant,
}

/// A momentary command whose FALSE is still to be written.
struct PendingPulse {
    point: crate::protocol::RuntimePoint,
    actor: String,
    due: Instant,
}

pub struct Engine {
    out: UnboundedSender<Message>,
    events_tx: UnboundedSender<(String, DriverEvent)>,
    greeted: bool,
    config: Option<RuntimeConfig>,
    credentials: BTreeMap<String, SourceCredential>,
    running: bool,
    drivers: BTreeMap<String, Box<dyn Driver>>,
    connected: BTreeSet<String>,
    armed: Option<Armed>,
    /// Variables in use; `None` = all. Values of the others are never forwarded.
    demand: Option<BTreeSet<String>>,
    /// Last report of every readable variable, in use or not.
    latest: HashMap<String, PointValue>,
    /// Running pulses by variable; released by the engine, armed or not.
    pulses: BTreeMap<String, PendingPulse>,
}

/// Runs until the request channel closes (client gone) or a `shutdown` request arrives.
/// Either way every source is disconnected before returning: an engine without its client
/// must not stay connected to machinery.
pub async fn run(mut requests: UnboundedReceiver<Request>, out: UnboundedSender<Message>) {
    let (events_tx, mut events_rx) = unbounded_channel();
    let mut engine = Engine {
        out,
        events_tx,
        greeted: false,
        config: None,
        credentials: BTreeMap::new(),
        running: false,
        drivers: BTreeMap::new(),
        connected: BTreeSet::new(),
        armed: None,
        demand: None,
        latest: HashMap::new(),
        pulses: BTreeMap::new(),
    };
    let mut tick = tokio::time::interval(Duration::from_millis(500));
    loop {
        tokio::select! {
            request = requests.recv() => match request {
                Some(Request::Shutdown { id }) => {
                    engine.stop_all().await;
                    engine.send(Message::Ok { id });
                    break;
                }
                Some(request) => engine.handle(request).await,
                None => break,
            },
            Some((source_id, event)) = events_rx.recv() => engine.on_driver_event(source_id, event),
            _ = tick.tick() => engine.on_tick().await,
            _ = sleep_until_due(engine.next_pulse_due()) => engine.release_due_pulses().await,
        }
    }
    engine.stop_all().await;
}

impl Engine {
    fn send(&self, message: Message) {
        let _ = self.out.send(message);
    }

    fn error(&self, id: u64, code: &str, message: impl Into<String>) {
        self.send(Message::Error {
            id,
            code: code.into(),
            message: message.into(),
        });
    }

    async fn handle(&mut self, request: Request) {
        if let Request::Hello { id, protocol } = request {
            if protocol != PROTOCOL_VERSION {
                self.greeted = false;
                self.error(
                    id,
                    "UNSUPPORTED_PROTOCOL",
                    format!("this engine speaks protocol {PROTOCOL_VERSION}, the client asked for {protocol}"),
                );
            } else {
                self.greeted = true;
                self.send(Message::HelloOk {
                    id,
                    protocol: PROTOCOL_VERSION,
                    engine_version: env!("CARGO_PKG_VERSION").into(),
                    drivers: driver::DRIVERS.iter().map(|d| d.to_string()).collect(),
                });
            }
            return;
        }
        if !self.greeted {
            let id = request_id(&request);
            self.error(
                id,
                "HELLO_REQUIRED",
                "send a matching 'hello' before any other request",
            );
            return;
        }
        match request {
            Request::Hello { .. } | Request::Shutdown { .. } => {
                unreachable!("handled by the caller")
            }
            Request::Configure {
                id,
                config,
                credentials,
            } => {
                // A new configuration starts from a safe state: disconnected and disarmed.
                self.stop_all().await;
                match guard::validate_config(&config, driver::DRIVERS) {
                    Ok(()) => {
                        self.config = Some(config);
                        self.demand = None;
                        self.credentials = credentials;
                        self.send(Message::Ok { id });
                    }
                    Err(problem) => {
                        self.config = None;
                        self.credentials.clear();
                        self.error(id, "INVALID_CONFIG", problem);
                    }
                }
            }
            Request::Start { id } => {
                let Some(config) = self.config.clone() else {
                    return self.error(id, "NOT_CONFIGURED", "no configuration is loaded");
                };
                if self.running {
                    return self.error(id, "ALREADY_RUNNING", "the engine is already running");
                }
                self.running = true;
                self.send(Message::Ok { id });
                for source in &config.sources {
                    self.start_source(&source.id).await;
                }
                if let Some(demand) = self.demand.clone() {
                    let unused = self.not_polled(|variable| !demand.contains(variable));
                    self.send_values(unused);
                    for driver in self.drivers.values_mut() {
                        driver.set_demand(Some(&demand)).await;
                    }
                }
            }
            Request::SetDemand { id, variable_ids } => {
                if self.config.is_none() {
                    return self.error(id, "NOT_CONFIGURED", "no configuration is loaded");
                }
                let next: Option<BTreeSet<String>> = variable_ids.map(|v| v.into_iter().collect());
                let previous = std::mem::replace(&mut self.demand, next.clone());
                self.send(Message::Ok { id });
                if !self.running {
                    return;
                }
                let wanted = |demand: &Option<BTreeSet<String>>, variable: &str| {
                    demand.as_ref().is_none_or(|d| d.contains(variable))
                };
                let dropped = self.not_polled(|v| wanted(&previous, v) && !wanted(&next, v));
                self.send_values(dropped);
                for driver in self.drivers.values_mut() {
                    driver.set_demand(next.as_ref()).await;
                }
                // Variables that come into use on a source that is not connected get no fresh
                // reading: say so instead of leaving them at "not polled".
                let config = self.config.as_ref().expect("checked above");
                let offline: Vec<PointValue> = config
                    .points
                    .iter()
                    .filter(|p| {
                        p.readable()
                            && !wanted(&previous, &p.variable_id)
                            && wanted(&next, &p.variable_id)
                            && !self.connected.contains(&p.source_id)
                    })
                    .map(|p| match self.latest.get(&p.variable_id) {
                        Some(known) if known.quality == Quality::Bad => known.clone(),
                        _ => PointValue {
                            variable_id: p.variable_id.clone(),
                            value: None,
                            quality: Quality::Bad,
                            status: Some("NotConnected".into()),
                            source_timestamp_ms: None,
                        },
                    })
                    .collect();
                self.send_values(offline);
            }
            Request::Stop { id } => {
                self.stop_all().await;
                self.send(Message::Ok { id });
            }
            Request::ArmWrites {
                id,
                actor,
                ttl_seconds,
            } => {
                if !self.running {
                    return self.error(
                        id,
                        "NOT_RUNNING",
                        "writes can only be armed while the engine is running",
                    );
                }
                if actor.trim().is_empty() {
                    return self.error(
                        id,
                        "ACTOR_REQUIRED",
                        "arming writes requires a named actor",
                    );
                }
                if ttl_seconds == 0 || ttl_seconds > MAX_ARM_SECONDS {
                    return self.error(
                        id,
                        "INVALID_TTL",
                        format!("ttlSeconds must be between 1 and {MAX_ARM_SECONDS}"),
                    );
                }
                // Re-arming is a new control period: release any pulse belonging to the
                // previous period before changing the named actor.
                self.release_pulses(true).await;
                self.armed = Some(Armed {
                    actor: actor.clone(),
                    until: Instant::now() + Duration::from_secs(ttl_seconds as u64),
                });
                self.send(Message::Ok { id });
                self.send(Message::WritesArmed {
                    armed: true,
                    actor: Some(actor),
                    expires_in_seconds: Some(ttl_seconds),
                });
            }
            Request::DisarmWrites { id } => {
                // A disarm must write FALSE immediately, not leave an asserted command
                // until its timer expires. Report any failed release to the client.
                self.release_pulses(true).await;
                self.disarm();
                self.send(Message::Ok { id });
            }
            Request::Write {
                id,
                variable_id,
                value,
                actor,
            } => {
                if self.expire_arming() {
                    self.release_pulses(true).await;
                }
                // The actor must match the person who armed the engine, not merely be a
                // non-empty string supplied by the client.
                let decision = if self
                    .armed
                    .as_ref()
                    .is_some_and(|armed| armed.actor == actor)
                {
                    guard::check_write(
                        &GuardState {
                            config: self.config.as_ref(),
                            armed: true,
                            connected_sources: &self.connected,
                        },
                        &variable_id,
                        &value,
                        false,
                    )
                    .cloned()
                } else {
                    Err(WriteRefusal::WritesDisarmed)
                };
                let (refusal, message) = match decision {
                    Err(refusal) => (Some(refusal), None),
                    Ok(point) => match self.drivers.get_mut(&point.source_id) {
                        None => (Some(WriteRefusal::SourceNotConnected), None),
                        Some(driver) => match driver.write(&point, &value).await {
                            Ok(()) => (None, None),
                            Err(error) => {
                                (Some(WriteRefusal::DriverError), Some(error.to_string()))
                            }
                        },
                    },
                };
                let accepted = refusal.is_none();
                self.send(Message::WriteAudit {
                    variable_id,
                    actor,
                    value,
                    accepted,
                    refusal,
                    message: message.clone(),
                });
                self.send(Message::WriteResult {
                    id,
                    accepted,
                    refusal,
                    message,
                });
            }
            Request::Pulse {
                id,
                variable_id,
                actor,
            } => {
                if self.expire_arming() {
                    self.release_pulses(true).await;
                }
                let value = crate::protocol::Value::Bool(true);
                let decision = if !self
                    .armed
                    .as_ref()
                    .is_some_and(|armed| armed.actor == actor)
                {
                    Err(WriteRefusal::WritesDisarmed)
                } else if self.pulses.contains_key(&variable_id) {
                    Err(WriteRefusal::Busy)
                } else {
                    guard::check_write(
                        &GuardState {
                            config: self.config.as_ref(),
                            armed: self.armed.is_some(),
                            connected_sources: &self.connected,
                        },
                        &variable_id,
                        &value,
                        true,
                    )
                    .cloned()
                };
                let (refusal, message) = match decision {
                    Err(refusal) => (Some(refusal), None),
                    Ok(point) => match self.drivers.get_mut(&point.source_id) {
                        None => (Some(WriteRefusal::SourceNotConnected), None),
                        Some(driver) => match driver.write(&point, &value).await {
                            Ok(()) => {
                                let due = Instant::now()
                                    + Duration::from_millis(point.pulse_ms.unwrap_or(0) as u64);
                                self.pulses.insert(
                                    variable_id.clone(),
                                    PendingPulse {
                                        point,
                                        actor: actor.clone(),
                                        due,
                                    },
                                );
                                (None, None)
                            }
                            Err(error) => {
                                (Some(WriteRefusal::DriverError), Some(error.to_string()))
                            }
                        },
                    },
                };
                let accepted = refusal.is_none();
                self.send(Message::WriteAudit {
                    variable_id,
                    actor,
                    value,
                    accepted,
                    refusal,
                    message: message.clone(),
                });
                self.send(Message::WriteResult {
                    id,
                    accepted,
                    refusal,
                    message,
                });
            }
            Request::Browse {
                id,
                source_id,
                node_id,
            } => match self.drivers.get_mut(&source_id) {
                None => self.error(
                    id,
                    "SOURCE_NOT_STARTED",
                    format!("source '{source_id}' is not started"),
                ),
                Some(driver) => match driver.browse(node_id.as_deref()).await {
                    Ok(nodes) => self.send(Message::BrowseResult { id, nodes }),
                    Err(error) => self.error(id, "BROWSE_FAILED", error.to_string()),
                },
            },
        }
    }

    async fn start_source(&mut self, source_id: &str) {
        let Some(config) = self.config.as_ref() else {
            return;
        };
        let Some(source) = config.sources.iter().find(|s| s.id == source_id).cloned() else {
            return;
        };
        let points = config
            .points
            .iter()
            .filter(|p| p.source_id == source_id)
            .cloned()
            .collect();
        let credential = self.credentials.get(source_id).cloned();
        let started = match driver::create(&source, points, credential, self.events_tx.clone()) {
            Ok(mut driver) => match driver.start().await {
                Ok(()) => {
                    self.drivers.insert(source_id.to_owned(), driver);
                    Ok(())
                }
                Err(error) => {
                    driver.stop().await;
                    Err(error)
                }
            },
            Err(error) => Err(error),
        };
        // Drivers connect and retry in the background; an error here is a configuration problem
        // (bad endpoint, address or security setting) that retrying cannot fix.
        if let Err(error) = started {
            self.send(Message::SourceState {
                source_id: source_id.to_owned(),
                state: SourceState::Failed,
                message: Some(format!("cannot start: {error}")),
            });
        }
    }

    fn on_driver_event(&mut self, source_id: String, event: DriverEvent) {
        // Late events from a driver that was already stopped are dropped.
        if !self.running {
            return;
        }
        match event {
            DriverEvent::State(state, message) => {
                if state == SourceState::Connected {
                    self.connected.insert(source_id.clone());
                } else {
                    self.connected.remove(&source_id);
                }
                self.send(Message::SourceState {
                    source_id,
                    state,
                    message,
                });
            }
            DriverEvent::Values(values) => {
                for value in &values {
                    self.latest.insert(value.variable_id.clone(), value.clone());
                }
                let in_use: Vec<PointValue> = match &self.demand {
                    None => values,
                    Some(demand) => values
                        .into_iter()
                        .filter(|v| demand.contains(&v.variable_id))
                        .collect(),
                };
                self.send_values(in_use);
            }
        }
    }

    fn send_values(&self, values: Vec<PointValue>) {
        if !values.is_empty() {
            self.send(Message::Values { values });
        }
    }

    /// "Not polled" reports for the readable variables selected by `which`.
    fn not_polled(&self, which: impl Fn(&str) -> bool) -> Vec<PointValue> {
        let Some(config) = self.config.as_ref() else {
            return Vec::new();
        };
        let mut seen = BTreeSet::new();
        config
            .points
            .iter()
            .filter(|p| p.readable() && which(&p.variable_id) && seen.insert(&p.variable_id))
            .map(|p| PointValue {
                variable_id: p.variable_id.clone(),
                value: None,
                quality: Quality::Uncertain,
                status: Some("NotPolled".into()),
                source_timestamp_ms: None,
            })
            .collect()
    }

    fn next_pulse_due(&self) -> Option<Instant> {
        self.pulses.values().map(|pulse| pulse.due).min()
    }

    /// Writes FALSE for every pulse that is due (all of them when [force]). The release does not
    /// depend on arming: a started pulse is always ended.
    async fn release_pulses(&mut self, force: bool) {
        let now = Instant::now();
        let due: Vec<String> = self
            .pulses
            .iter()
            .filter(|(_, pulse)| force || pulse.due <= now)
            .map(|(variable, _)| variable.clone())
            .collect();
        for variable_id in due {
            let Some(pulse) = self.pulses.remove(&variable_id) else {
                continue;
            };
            let result = match self.drivers.get_mut(&pulse.point.source_id) {
                None => Err("the source is not connected".to_string()),
                Some(driver) => driver
                    .write(&pulse.point, &crate::protocol::Value::Bool(false))
                    .await
                    .map_err(|error| error.to_string()),
            };
            self.send(Message::PulseReleased {
                variable_id,
                actor: pulse.actor,
                released: result.is_ok(),
                message: result.err(),
            });
        }
    }

    async fn release_due_pulses(&mut self) {
        self.release_pulses(false).await;
    }

    async fn on_tick(&mut self) {
        if self.expire_arming() {
            // TTL expiry has the same release semantics as an explicit disarm.
            self.release_pulses(true).await;
        }
    }

    fn expire_arming(&mut self) -> bool {
        if self
            .armed
            .as_ref()
            .is_some_and(|armed| Instant::now() >= armed.until)
        {
            self.disarm();
            true
        } else {
            false
        }
    }

    fn disarm(&mut self) {
        if let Some(armed) = self.armed.take() {
            self.send(Message::WritesArmed {
                armed: false,
                actor: Some(armed.actor),
                expires_in_seconds: None,
            });
        }
    }

    async fn stop_all(&mut self) {
        // End running pulses before the drivers go: a started pulse is always released.
        self.release_pulses(true).await;
        self.disarm();
        let was_running = self.running;
        self.running = false;
        let drivers = std::mem::take(&mut self.drivers);
        for (source_id, mut driver) in drivers {
            driver.stop().await;
            if was_running {
                self.send(Message::SourceState {
                    source_id,
                    state: SourceState::Disconnected,
                    message: None,
                });
            }
        }
        self.connected.clear();
        self.latest.clear();
    }
}

fn request_id(request: &Request) -> u64 {
    match request {
        Request::Hello { id, .. }
        | Request::Configure { id, .. }
        | Request::Start { id }
        | Request::Stop { id }
        | Request::ArmWrites { id, .. }
        | Request::DisarmWrites { id }
        | Request::Write { id, .. }
        | Request::Pulse { id, .. }
        | Request::Browse { id, .. }
        | Request::SetDemand { id, .. }
        | Request::Shutdown { id } => *id,
    }
}

/// Sleeps until [due], or forever when no pulse is running.
async fn sleep_until_due(due: Option<Instant>) {
    match due {
        Some(due) => tokio::time::sleep_until(tokio::time::Instant::from_std(due)).await,
        None => std::future::pending::<()>().await,
    }
}
