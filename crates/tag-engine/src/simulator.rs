//! In-memory simulated PLC.
//!
//! A [`SimulatorHub`] holds one [`SimPlc`] per simulator connection (keyed by
//! connection id). The engine's [`SimulatorFactory`] creates
//! [`SimulatedDriver`]s attached to them; tests and future simulation UIs
//! keep a [`SimPlc`] handle to change PLC-side values, inject latency,
//! connection loss and errors, and replay deterministic scenarios.
//!
//! The simulator never touches hardware or the network. Writes are allowed
//! without any production approval (D-004 / development simulation policy).
//!
//! ## Addresses
//!
//! Any non-empty string is a memory address (e.g. `DB10.Speed`). Unknown
//! addresses are created with the tag's initial value (or the type default)
//! unless the connection sets `"strictAddresses": true`. Generator addresses
//! are read-only and computed from the injected clock:
//!
//! * `sine:offset=1.5,amplitude=0.5,period=10s`
//! * `ramp:min=0,max=100,period=60s`
//! * `square:period=2s` (bool)

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use hmi_project_model::model::{Connection, DataType, Tag};
use serde::Serialize;

use crate::driver::{
    DriverCapabilities, DriverError, DriverErrorKind as K, DriverFactory, ItemResult, OpContext,
    ReadItem, WriteItem,
};
use crate::time::Clock;
use crate::value::{TagValue, TypeContext};

/// Operation counters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SimStats {
    /// Successful connects.
    pub connects: u64,
    /// Read requests answered (successfully or with item errors).
    pub read_requests: u64,
    /// Items read.
    pub items_read: u64,
    /// Write requests answered.
    pub write_requests: u64,
    /// Items written.
    pub items_written: u64,
    /// Requests failed by injected or simulated faults.
    pub failed_requests: u64,
}

/// One step of a deterministic simulation scenario.
#[derive(Clone, Debug, PartialEq)]
pub enum SimStep {
    /// PLC-side value change (as if PLC logic wrote it).
    Set {
        /// Address.
        address: String,
        /// Value.
        value: TagValue,
    },
    /// Reachability: `false` drops every connection and refuses new ones.
    Online(bool),
    /// Response latency.
    Latency(Duration),
    /// The next `count` requests (connect/read/write) fail with `kind`.
    FailNext {
        /// Error kind.
        kind: K,
        /// Number of requests.
        count: u32,
    },
    /// Item-level error for one address until cleared (`None`).
    FailAddress {
        /// Address.
        address: String,
        /// Error, or `None` to clear.
        kind: Option<K>,
    },
}

#[derive(Debug)]
struct SimState {
    memory: BTreeMap<String, TagValue>,
    online: bool,
    latency: Duration,
    fail_next: VecDeque<K>,
    failing_addresses: BTreeMap<String, K>,
    /// Incremented when the PLC goes offline: drivers connected under an
    /// older epoch have lost their connection.
    epoch: u64,
    stats: SimStats,
}

/// Handle to one simulated PLC (cheap to clone; shared state).
#[derive(Clone)]
pub struct SimPlc {
    state: Arc<Mutex<SimState>>,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for SimPlc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SimPlc").finish_non_exhaustive()
    }
}

impl SimPlc {
    /// New, online, zero latency.
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        Self {
            state: Arc::new(Mutex::new(SimState {
                memory: BTreeMap::new(),
                online: true,
                latency: Duration::ZERO,
                fail_next: VecDeque::new(),
                failing_addresses: BTreeMap::new(),
                epoch: 0,
                stats: SimStats::default(),
            })),
            clock,
        }
    }

    fn lock(&self) -> MutexGuard<'_, SimState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Applies scenario steps in order.
    pub fn apply(&self, steps: &[SimStep]) {
        let mut s = self.lock();
        for step in steps {
            match step {
                SimStep::Set { address, value } => {
                    s.memory.insert(address.clone(), value.clone());
                }
                SimStep::Online(on) => {
                    if s.online && !*on {
                        s.epoch += 1;
                    }
                    s.online = *on;
                }
                SimStep::Latency(d) => s.latency = *d,
                SimStep::FailNext { kind, count } => {
                    for _ in 0..*count {
                        s.fail_next.push_back(*kind);
                    }
                }
                SimStep::FailAddress { address, kind } => match kind {
                    Some(k) => {
                        s.failing_addresses.insert(address.clone(), *k);
                    }
                    None => {
                        s.failing_addresses.remove(address);
                    }
                },
            }
        }
    }

    /// Sets a PLC-side value.
    pub fn set(&self, address: &str, value: TagValue) {
        self.apply(&[SimStep::Set {
            address: address.into(),
            value,
        }]);
    }

    /// PLC-side value (generators excluded).
    pub fn get(&self, address: &str) -> Option<TagValue> {
        self.lock().memory.get(address).cloned()
    }

    /// Reachability.
    pub fn set_online(&self, online: bool) {
        self.apply(&[SimStep::Online(online)]);
    }

    /// Latency.
    pub fn set_latency(&self, d: Duration) {
        self.apply(&[SimStep::Latency(d)]);
    }

    /// Counters.
    pub fn stats(&self) -> SimStats {
        self.lock().stats
    }

    /// Addresses in memory, sorted.
    pub fn addresses(&self) -> Vec<String> {
        self.lock().memory.keys().cloned().collect()
    }
}

/// Registry of simulated PLCs, one per simulator connection id.
#[derive(Clone)]
pub struct SimulatorHub {
    plcs: Arc<Mutex<BTreeMap<String, SimPlc>>>,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for SimulatorHub {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SimulatorHub").finish_non_exhaustive()
    }
}

impl SimulatorHub {
    /// New hub; generators and timestamps use `clock`.
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        Self {
            plcs: Arc::new(Mutex::new(BTreeMap::new())),
            clock,
        }
    }

    /// The PLC for a connection id (created on first use).
    pub fn plc(&self, connection: &str) -> SimPlc {
        let mut m = self.plcs.lock().unwrap_or_else(|e| e.into_inner());
        m.entry(connection.to_owned())
            .or_insert_with(|| SimPlc::new(self.clock.clone()))
            .clone()
    }

    /// Factory to register for `DriverKind::Simulator`.
    pub fn factory(&self) -> Arc<SimulatorFactory> {
        Arc::new(SimulatorFactory { hub: self.clone() })
    }
}

/// Creates [`SimulatedDriver`]s.
#[derive(Debug)]
pub struct SimulatorFactory {
    hub: SimulatorHub,
}

impl DriverFactory for SimulatorFactory {
    fn create(
        &self,
        connection: &Connection,
        tags: &[&Tag],
        types: &TypeContext,
    ) -> Result<Box<dyn crate::driver::Driver>, DriverError> {
        let plc = self.hub.plc(connection.id.as_str());
        let p = &connection.parameters;
        let strict = p
            .get("strictAddresses")
            .map(|v| {
                v.as_bool()
                    .ok_or_else(|| bad_param("strictAddresses", "bool"))
            })
            .transpose()?
            .unwrap_or(false);
        if let Some(v) = p.get("latencyMs") {
            let ms = v
                .as_u64()
                .ok_or_else(|| bad_param("latencyMs", "integer"))?;
            plc.set_latency(Duration::from_millis(ms));
        }
        // Seed memory with initial (simulation) values, in project order;
        // existing values are kept so several engines can share one PLC.
        {
            let mut s = plc.lock();
            for t in tags {
                let Some(addr) = t.address.as_deref() else {
                    continue;
                };
                if generator(addr).is_some() || s.memory.contains_key(addr) {
                    continue;
                }
                let v = match &t.initial_value {
                    Some(j) => types.value_from_json(j, &t.data_type),
                    None => seed_default(types, t),
                }
                .map_err(|e| DriverError::new(K::TypeMismatch, format!("tag {}: {e}", t.id)))?;
                s.memory.insert(addr.to_owned(), v);
            }
        }
        Ok(Box::new(SimulatedDriver {
            plc,
            types: types.clone(),
            strict,
            connected_epoch: None,
        }))
    }
}

/// Default simulation value: the type default, moved to `range.min` when
/// the default lies outside the tag's engineering range (so a fresh
/// simulator does not start with out-of-range values).
fn seed_default(types: &TypeContext, t: &Tag) -> Result<TagValue, crate::value::ValueError> {
    let v = types.default_value(&t.data_type)?;
    match (&t.range, v.as_f64()) {
        (Some(r), Some(x)) if x < r.min || x > r.max => {
            let target = if t.data_type.is_integer() {
                r.min.ceil()
            } else {
                r.min
            };
            types.coerce(TagValue::Float64(target), &t.data_type)
        }
        _ => Ok(v),
    }
}

fn bad_param(name: &str, ty: &str) -> DriverError {
    DriverError::new(
        K::Protocol,
        format!("simulator parameter {name:?} must be a {ty}"),
    )
}

/// Driver backed by a [`SimPlc`].
#[derive(Debug)]
pub struct SimulatedDriver {
    plc: SimPlc,
    types: TypeContext,
    strict: bool,
    connected_epoch: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Gen {
    Sine {
        offset: f64,
        amplitude: f64,
        period_s: f64,
    },
    Ramp {
        min: f64,
        max: f64,
        period_s: f64,
    },
    Square {
        period_s: f64,
    },
}

fn parse_duration_s(v: &str) -> Option<f64> {
    let (num, scale) = if let Some(n) = v.strip_suffix("ms") {
        (n, 0.001)
    } else if let Some(n) = v.strip_suffix('s') {
        (n, 1.0)
    } else {
        (v, 1.0)
    };
    num.parse::<f64>()
        .ok()
        .map(|x| x * scale)
        .filter(|x| x.is_finite() && *x > 0.0)
}

fn generator(address: &str) -> Option<Result<Gen, String>> {
    let (kind, params) = address.split_once(':')?;
    if !matches!(kind, "sine" | "ramp" | "square") {
        return None;
    }
    let mut map = BTreeMap::new();
    for kv in params.split(',').filter(|s| !s.is_empty()) {
        let Some((k, v)) = kv.split_once('=') else {
            return Some(Err(format!("malformed generator parameter {kv:?}")));
        };
        map.insert(k.trim(), v.trim());
    }
    let num = |k: &str, d: f64| -> Result<f64, String> {
        map.get(k)
            .map_or(Ok(d), |v| {
                v.parse::<f64>()
                    .map_err(|_| format!("{k} must be a number"))
            })
            .and_then(|x| {
                if x.is_finite() {
                    Ok(x)
                } else {
                    Err(format!("{k} must be finite"))
                }
            })
    };
    let period = map
        .get("period")
        .map_or(Some(10.0), |v| parse_duration_s(v))
        .ok_or_else(|| "period must be a positive duration".to_owned());
    Some((|| {
        let period_s = period?;
        Ok(match kind {
            "sine" => Gen::Sine {
                offset: num("offset", 0.0)?,
                amplitude: num("amplitude", 1.0)?,
                period_s,
            },
            "ramp" => Gen::Ramp {
                min: num("min", 0.0)?,
                max: num("max", 100.0)?,
                period_s,
            },
            _ => Gen::Square { period_s },
        })
    })())
}

impl SimulatedDriver {
    fn latency_and_faults(&self, ctx: &OpContext) -> Result<(), DriverError> {
        let (latency, injected) = {
            let mut s = self.plc.lock();
            (s.latency, s.fail_next.pop_front())
        };
        if !latency.is_zero() {
            let wait = latency.min(ctx.timeout);
            if ctx.cancel.wait(wait) {
                return Err(DriverError::new(K::Cancelled, "cancelled"));
            }
            if latency > ctx.timeout {
                self.plc.lock().stats.failed_requests += 1;
                return Err(DriverError::new(
                    K::Timeout,
                    format!("no response within {} ms", ctx.timeout.as_millis()),
                ));
            }
        }
        if let Some(kind) = injected {
            self.plc.lock().stats.failed_requests += 1;
            return Err(DriverError::new(kind, "injected fault"));
        }
        Ok(())
    }

    fn read_item(&self, s: &mut SimState, item: &ReadItem) -> ItemResult<TagValue> {
        if let Some(k) = s.failing_addresses.get(&item.address) {
            return Err(DriverError::new(
                *k,
                format!("item error at {}", item.address),
            ));
        }
        if let Some(g) = generator(&item.address) {
            let g = g.map_err(|e| DriverError::new(K::AddressInvalid, e))?;
            return generator_value(&g, self.plc.clock.now().0, &item.data_type);
        }
        let v = match s.memory.get(&item.address) {
            Some(v) => v.clone(),
            None if self.strict => {
                return Err(DriverError::new(
                    K::AddressInvalid,
                    format!("unknown address {:?}", item.address),
                ))
            }
            None => {
                let v = self
                    .types
                    .default_value(&item.data_type)
                    .map_err(|e| DriverError::new(K::TypeMismatch, e.to_string()))?;
                s.memory.insert(item.address.clone(), v.clone());
                v
            }
        };
        self.types
            .check(&v, &item.data_type)
            .map_err(|e| DriverError::new(K::TypeMismatch, format!("{}: {e}", item.address)))?;
        Ok(v)
    }

    fn write_item(&self, s: &mut SimState, item: &WriteItem) -> ItemResult<()> {
        if let Some(k) = s.failing_addresses.get(&item.address) {
            return Err(DriverError::new(
                *k,
                format!("item error at {}", item.address),
            ));
        }
        if generator(&item.address).is_some() {
            return Err(DriverError::new(
                K::AccessDenied,
                "generator addresses are read-only",
            ));
        }
        let mut current = match s.memory.get(&item.address) {
            Some(v) => v.clone(),
            None if self.strict => {
                return Err(DriverError::new(
                    K::AddressInvalid,
                    format!("unknown address {:?}", item.address),
                ))
            }
            None => self
                .types
                .default_value(&item.data_type)
                .map_err(|e| DriverError::new(K::TypeMismatch, e.to_string()))?,
        };
        // Member writes are applied atomically under the PLC lock.
        let slot = current.get_mut(&item.member).ok_or_else(|| {
            DriverError::new(K::AddressInvalid, format!("no member {}", item.member))
        })?;
        *slot = item.value.clone();
        self.types
            .check(&current, &item.data_type)
            .map_err(|e| DriverError::new(K::TypeMismatch, e.to_string()))?;
        s.memory.insert(item.address.clone(), current);
        Ok(())
    }
}

fn check_link(epoch: &mut Option<u64>, s: &mut SimState) -> Result<(), DriverError> {
    match *epoch {
        Some(e) if s.online && e == s.epoch => Ok(()),
        Some(_) => {
            *epoch = None;
            s.stats.failed_requests += 1;
            Err(DriverError::new(
                K::ConnectionLost,
                "simulated PLC unreachable",
            ))
        }
        None => Err(DriverError::new(K::NotConnected, "not connected")),
    }
}

/// Fraction of the period elapsed at `t_micros`, computed in integer
/// microseconds so precision does not degrade with large epoch timestamps.
fn phase(t_micros: u64, period_s: f64) -> f64 {
    let period_us = (period_s * 1e6).round().max(1.0) as u64;
    (t_micros % period_us) as f64 / period_us as f64
}

/// Evaluates a generator at time `t_micros` for the requested type.
/// Analog generators feed numeric tags (rounded to nearest for integer
/// types); `square` feeds bool tags.
fn generator_value(g: &Gen, t_micros: u64, dt: &DataType) -> ItemResult<TagValue> {
    let mismatch = || {
        DriverError::new(
            K::TypeMismatch,
            format!("generator cannot produce {}", dt.describe()),
        )
    };
    let x = match *g {
        Gen::Sine {
            offset,
            amplitude,
            period_s,
        } => offset + amplitude * (std::f64::consts::TAU * phase(t_micros, period_s)).sin(),
        Gen::Ramp { min, max, period_s } => min + (max - min) * phase(t_micros, period_s),
        Gen::Square { period_s } => {
            return match dt {
                DataType::Bool => Ok(TagValue::Bool(phase(t_micros, period_s) < 0.5)),
                _ => Err(mismatch()),
            };
        }
    };
    if !dt.is_numeric() {
        return Err(mismatch());
    }
    let x = if dt.is_integer() { x.round() } else { x };
    TypeContext::default()
        .coerce(TagValue::Float64(x), dt)
        .map_err(|e| DriverError::new(K::TypeMismatch, e.to_string()))
}

impl crate::driver::Driver for SimulatedDriver {
    fn capabilities(&self) -> DriverCapabilities {
        DriverCapabilities {
            driver: "simulator".into(),
            grouped_read: true,
            grouped_write: true,
            max_items_per_read: 64,
            max_items_per_write: 64,
            member_writes: true,
            arrays: true,
            structs: true,
            min_scan_interval_ms: 10,
            physical: false,
        }
    }

    fn connect(&mut self, ctx: &OpContext) -> Result<(), DriverError> {
        self.latency_and_faults(ctx)?;
        let mut s = self.plc.lock();
        if !s.online {
            s.stats.failed_requests += 1;
            return Err(DriverError::new(
                K::ConnectFailed,
                "simulated PLC unreachable",
            ));
        }
        self.connected_epoch = Some(s.epoch);
        s.stats.connects += 1;
        Ok(())
    }

    fn disconnect(&mut self) {
        self.connected_epoch = None;
    }

    fn is_connected(&self) -> bool {
        let s = self.plc.lock();
        self.connected_epoch
            .is_some_and(|e| s.online && e == s.epoch)
    }

    fn read(
        &mut self,
        items: &[ReadItem],
        ctx: &OpContext,
    ) -> Result<Vec<ItemResult<TagValue>>, DriverError> {
        let plc = self.plc.clone();
        check_link(&mut self.connected_epoch, &mut plc.lock())?;
        self.latency_and_faults(ctx)?;
        let mut s = plc.lock();
        check_link(&mut self.connected_epoch, &mut s)?;
        let out: Vec<_> = items.iter().map(|i| self.read_item(&mut s, i)).collect();
        s.stats.read_requests += 1;
        s.stats.items_read += items.len() as u64;
        Ok(out)
    }

    fn write(
        &mut self,
        items: &[WriteItem],
        ctx: &OpContext,
    ) -> Result<Vec<ItemResult<()>>, DriverError> {
        let plc = self.plc.clone();
        check_link(&mut self.connected_epoch, &mut plc.lock())?;
        self.latency_and_faults(ctx)?;
        let mut s = plc.lock();
        check_link(&mut self.connected_epoch, &mut s)?;
        let out: Vec<_> = items.iter().map(|i| self.write_item(&mut s, i)).collect();
        s.stats.write_requests += 1;
        s.stats.items_written += out.iter().filter(|r| r.is_ok()).count() as u64;
        Ok(out)
    }
}

/// Addresses that are read-only generators.
pub fn is_generator_address(address: &str) -> bool {
    generator(address).is_some()
}
