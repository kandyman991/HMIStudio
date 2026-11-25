//! The tag engine: value store, access control, subscriptions, driver
//! management, scan groups and lifecycle.
//!
//! ## Concurrency
//!
//! `TagEngine` is `Send + Sync`; every method takes `&self`. Locks are always
//! acquired in this order, which rules out lock-order deadlocks:
//!
//! 1. a connection's driver mutex (serializes all I/O on that connection),
//! 2. a connection's status mutex,
//! 3. the value store (`RwLock`),
//! 4. the subscriber table.
//!
//! Value updates and their notifications happen under the store's write
//! lock, so retained changes stay in global sequence order. Pending values
//! may be coalesced; overflow terminates delivery with a resync marker.
//! A poisoned lock (a panic in another thread) is recovered: every update is computed before it is assigned, so the
//! protected data is never half-written.

use crate::subscription::{self, EventReceiver, Sender, MAX_PENDING, MAX_SUBSCRIPTIONS};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard, Weak};
use std::thread::JoinHandle;
use std::time::Duration;

use hmi_project_model::model::{ConnectionId, DataType, Project, Tag, TagId, ValueRange};
use serde::{Serialize, Serializer};

use crate::driver::{
    CancelToken, ConnectionStatus, Driver, DriverCapabilities, DriverError, DriverErrorKind,
    DriverRegistry, OpContext, ReadItem, WriteItem,
};
use crate::quality::{Quality, QualityReason};
use crate::time::{Clock, Timestamp};
use crate::value::{MemberPath, TagValue, TypeContext, ValueError};

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}
fn read<T>(m: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    m.read().unwrap_or_else(|e| e.into_inner())
}
fn write<T>(m: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    m.write().unwrap_or_else(|e| e.into_inner())
}

/// How scan groups are driven.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanMode {
    /// The caller drives polling with [`TagEngine::poll_due`] /
    /// [`TagEngine::poll_all`]. Fully deterministic; used by tests.
    Manual,
    /// One background thread per connection polls due scan groups.
    Threaded,
}

/// Engine configuration. Per-connection `parameters` (`timeoutMs`,
/// `reconnectMs`, `defaultScanMs`) override the defaults.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineConfig {
    /// Polling mode.
    pub scan_mode: ScanMode,
    /// Scan interval for tags with `scanRateMs = 0`.
    pub default_scan_ms: u32,
    /// Driver operation timeout.
    pub timeout_ms: u64,
    /// Delay before retrying a failed connection.
    pub reconnect_ms: u64,
    /// Longest sleep of a polling thread between checks.
    pub max_idle_ms: u64,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            scan_mode: ScanMode::Threaded,
            default_scan_ms: 1000,
            timeout_ms: 2000,
            reconnect_ms: 2000,
            max_idle_ms: 100,
        }
    }
}

fn ser_value<S: Serializer>(v: &TagValue, s: S) -> Result<S::Ok, S::Error> {
    v.to_json().serialize(s)
}

/// A tag value with quality, timestamp and change sequence.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagSnapshot {
    /// Tag.
    pub tag: TagId,
    /// Value (last known value when quality is bad).
    #[serde(serialize_with = "ser_value")]
    pub value: TagValue,
    /// Quality.
    pub quality: Quality,
    /// Time of the last update or confirmation.
    pub timestamp: Timestamp,
    /// Global sequence number of the last change (0 = initial value).
    pub sequence: u64,
}

/// Subscription event.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "event", rename_all = "camelCase")]
pub enum TagEvent {
    /// Value or quality changed (also sent once per tag on subscribe).
    Changed {
        /// New state.
        snapshot: TagSnapshot,
    },
    /// Connection status changed.
    Connection {
        /// Connection.
        connection: ConnectionId,
        /// New status.
        status: ConnectionStatus,
    },
    /// The engine shut down; no further events follow.
    Closed,
    /// Complete initial snapshot boundary for this subscription generation.
    BaselineComplete { generation: u64 },
    /// Mailbox overflow; discard cached state and subscribe again.
    ResyncRequired,
}

/// Tag operation error.
#[derive(Clone, Debug, PartialEq, Serialize, thiserror::Error)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TagError {
    /// No such tag.
    #[error("unknown tag {tag:?}")]
    UnknownTag {
        /// Tag.
        tag: String,
    },
    /// Access right missing.
    #[error("tag {tag:?} is not {operation}able by the HMI")]
    AccessDenied {
        /// Tag.
        tag: String,
        /// `read` or `write`.
        operation: &'static str,
    },
    /// Value does not fit the tag or member type.
    #[error("tag {tag:?}: {error}")]
    Value {
        /// Tag.
        tag: String,
        /// Cause.
        error: ValueError,
    },
    /// Outside the tag's engineering range.
    #[error("tag {tag:?}: {value} outside [{min}, {max}]")]
    OutOfRange {
        /// Tag.
        tag: String,
        /// Value.
        value: f64,
        /// Minimum.
        min: f64,
        /// Maximum.
        max: f64,
    },
    /// The tag's connection is not connected.
    #[error("connection {connection:?} of tag {tag:?} is not connected")]
    NotConnected {
        /// Tag.
        tag: String,
        /// Connection.
        connection: String,
    },
    /// The tag cannot be served (configuration error).
    #[error("tag {tag:?} unavailable: {reason}")]
    Unavailable {
        /// Tag.
        tag: String,
        /// Reason.
        reason: String,
    },
    /// Driver failure.
    #[error("tag {tag:?}: {error}")]
    Driver {
        /// Tag.
        tag: String,
        /// Cause.
        error: DriverError,
    },
    /// Operation not supported by the driver.
    #[error("tag {tag:?}: {message}")]
    Unsupported {
        /// Tag.
        tag: String,
        /// Details.
        message: String,
    },
    /// Subscription count or initial snapshot exceeds the delivery budget.
    #[error("subscription capacity exceeded")]
    SubscriptionCapacity,
    /// Engine shut down.
    #[error("tag engine is shut down")]
    ShutDown,
}

/// A configuration problem found at initialization (the engine still starts;
/// affected tags have bad quality with reason `configError`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InitIssue {
    /// Tag or connection id.
    pub subject: String,
    /// Problem.
    pub message: String,
}

/// Initialization report.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct InitReport {
    /// Problems in initialization order: tag references (project tag
    /// order), then connections (project order), then initial values.
    pub issues: Vec<InitIssue>,
}

/// One requested write.
#[derive(Clone, Debug, PartialEq)]
pub struct WriteRequest {
    /// Tag.
    pub tag: TagId,
    /// Member (root = whole tag).
    pub member: MemberPath,
    /// Value (coerced to the member type).
    pub value: TagValue,
}

#[derive(Clone, Debug)]
struct TagDef {
    tag: Tag,
    conn: Option<usize>,
    config_error: Option<String>,
}

#[derive(Clone, Debug)]
struct Slot {
    value: TagValue,
    quality: Quality,
    timestamp: Timestamp,
    sequence: u64,
}

#[derive(Debug)]
struct ScanGroup {
    interval_ms: u32,
    tags: Vec<TagId>,
    next_due: Timestamp,
}

struct DriverSlot {
    driver: Option<Box<dyn Driver>>,
    caps: Option<DriverCapabilities>,
    next_connect: Timestamp,
}

struct ConnRt {
    id: ConnectionId,
    timeout: Duration,
    reconnect_ms: u64,
    driver: Mutex<DriverSlot>,
    status: Mutex<ConnectionStatus>,
    groups: Mutex<Vec<ScanGroup>>,
    /// Tags of this connection, project order.
    tags: Vec<TagId>,
}

struct Subscriber {
    filter: Option<BTreeSet<TagId>>,
    tx: Sender,
}

struct Inner {
    types: TypeContext,
    clock: Arc<dyn Clock>,
    defs: BTreeMap<TagId, TagDef>,
    order: Vec<TagId>,
    values: RwLock<BTreeMap<TagId, Slot>>,
    subs: Mutex<BTreeMap<u64, Subscriber>>,
    next_sub: AtomicU64,
    sequence: AtomicU64,
    conns: Vec<ConnRt>,
    running: AtomicBool,
    cancel: CancelToken,
}

/// The industrial tag engine for one project.
pub struct TagEngine {
    inner: Arc<Inner>,
    threads: Mutex<Vec<JoinHandle<()>>>,
}

impl std::fmt::Debug for TagEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TagEngine")
            .field("tags", &self.inner.order.len())
            .field("connections", &self.inner.conns.len())
            .field("running", &self.inner.running.load(Ordering::SeqCst))
            .finish()
    }
}

/// RAII subscription; dropping it unsubscribes.
#[derive(Debug)]
pub struct Subscription {
    id: u64,
    inner: Weak<Inner>,
}

impl Subscription {
    /// Subscription id.
    pub fn id(&self) -> u64 {
        self.id
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.upgrade() {
            if let Some(sub) = lock(&inner.subs).remove(&self.id) {
                sub.tx.cancel();
            }
        }
    }
}

fn param_u64(c: &hmi_project_model::model::Connection, k: &str) -> Result<Option<u64>, String> {
    match c.parameters.get(k) {
        None => Ok(None),
        Some(v) => v
            .as_u64()
            .map(Some)
            .ok_or_else(|| format!("parameter {k:?} must be a non-negative integer")),
    }
}

fn reason_for(kind: DriverErrorKind) -> QualityReason {
    match kind {
        DriverErrorKind::NotConnected
        | DriverErrorKind::ConnectFailed
        | DriverErrorKind::ConnectionLost => QualityReason::ConnectionLost,
        DriverErrorKind::Timeout => QualityReason::Timeout,
        DriverErrorKind::TypeMismatch => QualityReason::TypeMismatch,
        DriverErrorKind::AddressInvalid | DriverErrorKind::Unsupported => {
            QualityReason::ConfigError
        }
        DriverErrorKind::Cancelled => QualityReason::Shutdown,
        DriverErrorKind::AccessDenied | DriverErrorKind::Protocol => {
            QualityReason::CommunicationError
        }
    }
}

fn out_of_range(range: Option<&ValueRange>, v: &TagValue) -> Option<(f64, f64, f64)> {
    let r = range?;
    let x = v.as_f64()?;
    // Compare integral values against ceil(min)/floor(max) without rounding
    // the tag to f64. All tag integers fit in i128; saturating float casts
    // are harmless for bounds beyond the entire tag integer domain.
    let outside = if let Some(i) = v.as_i128() {
        i < r.min.ceil() as i128 || i > r.max.floor() as i128
    } else {
        x < r.min || x > r.max
    };
    (outside || !r.min.is_finite() || !r.max.is_finite() || r.min > r.max)
        .then_some((x, r.min, r.max))
}

impl TagEngine {
    /// Builds the engine for `project` deterministically (connections and
    /// tags in project order). Configuration problems are reported, not
    /// fatal. In [`ScanMode::Threaded`] polling threads start immediately.
    pub fn new(
        project: &Project,
        registry: &DriverRegistry,
        clock: Arc<dyn Clock>,
        config: EngineConfig,
    ) -> (TagEngine, InitReport) {
        // This engine does not yet implement the v2 policy/authorization
        // semantics. Never start drivers or offer writes for such a project.
        if !project.v2.is_empty()
            || project.schema_version != hmi_project_model::CURRENT_SCHEMA_VERSION
        {
            let mut empty = project.clone();
            empty.schema_version = hmi_project_model::CURRENT_SCHEMA_VERSION;
            empty.v2 = Default::default();
            empty.tags.clear();
            empty.connections.clear();
            let mut safe_config = config;
            safe_config.scan_mode = ScanMode::Manual;
            let (engine, mut report) = Self::new(&empty, registry, clock, safe_config);
            engine.shutdown();
            report.issues.push(InitIssue {
                subject: project.id.to_string(),
                message: "unsupported project declarations: runtime activation refused".into(),
            });
            return (engine, report);
        }
        let types = TypeContext::from_project(project);
        let mut report = InitReport::default();
        let now = clock.now();

        // Tags first (connections need their tag lists).
        let conn_index: BTreeMap<&str, usize> = project
            .connections
            .iter()
            .enumerate()
            .rev() // first occurrence wins
            .map(|(i, c)| (c.id.as_str(), i))
            .collect();
        let mut defs = BTreeMap::new();
        let mut order = Vec::new();
        for t in &project.tags {
            if defs.contains_key(&t.id) {
                report.issues.push(InitIssue {
                    subject: t.id.to_string(),
                    message: "duplicate tag id; first definition used".into(),
                });
                continue;
            }
            let mut config_error = None;
            let conn = match &t.connection {
                None => None,
                Some(c) => match conn_index.get(c.as_str()) {
                    Some(i) => Some(*i),
                    None => {
                        config_error = Some(format!("unknown connection {c:?}"));
                        None
                    }
                },
            };
            if conn.is_some() && t.address.as_deref().is_none_or(|a| a.trim().is_empty()) {
                config_error = Some("connection tag without address".into());
            }
            if let Some(e) = &config_error {
                report.issues.push(InitIssue {
                    subject: t.id.to_string(),
                    message: e.clone(),
                });
            }
            order.push(t.id.clone());
            defs.insert(
                t.id.clone(),
                TagDef {
                    tag: t.clone(),
                    conn: if config_error.is_some() { None } else { conn },
                    config_error,
                },
            );
        }

        // Connections and drivers.
        let mut conns = Vec::new();
        for (ci, c) in project.connections.iter().enumerate() {
            if conn_index.get(c.id.as_str()) != Some(&ci) {
                report.issues.push(InitIssue {
                    subject: c.id.to_string(),
                    message: "duplicate connection id; ignored".into(),
                });
                continue;
            }
            let owned: Vec<Tag> = order
                .iter()
                .filter_map(|id| defs.get(id))
                .filter(|d| d.conn == Some(ci))
                .map(|d| d.tag.clone())
                .collect();
            let tags: Vec<&Tag> = owned.iter().collect();
            let params = (|| -> Result<(u64, u64, u64), String> {
                Ok((
                    param_u64(c, "timeoutMs")?.unwrap_or(config.timeout_ms),
                    param_u64(c, "reconnectMs")?.unwrap_or(config.reconnect_ms),
                    param_u64(c, "defaultScanMs")?.unwrap_or(u64::from(config.default_scan_ms)),
                ))
            })();
            let (driver, error) = match (&params, registry.factory(c.driver)) {
                (Err(e), _) => (
                    None,
                    Some(DriverError::new(DriverErrorKind::Protocol, e.clone())),
                ),
                (_, None) => (
                    None,
                    Some(DriverError::new(
                        DriverErrorKind::Unsupported,
                        format!(
                            "no driver registered for {:?} (only the simulator exists in RUST-06A)",
                            c.driver
                        ),
                    )),
                ),
                (Ok(_), Some(f)) => match f.create(c, &tags, &types) {
                    Ok(d) => (Some(d), None),
                    Err(e) => (None, Some(e)),
                },
            };
            let (timeout, reconnect, default_scan) = params.unwrap_or((
                config.timeout_ms,
                config.reconnect_ms,
                u64::from(config.default_scan_ms),
            ));
            let caps = driver.as_ref().map(|d| d.capabilities());
            let min_scan = caps.as_ref().map_or(1, |c| c.min_scan_interval_ms.max(1));
            // Scan groups: readable tags grouped by effective interval.
            let mut by_interval: BTreeMap<u32, Vec<TagId>> = BTreeMap::new();
            for t in &tags {
                if !t.access.readable() {
                    continue;
                }
                let iv = if t.scan_rate_ms == 0 {
                    u32::try_from(default_scan).unwrap_or(u32::MAX)
                } else {
                    t.scan_rate_ms
                };
                by_interval
                    .entry(iv.max(min_scan))
                    .or_default()
                    .push(t.id.clone());
            }
            let groups = by_interval
                .into_iter()
                .map(|(interval_ms, tags)| ScanGroup {
                    interval_ms,
                    tags,
                    next_due: now,
                })
                .collect();
            let status = match &error {
                Some(e) => {
                    report.issues.push(InitIssue {
                        subject: c.id.to_string(),
                        message: e.to_string(),
                    });
                    ConnectionStatus::Unavailable { error: e.clone() }
                }
                None => ConnectionStatus::Disconnected,
            };
            if error.is_some() {
                for t in &tags {
                    if let Some(d) = defs.get_mut(&t.id) {
                        d.config_error = Some(format!("connection {} unavailable", c.id));
                    }
                }
            }
            conns.push(ConnRt {
                id: c.id.clone(),
                timeout: Duration::from_millis(timeout),
                reconnect_ms: reconnect,
                driver: Mutex::new(DriverSlot {
                    driver,
                    caps,
                    next_connect: now,
                }),
                status: Mutex::new(status),
                groups: Mutex::new(groups),
                tags: tags.iter().map(|t| t.id.clone()).collect(),
            });
        }
        // Map tag.conn indices from project connection index to conns index.
        let pos: BTreeMap<usize, usize> = project
            .connections
            .iter()
            .enumerate()
            .filter_map(|(ci, c)| conns.iter().position(|r| r.id == c.id).map(|ri| (ci, ri)))
            .collect();
        for d in defs.values_mut() {
            d.conn = d.conn.and_then(|ci| pos.get(&ci).copied());
        }

        // Initial values, project order.
        let mut values = BTreeMap::new();
        for id in &order {
            let Some(d) = defs.get_mut(id) else { continue };
            let t = &d.tag;
            let mut init_problem = None;
            let value = match &t.initial_value {
                Some(j) => match types.value_from_json(j, &t.data_type) {
                    Ok(v) => v,
                    Err(e) => {
                        init_problem = Some(format!("invalid initialValue: {e}"));
                        types
                            .default_value(&t.data_type)
                            .unwrap_or(TagValue::Bool(false))
                    }
                },
                None => match types.default_value(&t.data_type) {
                    Ok(v) => v,
                    Err(e) => {
                        init_problem = Some(format!("invalid data type: {e}"));
                        TagValue::Bool(false)
                    }
                },
            };
            if let Some(p) = init_problem {
                report.issues.push(InitIssue {
                    subject: id.to_string(),
                    message: p.clone(),
                });
                d.config_error.get_or_insert(p);
            }
            let quality = if d.config_error.is_some() {
                Quality::bad(QualityReason::ConfigError)
            } else if d.conn.is_none() {
                Quality::GOOD
            } else if !t.access.readable() {
                Quality::uncertain(QualityReason::NotReadable)
            } else {
                Quality::bad(QualityReason::NotConnected)
            };
            values.insert(
                id.clone(),
                Slot {
                    value,
                    quality,
                    timestamp: now,
                    sequence: 0,
                },
            );
        }

        let inner = Arc::new(Inner {
            types,
            clock,
            defs,
            order,
            values: RwLock::new(values),
            subs: Mutex::new(BTreeMap::new()),
            next_sub: AtomicU64::new(1),
            sequence: AtomicU64::new(0),
            conns,
            running: AtomicBool::new(true),
            cancel: CancelToken::new(),
        });
        let engine = TagEngine {
            inner,
            threads: Mutex::new(Vec::new()),
        };
        if config.scan_mode == ScanMode::Threaded {
            let mut threads = lock(&engine.threads);
            for i in 0..engine.inner.conns.len() {
                let inner = engine.inner.clone();
                let max_idle = config.max_idle_ms.max(1);
                let handle = std::thread::Builder::new()
                    .name(format!("tag-scan-{}", engine.inner.conns[i].id))
                    .spawn(move || inner.scan_loop(i, max_idle));
                if let Ok(h) = handle {
                    threads.push(h);
                }
            }
        }
        (engine, report)
    }

    // -------------------------------------------------------------- queries

    /// Tag ids in project order.
    pub fn tag_ids(&self) -> &[TagId] {
        &self.inner.order
    }

    /// Tag definition.
    pub fn definition(&self, tag: &TagId) -> Option<&Tag> {
        self.inner.defs.get(tag).map(|d| &d.tag)
    }

    /// Struct type context.
    pub fn types(&self) -> &TypeContext {
        &self.inner.types
    }

    /// Whether the engine is running.
    pub fn is_running(&self) -> bool {
        self.inner.running.load(Ordering::SeqCst)
    }

    /// Status of a connection.
    pub fn connection_status(&self, id: &ConnectionId) -> Option<ConnectionStatus> {
        self.inner
            .conns
            .iter()
            .find(|c| &c.id == id)
            .map(|c| lock(&c.status).clone())
    }

    /// Capabilities of a connection's driver.
    pub fn capabilities(&self, id: &ConnectionId) -> Option<DriverCapabilities> {
        let c = self.inner.conns.iter().find(|c| &c.id == id)?;
        lock(&c.driver).caps.clone()
    }

    /// Cached value of a readable tag.
    pub fn read(&self, tag: &TagId) -> Result<TagSnapshot, TagError> {
        self.inner.ensure_running()?;
        let d = self.inner.def(tag)?;
        if !d.tag.access.readable() {
            return Err(TagError::AccessDenied {
                tag: tag.to_string(),
                operation: "read",
            });
        }
        Ok(self.inner.snapshot(tag))
    }

    /// Cached value of a member (`member` in RUST-03A path syntax).
    pub fn read_member(&self, tag: &TagId, member: &str) -> Result<TagSnapshot, TagError> {
        let path = MemberPath::parse(member).map_err(|error| TagError::Value {
            tag: tag.to_string(),
            error,
        })?;
        let mut s = self.read(tag)?;
        let v = s.value.get(&path).cloned().ok_or_else(|| TagError::Value {
            tag: tag.to_string(),
            error: ValueError::MalformedPath(member.into()),
        })?;
        s.value = v;
        Ok(s)
    }

    /// Reads tags from their devices now (grouped per connection) and
    /// returns fresh snapshots. Internal tags return their cached value.
    pub fn read_device(&self, tags: &[TagId]) -> Vec<Result<TagSnapshot, TagError>> {
        if let Err(e) = self.inner.ensure_running() {
            return tags.iter().map(|_| Err(e.clone())).collect();
        }
        let mut results: Vec<Option<Result<TagSnapshot, TagError>>> = vec![None; tags.len()];
        let mut per_conn: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for (i, t) in tags.iter().enumerate() {
            match self.inner.def(t) {
                Err(e) => results[i] = Some(Err(e)),
                Ok(d) if !d.tag.access.readable() => {
                    results[i] = Some(Err(TagError::AccessDenied {
                        tag: t.to_string(),
                        operation: "read",
                    }))
                }
                Ok(d) => match (d.conn, &d.config_error) {
                    (_, Some(r)) => {
                        results[i] = Some(Err(TagError::Unavailable {
                            tag: t.to_string(),
                            reason: r.clone(),
                        }))
                    }
                    (None, None) => results[i] = Some(Ok(self.inner.snapshot(t))),
                    (Some(c), None) => per_conn.entry(c).or_default().push(i),
                },
            }
        }
        for (ci, idxs) in per_conn {
            let ids: Vec<TagId> = idxs.iter().map(|&i| tags[i].clone()).collect();
            let errs = self.inner.read_from_device(ci, &ids);
            for (k, &i) in idxs.iter().enumerate() {
                results[i] = Some(match &errs[k] {
                    Some(e) => Err(e.clone()),
                    None => Ok(self.inner.snapshot(&tags[i])),
                });
            }
        }
        results
            .into_iter()
            .map(|r| r.unwrap_or(Err(TagError::ShutDown)))
            .collect()
    }

    // --------------------------------------------------------------- writes

    /// Writes a whole tag value (coerced to the tag type).
    pub fn write(&self, tag: &TagId, value: TagValue) -> Result<(), TagError> {
        self.write_member(tag, &MemberPath::root(), value)
    }

    /// Writes a JSON value (project-document form) to a tag or member.
    pub fn write_json(
        &self,
        tag: &TagId,
        member: Option<&str>,
        value: &serde_json::Value,
    ) -> Result<(), TagError> {
        let path = MemberPath::parse_opt(member).map_err(|error| TagError::Value {
            tag: tag.to_string(),
            error,
        })?;
        let d = self.inner.def(tag)?;
        let mt = self
            .inner
            .types
            .member_type(&d.tag.data_type, &path)
            .map_err(|error| TagError::Value {
                tag: tag.to_string(),
                error,
            })?;
        let v = self
            .inner
            .types
            .value_from_json(value, &mt)
            .map_err(|error| TagError::Value {
                tag: tag.to_string(),
                error,
            })?;
        self.write_member(tag, &path, v)
    }

    /// Writes one member (or the whole tag for the root path).
    pub fn write_member(
        &self,
        tag: &TagId,
        member: &MemberPath,
        value: TagValue,
    ) -> Result<(), TagError> {
        self.write_many(vec![WriteRequest {
            tag: tag.clone(),
            member: member.clone(),
            value,
        }])
        .pop()
        .unwrap_or(Err(TagError::ShutDown))
    }

    /// Grouped writes. Every request is validated first; internal tags are
    /// then updated together, device writes are grouped per connection (in
    /// request order, split by the driver's request size). Results are
    /// returned in request order. Not transactional across items: a device
    /// may accept some items of a group and reject others.
    pub fn write_many(&self, requests: Vec<WriteRequest>) -> Vec<Result<(), TagError>> {
        if let Err(e) = self.inner.ensure_running() {
            return requests.iter().map(|_| Err(e.clone())).collect();
        }
        let mut results: Vec<Option<Result<(), TagError>>> = vec![None; requests.len()];
        let mut internal = Vec::new();
        let mut per_conn: BTreeMap<usize, Vec<(usize, WriteItem)>> = BTreeMap::new();
        for (i, r) in requests.into_iter().enumerate() {
            match self.inner.prepare_write(r) {
                Err(e) => results[i] = Some(Err(e)),
                Ok((None, item)) => internal.push((i, item)),
                Ok((Some(ci), item)) => per_conn.entry(ci).or_default().push((i, item)),
            }
        }
        if !internal.is_empty() {
            let items: Vec<WriteItem> = internal.iter().map(|(_, w)| w.clone()).collect();
            let applied = self
                .inner
                .apply_written(&items, &vec![true; items.len()], true);
            for (i, _) in internal {
                results[i] = Some(if applied {
                    Ok(())
                } else {
                    Err(TagError::ShutDown)
                });
            }
        }
        for (ci, items) in per_conn {
            let r = self
                .inner
                .write_to_device(ci, items.iter().map(|(_, w)| w.clone()).collect());
            for ((i, _), res) in items.iter().zip(r) {
                results[*i] = Some(res);
            }
        }
        results
            .into_iter()
            .map(|r| r.unwrap_or(Err(TagError::ShutDown)))
            .collect()
    }

    // -------------------------------------------------------- subscriptions

    /// Subscribes to changes of `tags` (`None` = every tag and connection).
    /// The receiver first gets one `changed` event per subscribed tag with
    /// its current state, current connection statuses (unfiltered only),
    /// and a BaselineComplete marker, then retained changes in global order. Pending
    /// changes to the same tag can coalesce. Overflow emits ResyncRequired
    /// and disconnects; resubscribe for a fresh atomic baseline.
    /// Write-only tags are never reported.
    pub fn subscribe(
        &self,
        tags: Option<&[TagId]>,
    ) -> Result<(Subscription, EventReceiver), TagError> {
        self.inner.ensure_running()?;
        let filter = match tags {
            None => None,
            Some(list) => {
                let mut set = BTreeSet::new();
                for t in list {
                    let d = self.inner.def(t)?;
                    if !d.tag.access.readable() {
                        return Err(TagError::AccessDenied {
                            tag: t.to_string(),
                            operation: "read",
                        });
                    }
                    set.insert(t.clone());
                    if set.len() >= MAX_PENDING {
                        return Err(TagError::SubscriptionCapacity);
                    }
                }
                Some(set)
            }
        };
        let (tx, rx) = subscription::channel();
        let id = self
            .inner
            .next_sub
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_add(1))
            .map_err(|_| TagError::SubscriptionCapacity)?;
        // Lock status before values and subscriber table, following the global lock
        // order. Hold through admission so status notifications cannot overtake baseline.
        if filter.is_none() && self.inner.conns.len() >= MAX_PENDING {
            return Err(TagError::SubscriptionCapacity);
        }
        let statuses: Vec<_> = if filter.is_none() {
            self.inner.conns.iter().map(|c| lock(&c.status)).collect()
        } else {
            Vec::new()
        };
        // Values read lock, then subscriber lock: no update can interleave.
        let values = read(&self.inner.values);
        let mut subs = lock(&self.inner.subs);
        self.inner.ensure_running()?;
        subs.retain(|_, sub| !sub.tx.receiver_gone());
        if subs.len() >= MAX_SUBSCRIPTIONS {
            return Err(TagError::SubscriptionCapacity);
        }
        for t in &self.inner.order {
            let readable = self
                .inner
                .defs
                .get(t)
                .is_some_and(|d| d.tag.access.readable());
            if readable && filter.as_ref().is_none_or(|f| f.contains(t)) {
                if let Some(s) = values.get(t) {
                    tx.send(TagEvent::Changed {
                        snapshot: Inner::snap(t, s),
                    })
                    .map_err(|_| TagError::SubscriptionCapacity)?;
                }
            }
        }
        for (c, status) in self.inner.conns.iter().zip(statuses.iter()) {
            tx.send(TagEvent::Connection {
                connection: c.id.clone(),
                status: (**status).clone(),
            })
            .map_err(|_| TagError::SubscriptionCapacity)?;
        }
        tx.send(TagEvent::BaselineComplete { generation: id })
            .map_err(|_| TagError::SubscriptionCapacity)?;
        subs.insert(id, Subscriber { filter, tx });
        drop(subs);
        drop(values);
        Ok((
            Subscription {
                id,
                inner: Arc::downgrade(&self.inner),
            },
            rx,
        ))
    }

    /// Number of live subscriptions.
    pub fn subscription_count(&self) -> usize {
        lock(&self.inner.subs).len()
    }

    // -------------------------------------------------------------- polling

    /// Polls scan groups that are due (manual mode; also safe in threaded
    /// mode). Returns the number of items read.
    pub fn poll_due(&self) -> usize {
        (0..self.inner.conns.len())
            .map(|i| self.inner.step(i, false))
            .sum()
    }

    /// Polls every scan group now and retries disconnected connections
    /// regardless of the reconnect delay.
    pub fn poll_all(&self) -> usize {
        (0..self.inner.conns.len())
            .map(|i| self.inner.step(i, true))
            .sum()
    }

    // ------------------------------------------------------------- shutdown

    /// Stops polling, disconnects drivers (project order), marks every tag
    /// bad with reason `shutdown`, sends `closed` and ends all
    /// subscriptions. Idempotent; later operations return `ShutDown`.
    pub fn shutdown(&self) {
        if !self.inner.running.swap(false, Ordering::SeqCst) {
            return;
        }
        self.inner.cancel.cancel();
        let threads: Vec<JoinHandle<()>> = lock(&self.threads).drain(..).collect();
        for t in threads {
            let _ = t.join();
        }
        for c in &self.inner.conns {
            let mut d = lock(&c.driver);
            if let Some(drv) = d.driver.as_mut() {
                drv.disconnect();
            }
            drop(d);
            self.inner.set_status(c, ConnectionStatus::Stopped);
        }
        let updates: Vec<(TagId, Option<TagValue>, Quality)> = self
            .inner
            .order
            .iter()
            .map(|t| (t.clone(), None, Quality::bad(QualityReason::Shutdown)))
            .collect();
        self.inner.update(updates);
        let mut subs = lock(&self.inner.subs);
        for s in subs.values() {
            let _ = s.tx.send(TagEvent::Closed);
        }
        subs.clear();
    }
}

impl Drop for TagEngine {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl Inner {
    fn ensure_running(&self) -> Result<(), TagError> {
        if self.running.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(TagError::ShutDown)
        }
    }

    fn def(&self, tag: &TagId) -> Result<&TagDef, TagError> {
        self.defs.get(tag).ok_or_else(|| TagError::UnknownTag {
            tag: tag.to_string(),
        })
    }

    fn snap(tag: &TagId, s: &Slot) -> TagSnapshot {
        TagSnapshot {
            tag: tag.clone(),
            value: s.value.clone(),
            quality: s.quality,
            timestamp: s.timestamp,
            sequence: s.sequence,
        }
    }

    fn snapshot(&self, tag: &TagId) -> TagSnapshot {
        let values = read(&self.values);
        values.get(tag).map_or_else(
            || TagSnapshot {
                tag: tag.clone(),
                value: TagValue::Bool(false),
                quality: Quality::bad(QualityReason::ConfigError),
                timestamp: Timestamp(0),
                sequence: 0,
            },
            |s| Self::snap(tag, s),
        )
    }

    /// Applies updates and notifies. `None` value = quality-only update.
    fn update(&self, updates: Vec<(TagId, Option<TagValue>, Quality)>) {
        if updates.is_empty() {
            return;
        }
        let now = self.clock.now();
        let mut values = write(&self.values);
        let mut events = Vec::new();
        for (id, value, quality) in updates {
            let Some(slot) = values.get_mut(&id) else {
                continue;
            };
            let changed =
                value.as_ref().is_some_and(|v| *v != slot.value) || quality != slot.quality;
            if let Some(v) = value {
                slot.value = v;
            }
            slot.quality = quality;
            slot.timestamp = now;
            if changed {
                slot.sequence = self.sequence.fetch_add(1, Ordering::SeqCst) + 1;
                events.push(Self::snap(&id, slot));
            }
        }
        if events.is_empty() {
            return;
        }
        let mut subs = lock(&self.subs);
        let mut dead = Vec::new();
        for (sid, s) in subs.iter() {
            for e in &events {
                let readable = self
                    .defs
                    .get(&e.tag)
                    .is_some_and(|d| d.tag.access.readable());
                if !readable || !s.filter.as_ref().is_none_or(|f| f.contains(&e.tag)) {
                    continue;
                }
                if s.tx
                    .send(TagEvent::Changed {
                        snapshot: e.clone(),
                    })
                    .is_err()
                {
                    dead.push(*sid);
                    break;
                }
            }
        }
        for sid in dead {
            subs.remove(&sid);
        }
    }

    fn set_status(&self, c: &ConnRt, status: ConnectionStatus) {
        let mut st = lock(&c.status);
        if *st == status {
            return;
        }
        *st = status.clone();
        // Keep the status guard until notifications are enqueued, so an initial
        // snapshot cannot observe the new status before an older notification.
        let mut subs = lock(&self.subs);
        subs.retain(|_, s| {
            if s.filter.is_some() {
                return true;
            }
            s.tx.send(TagEvent::Connection {
                connection: c.id.clone(),
                status: status.clone(),
            })
            .is_ok()
        });
    }

    fn ctx(&self, c: &ConnRt) -> OpContext {
        OpContext {
            timeout: c.timeout,
            cancel: self.cancel.clone(),
        }
    }

    fn mark_connection(&self, c: &ConnRt, reason: QualityReason) {
        let updates = c
            .tags
            .iter()
            .filter(|t| self.defs.get(*t).is_some_and(|d| d.tag.access.readable()))
            .map(|t| (t.clone(), None, Quality::bad(reason)))
            .collect();
        self.update(updates);
    }

    fn connection_lost(&self, c: &ConnRt, d: &mut DriverSlot, e: DriverError) {
        if let Some(drv) = d.driver.as_mut() {
            drv.disconnect();
        }
        d.next_connect = self.clock.now().plus_millis(c.reconnect_ms);
        self.set_status(c, ConnectionStatus::Faulted { error: e });
        self.mark_connection(c, QualityReason::ConnectionLost);
    }

    /// Connects if needed. Returns whether the connection is usable.
    fn ensure_connected(&self, c: &ConnRt, d: &mut DriverSlot, force: bool) -> bool {
        if self.ensure_running().is_err() {
            return false;
        }
        let Some(drv) = d.driver.as_mut() else {
            return false;
        };
        let was_connected = matches!(*lock(&c.status), ConnectionStatus::Connected);
        if was_connected && drv.is_connected() {
            return true;
        }
        if was_connected {
            // Dropped since the last operation.
            let e = DriverError::new(DriverErrorKind::ConnectionLost, "connection dropped");
            self.connection_lost(c, d, e);
            if !force {
                return false;
            }
        }
        let now = self.clock.now();
        if !force && now < d.next_connect {
            return false;
        }
        self.set_status(c, ConnectionStatus::Connecting);
        let ctx = self.ctx(c);
        let Some(drv) = d.driver.as_mut() else {
            return false;
        };
        match drv.connect(&ctx) {
            Ok(()) => {
                self.set_status(c, ConnectionStatus::Connected);
                for g in lock(&c.groups).iter_mut() {
                    g.next_due = now;
                }
                true
            }
            Err(e) => {
                d.next_connect = now.plus_millis(c.reconnect_ms);
                self.set_status(c, ConnectionStatus::Faulted { error: e });
                self.mark_connection(c, QualityReason::ConnectionLost);
                false
            }
        }
    }

    /// One polling step for connection `i`. Returns items read.
    fn step(&self, i: usize, force: bool) -> usize {
        if !self.running.load(Ordering::SeqCst) {
            return 0;
        }
        let Some(c) = self.conns.get(i) else { return 0 };
        let mut d = lock(&c.driver);
        if !self.ensure_connected(c, &mut d, force) {
            return 0;
        }
        let now = self.clock.now();
        let ids: Vec<TagId> = {
            let mut groups = lock(&c.groups);
            let mut ids = Vec::new();
            for g in groups.iter_mut() {
                if force || g.next_due <= now {
                    ids.extend(g.tags.iter().cloned());
                    g.next_due = now.plus_millis(u64::from(g.interval_ms));
                }
            }
            ids
        };
        let n = ids.len();
        self.read_locked(c, &mut d, &ids);
        n
    }

    /// Reads `ids` (all of connection `c`) with the driver lock held.
    /// Returns per-tag errors (None = read ok).
    fn read_locked(&self, c: &ConnRt, d: &mut DriverSlot, ids: &[TagId]) -> Vec<Option<TagError>> {
        let mut errs = vec![None; ids.len()];
        let max = d.caps.as_ref().map_or(1, |c| c.max_items_per_read.max(1));
        let items: Vec<ReadItem> = ids
            .iter()
            .filter_map(|id| self.defs.get(id))
            .map(|def| ReadItem {
                tag: def.tag.id.clone(),
                address: def.tag.address.clone().unwrap_or_default(),
                data_type: def.tag.data_type.clone(),
            })
            .collect();
        let ctx = self.ctx(c);
        let mut offset = 0;
        for chunk in items.chunks(max) {
            if self.ensure_running().is_err() || d.driver.is_none() {
                errs[offset..].fill(Some(TagError::ShutDown));
                break;
            }
            let Some(drv) = d.driver.as_mut() else { break };
            let res = drv.read(chunk, &ctx);
            let range_ids = offset..offset + chunk.len();
            offset += chunk.len();
            match res {
                Err(e) if e.kind.is_connection_loss() => {
                    for k in range_ids.clone() {
                        errs[k] = Some(TagError::Driver {
                            tag: ids[k].to_string(),
                            error: e.clone(),
                        });
                    }
                    for k in offset..ids.len() {
                        errs[k] = Some(TagError::Driver {
                            tag: ids[k].to_string(),
                            error: e.clone(),
                        });
                    }
                    self.connection_lost(c, d, e);
                    return errs;
                }
                Err(e) => {
                    let q = Quality::bad(reason_for(e.kind));
                    self.update(chunk.iter().map(|it| (it.tag.clone(), None, q)).collect());
                    for k in range_ids {
                        errs[k] = Some(TagError::Driver {
                            tag: ids[k].to_string(),
                            error: e.clone(),
                        });
                    }
                    if e.kind == DriverErrorKind::Cancelled {
                        for k in offset..ids.len() {
                            errs[k] = Some(TagError::Driver {
                                tag: ids[k].to_string(),
                                error: e.clone(),
                            });
                        }
                        return errs;
                    }
                }
                Ok(results) if results.len() != chunk.len() => {
                    let e = DriverError::new(
                        DriverErrorKind::Protocol,
                        format!(
                            "driver returned {} results for {} items",
                            results.len(),
                            chunk.len()
                        ),
                    );
                    let q = Quality::bad(QualityReason::CommunicationError);
                    self.update(chunk.iter().map(|it| (it.tag.clone(), None, q)).collect());
                    for k in range_ids {
                        errs[k] = Some(TagError::Driver {
                            tag: ids[k].to_string(),
                            error: e.clone(),
                        });
                    }
                }
                Ok(results) => {
                    let mut updates = Vec::new();
                    for ((it, r), k) in chunk.iter().zip(results).zip(range_ids) {
                        let def = &self.defs[&it.tag];
                        match r.and_then(|v| {
                            self.types
                                .check(&v, &def.tag.data_type)
                                .map(|()| v)
                                .map_err(|e| {
                                    DriverError::new(DriverErrorKind::TypeMismatch, e.to_string())
                                })
                        }) {
                            Ok(v) => {
                                let q = if out_of_range(def.tag.range.as_ref(), &v).is_some() {
                                    Quality::uncertain(QualityReason::OutOfRange)
                                } else {
                                    Quality::GOOD
                                };
                                updates.push((it.tag.clone(), Some(v), q));
                            }
                            Err(e) => {
                                updates.push((
                                    it.tag.clone(),
                                    None,
                                    Quality::bad(reason_for(e.kind)),
                                ));
                                errs[k] = Some(TagError::Driver {
                                    tag: it.tag.to_string(),
                                    error: e,
                                });
                            }
                        }
                    }
                    self.update(updates);
                }
            }
        }
        errs
    }

    fn read_from_device(&self, ci: usize, ids: &[TagId]) -> Vec<Option<TagError>> {
        let Some(c) = self.conns.get(ci) else {
            return vec![Some(TagError::ShutDown); ids.len()];
        };
        let mut d = lock(&c.driver);
        if self.ensure_running().is_err() {
            return vec![Some(TagError::ShutDown); ids.len()];
        }
        if !self.ensure_connected(c, &mut d, false) {
            return ids
                .iter()
                .map(|t| {
                    Some(TagError::NotConnected {
                        tag: t.to_string(),
                        connection: c.id.to_string(),
                    })
                })
                .collect();
        }
        self.read_locked(c, &mut d, ids)
    }

    /// Validates a write; returns (connection index, item) on success.
    fn prepare_write(&self, r: WriteRequest) -> Result<(Option<usize>, WriteItem), TagError> {
        let tag = r.tag.to_string();
        let d = self.def(&r.tag)?;
        if !d.tag.access.writable() {
            return Err(TagError::AccessDenied {
                tag,
                operation: "write",
            });
        }
        if let Some(reason) = &d.config_error {
            return Err(TagError::Unavailable {
                tag,
                reason: reason.clone(),
            });
        }
        let mt = self
            .types
            .member_type(&d.tag.data_type, &r.member)
            .map_err(|error| TagError::Value {
                tag: tag.clone(),
                error,
            })?;
        let value = self
            .types
            .coerce(r.value, &mt)
            .map_err(|error| TagError::Value {
                tag: tag.clone(),
                error,
            })?;
        if r.member.is_root() {
            if let Some((value, min, max)) = out_of_range(d.tag.range.as_ref(), &value) {
                return Err(TagError::OutOfRange {
                    tag,
                    value,
                    min,
                    max,
                });
            }
        }
        Ok((
            d.conn,
            WriteItem {
                tag: r.tag,
                address: d.tag.address.clone().unwrap_or_default(),
                data_type: d.tag.data_type.clone(),
                member: r.member,
                value,
            },
        ))
    }

    /// Updates cached values after accepted writes (atomic member update
    /// under the store lock).
    fn apply_written(&self, items: &[WriteItem], ok: &[bool], require_running: bool) -> bool {
        let now = self.clock.now();
        let mut values = write(&self.values);
        if require_running && self.ensure_running().is_err() {
            return false;
        }
        let mut events = Vec::new();
        for (it, ok) in items.iter().zip(ok) {
            if !*ok {
                continue;
            }
            let Some(slot) = values.get_mut(&it.tag) else {
                continue;
            };
            let mut next = slot.value.clone();
            match next.get_mut(&it.member) {
                Some(m) => *m = it.value.clone(),
                None => continue,
            }
            // Accepted by the device (or internal): good. For device tags the
            // next poll replaces it with the device's own value.
            let quality = Quality::GOOD;
            let changed = next != slot.value || quality != slot.quality;
            slot.value = next;
            slot.quality = quality;
            slot.timestamp = now;
            if changed {
                slot.sequence = self.sequence.fetch_add(1, Ordering::SeqCst) + 1;
                events.push(Self::snap(&it.tag, slot));
            }
        }
        if events.is_empty() {
            return true;
        }
        let mut subs = lock(&self.subs);
        subs.retain(|_, s| {
            for e in &events {
                let readable = self
                    .defs
                    .get(&e.tag)
                    .is_some_and(|d| d.tag.access.readable());
                if readable
                    && s.filter.as_ref().is_none_or(|f| f.contains(&e.tag))
                    && s.tx
                        .send(TagEvent::Changed {
                            snapshot: e.clone(),
                        })
                        .is_err()
                {
                    return false;
                }
            }
            true
        });
        true
    }

    fn write_to_device(&self, ci: usize, items: Vec<WriteItem>) -> Vec<Result<(), TagError>> {
        let Some(c) = self.conns.get(ci) else {
            return items.iter().map(|_| Err(TagError::ShutDown)).collect();
        };
        let mut d = lock(&c.driver);
        if self.ensure_running().is_err() {
            return vec![Err(TagError::ShutDown); items.len()];
        }
        if !self.ensure_connected(c, &mut d, false) {
            return items
                .iter()
                .map(|it| {
                    Err(TagError::NotConnected {
                        tag: it.tag.to_string(),
                        connection: c.id.to_string(),
                    })
                })
                .collect();
        }
        let caps = d.caps.clone();
        let member_ok = caps.as_ref().is_some_and(|c| c.member_writes);
        let max = caps.as_ref().map_or(1, |c| c.max_items_per_write.max(1));
        let mut results: Vec<Result<(), TagError>> = Vec::with_capacity(items.len());
        let mut sendable = Vec::new();
        let mut slots = Vec::new();
        for it in items {
            if !it.member.is_root() && !member_ok {
                results.push(Err(TagError::Unsupported {
                    tag: it.tag.to_string(),
                    message: "driver cannot write members".into(),
                }));
            } else {
                slots.push(results.len());
                results.push(Ok(()));
                sendable.push(it);
            }
        }
        let ctx = self.ctx(c);
        let mut done = 0;
        for chunk in sendable.chunks(max) {
            if self.ensure_running().is_err() {
                for &s in &slots[done..] {
                    results[s] = Err(TagError::ShutDown);
                }
                break;
            }
            let Some(drv) = d.driver.as_mut() else {
                for &s in &slots[done..] {
                    results[s] = Err(TagError::ShutDown);
                }
                break;
            };
            let res = drv.write(chunk, &ctx);
            let chunk_slots = &slots[done..done + chunk.len()];
            done += chunk.len();
            match res {
                Err(e) => {
                    for (it, &s) in chunk.iter().zip(chunk_slots) {
                        results[s] = Err(TagError::Driver {
                            tag: it.tag.to_string(),
                            error: e.clone(),
                        });
                    }
                    if e.kind.is_connection_loss() {
                        for (k, &s) in slots.iter().enumerate().skip(done) {
                            results[s] = Err(TagError::NotConnected {
                                tag: sendable[k].tag.to_string(),
                                connection: c.id.to_string(),
                            });
                        }
                        self.connection_lost(c, &mut d, e);
                        return results;
                    }
                }
                Ok(r) if r.len() != chunk.len() => {
                    let e = DriverError::new(DriverErrorKind::Protocol, "result count mismatch");
                    for (it, &s) in chunk.iter().zip(chunk_slots) {
                        results[s] = Err(TagError::Driver {
                            tag: it.tag.to_string(),
                            error: e.clone(),
                        });
                    }
                }
                Ok(r) => {
                    let ok: Vec<bool> = r.iter().map(Result::is_ok).collect();
                    self.apply_written(chunk, &ok, false);
                    for ((it, res), &s) in chunk.iter().zip(r).zip(chunk_slots) {
                        if let Err(error) = res {
                            results[s] = Err(TagError::Driver {
                                tag: it.tag.to_string(),
                                error,
                            });
                        }
                    }
                }
            }
        }
        results
    }

    fn scan_loop(self: Arc<Self>, i: usize, max_idle_ms: u64) {
        while self.running.load(Ordering::SeqCst) && !self.cancel.is_cancelled() {
            self.step(i, false);
            let wait_ms = self.next_wake_ms(i).clamp(1, max_idle_ms);
            if self.cancel.wait(Duration::from_millis(wait_ms)) {
                break;
            }
        }
    }

    fn next_wake_ms(&self, i: usize) -> u64 {
        let Some(c) = self.conns.get(i) else { return 1 };
        let now = self.clock.now();
        let mut next = u64::MAX;
        for g in lock(&c.groups).iter() {
            next = next.min(g.next_due.0.saturating_sub(now.0) / 1000);
        }
        // Lock order is driver -> status: take the driver lock first and
        // only then peek at the status (never the other way round).
        let d = lock(&c.driver);
        let connected = matches!(*lock(&c.status), ConnectionStatus::Connected);
        if !connected {
            next = next.min(d.next_connect.0.saturating_sub(now.0) / 1000);
        }
        next
    }
}

/// Data type of a tag member, for callers that build writes.
pub fn member_data_type(
    engine: &TagEngine,
    tag: &TagId,
    member: &MemberPath,
) -> Result<DataType, TagError> {
    let d = engine.inner.def(tag)?;
    engine
        .inner
        .types
        .member_type(&d.tag.data_type, member)
        .map_err(|error| TagError::Value {
            tag: tag.to_string(),
            error,
        })
}
