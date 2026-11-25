//! OPC UA client driver (async-opcua).
//!
//! Source settings (all non-secret; credentials are passed separately at configure time):
//! - `endpoint` – `opc.tcp://host:port[/path]` (required)
//! - `securityPolicy` – `None` (default), `Basic256Sha256`, `Aes128Sha256RsaOaep`, `Aes256Sha256RsaPss`
//! - `securityMode` – `None` (default), `Sign`, `SignAndEncrypt`
//! - `pkiDir` – certificate store directory; required for any policy other than `None`
//! - `trustServerCertificate` – `true` to accept an unknown server certificate (commissioning only)
//! - `publishingIntervalMs` – subscription publishing interval, default 500
//!
//! Addresses are OPC UA node IDs in the standard text form, e.g. `ns=2;s=Line1.Motor.Running`.

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use futures_util::StreamExt;
use opcua::client::{ClientBuilder, DataChangeCallback, IdentityToken, Session, SessionPollResult};
use opcua::crypto::SecurityPolicy;
use opcua::types::{
    AttributeId, BrowseDescription, BrowseDirection, DataValue, MessageSecurityMode,
    MonitoredItemCreateRequest, NodeClass, NodeId, NumericRange, ReadValueId, ReferenceTypeId,
    StatusCode, TimestampsToReturn, UserTokenPolicy, Variant, WriteValue,
};
use tokio::task::JoinHandle;

use super::{setting, Driver, DriverError, DriverEvent, EventSink};
use crate::protocol::{
    BrowseNode, PointValue, Quality, RuntimePoint, RuntimeSource, SourceCredential, SourceState,
    Value, ValueType,
};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_BROWSE_REFERENCES: u32 = 500;

/// The scalar type a node actually holds, learned from the values it reports. Used to write
/// e.g. an `INT` variable to an `Int16` node in the node's own type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NodeKind {
    Boolean,
    SByte,
    Byte,
    Int16,
    UInt16,
    Int32,
    UInt32,
    Int64,
    UInt64,
    Float,
    Double,
    String,
}

pub(crate) fn kind_of(variant: &Variant) -> Option<NodeKind> {
    Some(match variant {
        Variant::Boolean(_) => NodeKind::Boolean,
        Variant::SByte(_) => NodeKind::SByte,
        Variant::Byte(_) => NodeKind::Byte,
        Variant::Int16(_) => NodeKind::Int16,
        Variant::UInt16(_) => NodeKind::UInt16,
        Variant::Int32(_) => NodeKind::Int32,
        Variant::UInt32(_) => NodeKind::UInt32,
        Variant::Int64(_) => NodeKind::Int64,
        Variant::UInt64(_) => NodeKind::UInt64,
        Variant::Float(_) => NodeKind::Float,
        Variant::Double(_) => NodeKind::Double,
        Variant::String(_) => NodeKind::String,
        _ => return None,
    })
}

/// The variable type able to hold every value of a built-in OPC UA data type, if any.
fn suggested_type(data_type: &NodeId) -> Option<ValueType> {
    use opcua::types::Identifier;
    if data_type.namespace != 0 {
        return None;
    }
    let Identifier::Numeric(id) = data_type.identifier else {
        return None;
    };
    Some(match id {
        1 => ValueType::Bool,
        2..=6 => ValueType::Int,
        7 | 8 => ValueType::Long, // UInt32, Int64 (UInt64 can exceed LONG: no suggestion)
        10 => ValueType::Float,
        11 => ValueType::Double,
        12 => ValueType::String,
        _ => return None,
    })
}

/// Converts a device value to the variable's declared type. Never guesses: a value that does
/// not fit is reported as an error, not truncated.
pub(crate) fn from_variant(variant: &Variant, wanted: ValueType) -> Result<Value, &'static str> {
    let integer: Option<i128> = match variant {
        Variant::SByte(v) => Some(*v as i128),
        Variant::Byte(v) => Some(*v as i128),
        Variant::Int16(v) => Some(*v as i128),
        Variant::UInt16(v) => Some(*v as i128),
        Variant::Int32(v) => Some(*v as i128),
        Variant::UInt32(v) => Some(*v as i128),
        Variant::Int64(v) => Some(*v as i128),
        Variant::UInt64(v) => Some(*v as i128),
        _ => None,
    };
    match (wanted, variant) {
        (ValueType::Bool, Variant::Boolean(v)) => Ok(Value::Bool(*v)),
        (ValueType::Int, _) if integer.is_some() => i32::try_from(integer.unwrap())
            .map(Value::Int)
            .map_err(|_| "OutOfRange"),
        (ValueType::Long, _) if integer.is_some() => i64::try_from(integer.unwrap())
            .map(Value::Long)
            .map_err(|_| "OutOfRange"),
        (ValueType::Float, Variant::Float(v)) => Ok(Value::Float(*v)),
        (ValueType::Float, Variant::Double(v)) => {
            if v.is_finite() && v.abs() > f32::MAX as f64 {
                Err("OutOfRange")
            } else {
                Ok(Value::Float(*v as f32))
            }
        }
        (ValueType::Double, Variant::Double(v)) => Ok(Value::Double(*v)),
        (ValueType::Double, Variant::Float(v)) => Ok(Value::Double(*v as f64)),
        (ValueType::String, Variant::String(v)) => {
            Ok(Value::String(v.value().clone().unwrap_or_default()))
        }
        _ => Err("TypeMismatch"),
    }
}

/// Converts a value to write into the node's own scalar type (or the natural OPC UA type when
/// the node's type is not known yet). Refuses anything that would not be represented exactly.
pub(crate) fn to_variant(value: &Value, kind: Option<NodeKind>) -> Result<Variant, String> {
    fn int_to(kind: NodeKind, v: i128) -> Result<Variant, String> {
        let out_of_range = || format!("value {v} does not fit the node's type {kind:?}");
        Ok(match kind {
            NodeKind::SByte => Variant::SByte(i8::try_from(v).map_err(|_| out_of_range())?),
            NodeKind::Byte => Variant::Byte(u8::try_from(v).map_err(|_| out_of_range())?),
            NodeKind::Int16 => Variant::Int16(i16::try_from(v).map_err(|_| out_of_range())?),
            NodeKind::UInt16 => Variant::UInt16(u16::try_from(v).map_err(|_| out_of_range())?),
            NodeKind::Int32 => Variant::Int32(i32::try_from(v).map_err(|_| out_of_range())?),
            NodeKind::UInt32 => Variant::UInt32(u32::try_from(v).map_err(|_| out_of_range())?),
            NodeKind::Int64 => Variant::Int64(i64::try_from(v).map_err(|_| out_of_range())?),
            NodeKind::UInt64 => Variant::UInt64(u64::try_from(v).map_err(|_| out_of_range())?),
            other => {
                return Err(format!(
                    "an integer cannot be written to a node of type {other:?}"
                ))
            }
        })
    }
    match (value, kind) {
        (Value::Bool(v), None | Some(NodeKind::Boolean)) => Ok(Variant::Boolean(*v)),
        (Value::Int(v), None) => Ok(Variant::Int32(*v)),
        (Value::Int(v), Some(kind)) => int_to(kind, *v as i128),
        (Value::Long(v), None) => Ok(Variant::Int64(*v)),
        (Value::Long(v), Some(kind)) => int_to(kind, *v as i128),
        (Value::Float(v), None | Some(NodeKind::Float)) => Ok(Variant::Float(*v)),
        (Value::Float(v), Some(NodeKind::Double)) => Ok(Variant::Double(*v as f64)),
        (Value::Double(v), None | Some(NodeKind::Double)) => Ok(Variant::Double(*v)),
        (Value::Double(v), Some(NodeKind::Float)) => {
            if v.abs() > f32::MAX as f64 {
                Err(format!("value {v} does not fit the node's type Float"))
            } else {
                Ok(Variant::Float(*v as f32))
            }
        }
        (Value::String(v), None | Some(NodeKind::String)) => Ok(Variant::String(v.as_str().into())),
        (value, Some(kind)) => Err(format!(
            "a {:?} value cannot be written to a node of type {kind:?}",
            value.value_type()
        )),
    }
}

fn point_value(point: &RuntimePoint, data: &DataValue) -> PointValue {
    let status = data.status.unwrap_or(StatusCode::Good);
    let timestamp = data
        .source_timestamp
        .as_ref()
        .map(|t| t.as_chrono().timestamp_millis());
    let quality = if status.is_bad() {
        Quality::Bad
    } else if status.is_uncertain() {
        Quality::Uncertain
    } else {
        Quality::Good
    };
    let bad = |text: String| PointValue {
        variable_id: point.variable_id.clone(),
        value: None,
        quality: Quality::Bad,
        status: Some(text),
        source_timestamp_ms: timestamp,
    };
    if quality == Quality::Bad {
        return bad(status.to_string());
    }
    match data.value.as_ref() {
        None | Some(Variant::Empty) => bad("NoValue".into()),
        Some(variant) => match from_variant(variant, point.value_type) {
            Ok(value) => PointValue {
                variable_id: point.variable_id.clone(),
                value: Some(value),
                quality,
                status: (quality != Quality::Good).then(|| status.to_string()),
                source_timestamp_ms: timestamp,
            },
            Err(reason) => bad(reason.into()),
        },
    }
}

/// Where a point lives. `nsu=<namespace URI>;...` addresses are resolved to the server's current
/// namespace index after every (re)connection, because a PLC may number its namespaces
/// differently after a download or firmware update.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Target {
    Fixed(NodeId),
    ByUri { uri: String, identifier: String },
}

pub(crate) fn parse_target(text: &str) -> Result<Target, String> {
    let text = text.trim();
    let invalid = || {
        format!(
            "'{text}' is not an OPC UA node id (e.g. ns=2;s=Name, ns=2;i=1001 or nsu=<namespace URI>;s=Name)"
        )
    };
    if let Some(rest) = text.strip_prefix("nsu=") {
        let (uri, identifier) = rest.split_once(';').ok_or_else(invalid)?;
        if uri.is_empty() {
            return Err(invalid());
        }
        // Check the identifier part with a placeholder namespace index.
        NodeId::from_str(&format!("ns=1;{identifier}")).map_err(|_| invalid())?;
        return Ok(Target::ByUri {
            uri: uri.to_owned(),
            identifier: identifier.to_owned(),
        });
    }
    NodeId::from_str(text)
        .map(Target::Fixed)
        .map_err(|_| invalid())
}

/// Validates an OPC UA source and its addresses without connecting.
pub fn validate(source: &RuntimeSource, points: &[RuntimePoint]) -> Result<(), String> {
    let probe = OpcUaDriver {
        source: source.clone(),
        credential: None,
        shared: Arc::new(Shared::new(
            source.id.clone(),
            Vec::new(),
            crate::driver::EventSink::new(tokio::sync::mpsc::channel(256).0),
        )),
        session: Arc::new(Mutex::new(None)),
        task: None,
        publishing: None,
    };
    probe.plan().map_err(|e| e.0)?;
    for point in points {
        parse_target(&point.address).map_err(|e| format!("mapping '{}': {e}", point.mapping_id))?;
    }
    Ok(())
}

struct Shared {
    source_id: String,
    targets: Vec<(Target, RuntimePoint)>,
    /// Points by resolved node, so one notification updates every variable mapped to that node.
    by_node: std::sync::RwLock<HashMap<NodeId, Vec<RuntimePoint>>>,
    /// Resolved node per mapping, for writes.
    node_of: std::sync::RwLock<HashMap<String, NodeId>>,
    kinds: Mutex<HashMap<NodeId, NodeKind>>,
    /// Variables in use (`None` = all): only their nodes are monitored.
    demand: std::sync::RwLock<Option<BTreeSet<String>>>,
    /// Set when the monitored items do not match the demand (a change while disconnected).
    monitor_stale: std::sync::atomic::AtomicBool,
    /// One (re)subscription at a time.
    monitor_lock: tokio::sync::Mutex<()>,
    sink: EventSink,
}

impl Shared {
    fn new(source_id: String, targets: Vec<(Target, RuntimePoint)>, sink: EventSink) -> Self {
        Self {
            source_id,
            targets,
            by_node: Default::default(),
            node_of: Default::default(),
            kinds: Mutex::new(HashMap::new()),
            demand: Default::default(),
            monitor_stale: Default::default(),
            monitor_lock: Default::default(),
            sink,
        }
    }

    /// Resolves every target against this session. Points whose namespace the server does not
    /// have are reported BAD and left out (their writes are refused).
    async fn resolve(&self, session: &Session) {
        let mut by_node: HashMap<NodeId, Vec<RuntimePoint>> = HashMap::new();
        let mut node_of = HashMap::new();
        let mut unresolved = Vec::new();
        let mut indexes: HashMap<String, Option<u16>> = HashMap::new();
        for (target, point) in &self.targets {
            let node = match target {
                Target::Fixed(node) => Some(node.clone()),
                Target::ByUri { uri, identifier } => {
                    if !indexes.contains_key(uri) {
                        let index = session.get_namespace_index(uri).await.ok();
                        indexes.insert(uri.clone(), index);
                    }
                    indexes[uri]
                        .and_then(|ns| NodeId::from_str(&format!("ns={ns};{identifier}")).ok())
                }
            };
            match node {
                Some(node) => {
                    node_of.insert(point.mapping_id.clone(), node.clone());
                    by_node.entry(node).or_default().push(point.clone());
                }
                None => unresolved.push(point.clone()),
            }
        }
        *self.by_node.write().unwrap() = by_node;
        *self.node_of.write().unwrap() = node_of;
        let values: Vec<PointValue> = unresolved
            .iter()
            .filter(|p| p.readable())
            .map(|p| PointValue {
                variable_id: p.variable_id.clone(),
                value: None,
                quality: Quality::Bad,
                status: Some("NamespaceNotFound".into()),
                source_timestamp_ms: None,
            })
            .collect();
        if !values.is_empty() {
            self.send(DriverEvent::Values(values));
        }
    }

    fn send(&self, event: DriverEvent) {
        let _ = self.sink.send((self.source_id.clone(), event));
    }

    fn on_data(&self, node: &NodeId, data: &DataValue) {
        if let Some(kind) = data.value.as_ref().and_then(kind_of) {
            self.kinds.lock().unwrap().insert(node.clone(), kind);
        }
        let values: Vec<PointValue> = self
            .by_node
            .read()
            .unwrap()
            .get(node)
            .map(|points| {
                points
                    .iter()
                    .filter(|p| p.readable())
                    .map(|p| point_value(p, data))
                    .collect()
            })
            .unwrap_or_default();
        if !values.is_empty() {
            self.send(DriverEvent::Values(values));
        }
    }

    /// Values are unknown while the connection is down; say so rather than show stale data as good.
    fn mark_all_bad(&self, status: &str) {
        let values: Vec<PointValue> = self
            .targets
            .iter()
            .map(|(_, point)| point)
            .filter(|p| p.readable())
            .map(|p| PointValue {
                variable_id: p.variable_id.clone(),
                value: None,
                quality: Quality::Bad,
                status: Some(status.into()),
                source_timestamp_ms: None,
            })
            .collect();
        if !values.is_empty() {
            self.send(DriverEvent::Values(values));
        }
    }
}

pub struct OpcUaDriver {
    source: RuntimeSource,
    credential: Option<SourceCredential>,
    shared: Arc<Shared>,
    /// Set by the connection task once a session exists; used by write and browse.
    session: Arc<Mutex<Option<Arc<Session>>>>,
    task: Option<JoinHandle<()>>,
    publishing: Option<Duration>,
}

/// Everything needed to (re)connect, validated before the connection task starts.
struct ConnectPlan {
    endpoint: String,
    policy: SecurityPolicy,
    mode: MessageSecurityMode,
    pki_dir: PathBuf,
    trust_server: bool,
    publishing: Duration,
    credential: Option<SourceCredential>,
}

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const RETRY_INITIAL: Duration = Duration::from_secs(1);
const RETRY_MAX: Duration = Duration::from_secs(15);

impl OpcUaDriver {
    pub fn new(
        source: RuntimeSource,
        points: Vec<RuntimePoint>,
        credential: Option<SourceCredential>,
        sink: EventSink,
    ) -> Result<Self, DriverError> {
        let mut targets = Vec::new();
        for point in points {
            let target = parse_target(&point.address)
                .map_err(|e| DriverError(format!("mapping '{}': {e}", point.mapping_id)))?;
            targets.push((target, point));
        }
        let shared = Arc::new(Shared::new(source.id.clone(), targets, sink));
        Ok(Self {
            source,
            credential,
            shared,
            session: Arc::new(Mutex::new(None)),
            task: None,
            publishing: None,
        })
    }

    fn security(&self) -> Result<(SecurityPolicy, MessageSecurityMode), DriverError> {
        let policy_name = setting(&self.source.settings, "securityPolicy").unwrap_or("None");
        let policy = match policy_name {
            "None" => SecurityPolicy::None,
            "Basic256Sha256" => SecurityPolicy::Basic256Sha256,
            "Aes128Sha256RsaOaep" => SecurityPolicy::Aes128Sha256RsaOaep,
            "Aes256Sha256RsaPss" => SecurityPolicy::Aes256Sha256RsaPss,
            other => {
                return Err(DriverError(format!(
                    "securityPolicy '{other}' is not supported (None, Basic256Sha256, Aes128Sha256RsaOaep, Aes256Sha256RsaPss)"
                )))
            }
        };
        let mode = match setting(&self.source.settings, "securityMode").unwrap_or("None") {
            "None" => MessageSecurityMode::None,
            "Sign" => MessageSecurityMode::Sign,
            "SignAndEncrypt" => MessageSecurityMode::SignAndEncrypt,
            other => {
                return Err(DriverError(format!(
                    "securityMode '{other}' is not supported (None, Sign, SignAndEncrypt)"
                )))
            }
        };
        if (policy == SecurityPolicy::None) != (mode == MessageSecurityMode::None) {
            return Err(DriverError(
                "securityPolicy and securityMode must both be None or both be set".into(),
            ));
        }
        Ok((policy, mode))
    }

    fn plan(&self) -> Result<ConnectPlan, DriverError> {
        let endpoint = setting(&self.source.settings, "endpoint")
            .ok_or_else(|| DriverError("the source has no 'endpoint' setting".into()))?
            .to_owned();
        if !endpoint.starts_with("opc.tcp://") {
            return Err(DriverError(format!(
                "endpoint '{endpoint}' must start with opc.tcp://"
            )));
        }
        let (policy, mode) = self.security()?;
        let pki_dir = match setting(&self.source.settings, "pkiDir") {
            Some(dir) => PathBuf::from(dir),
            None if policy == SecurityPolicy::None => std::env::temp_dir().join("hmi-engine-pki"),
            None => {
                return Err(DriverError(
                    "a 'pkiDir' setting is required when a security policy is used".into(),
                ))
            }
        };
        Ok(ConnectPlan {
            endpoint,
            policy,
            mode,
            pki_dir,
            trust_server: setting(&self.source.settings, "trustServerCertificate") == Some("true"),
            publishing: Duration::from_millis(
                setting(&self.source.settings, "publishingIntervalMs")
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(500)
                    .clamp(50, 60_000),
            ),
            credential: self.credential.clone(),
        })
    }

    /// Connects (retrying with back-off until it succeeds or the task is aborted), subscribes,
    /// then follows the session until it ends. Runs on its own task so a slow or absent device
    /// never blocks the engine.
    async fn run(plan: ConnectPlan, shared: Arc<Shared>, slot: Arc<Mutex<Option<Arc<Session>>>>) {
        let mut delay = RETRY_INITIAL;
        loop {
            shared.send(DriverEvent::State(SourceState::Connecting, None));
            match tokio::time::timeout(CONNECT_TIMEOUT, Self::connect_once(&plan)).await {
                Ok(Ok((session, event_loop))) => {
                    *slot.lock().unwrap() = Some(session.clone());
                    Self::follow(&plan, shared.clone(), session, event_loop).await;
                    *slot.lock().unwrap() = None;
                    delay = RETRY_INITIAL;
                }
                Ok(Err(message)) => {
                    shared.send(DriverEvent::State(SourceState::Failed, Some(message)))
                }
                Err(_) => shared.send(DriverEvent::State(
                    SourceState::Failed,
                    Some(format!(
                        "no answer from {} within {}s",
                        plan.endpoint,
                        CONNECT_TIMEOUT.as_secs()
                    )),
                )),
            }
            shared.mark_all_bad("NotConnected");
            tokio::time::sleep(delay).await;
            delay = (delay * 2).min(RETRY_MAX);
        }
    }

    async fn connect_once(
        plan: &ConnectPlan,
    ) -> Result<
        (
            Arc<Session>,
            opcua::client::SessionEventLoop<opcua::client::transport::TcpConnector>,
        ),
        String,
    > {
        let mut client = ClientBuilder::new()
            .application_name("HMI Studio Engine")
            .application_uri("urn:hmistudio:engine")
            .product_uri("urn:hmistudio:engine")
            .pki_dir(plan.pki_dir.clone())
            .create_sample_keypair(plan.policy != SecurityPolicy::None)
            .trust_server_certs(plan.trust_server)
            // Reconnects within a session are the library's job; after it gives up, `run` starts over.
            .session_retry_limit(5)
            .session_retry_initial(RETRY_INITIAL)
            .session_retry_max(RETRY_MAX)
            .request_timeout(REQUEST_TIMEOUT)
            .client()
            .map_err(|errors| {
                format!("invalid OPC UA client configuration: {}", errors.join("; "))
            })?;
        let identity = match &plan.credential {
            Some(credential) => IdentityToken::new_user_name(
                credential.username.clone(),
                credential.password.clone(),
            ),
            None => IdentityToken::Anonymous,
        };
        client
            .connect_to_matching_endpoint(
                (
                    plan.endpoint.as_str(),
                    plan.policy.to_str(),
                    plan.mode,
                    UserTokenPolicy::anonymous(),
                ),
                identity,
            )
            .await
            .map_err(|e| format!("cannot connect to {}: {e}", plan.endpoint))
    }

    /// Polls the session's event loop, reporting state and subscribing after the first connect.
    async fn follow(
        plan: &ConnectPlan,
        shared: Arc<Shared>,
        session: Arc<Session>,
        event_loop: opcua::client::SessionEventLoop<opcua::client::transport::TcpConnector>,
    ) {
        let stream = event_loop.enter();
        tokio::pin!(stream);
        let mut subscribed = false;
        while let Some(event) = stream.next().await {
            match event {
                Ok(SessionPollResult::Reconnected(_)) => {
                    if !subscribed {
                        // Subscribing needs the event loop polled, so it runs beside it.
                        let (shared, session, publishing) =
                            (shared.clone(), session.clone(), plan.publishing);
                        tokio::spawn(async move {
                            match Self::subscribe(shared.clone(), session, publishing).await {
                                Ok(()) => {
                                    shared.send(DriverEvent::State(SourceState::Connected, None))
                                }
                                Err(status) => {
                                    shared.send(DriverEvent::State(
                                        SourceState::Failed,
                                        Some(format!("cannot subscribe: {status}")),
                                    ));
                                    shared.mark_all_bad("SubscribeFailed");
                                }
                            }
                        });
                        subscribed = true;
                    } else {
                        shared.send(DriverEvent::State(SourceState::Connected, None));
                        if shared.monitor_stale.load(Ordering::SeqCst) {
                            let (shared, session, publishing) =
                                (shared.clone(), session.clone(), plan.publishing);
                            tokio::spawn(async move {
                                let _ = Self::monitor(&shared, &session, publishing).await;
                            });
                        }
                    }
                }
                Ok(SessionPollResult::ConnectionLost(status)) => {
                    shared.send(DriverEvent::State(
                        SourceState::Connecting,
                        Some(format!("connection lost: {status}")),
                    ));
                    shared.mark_all_bad("ConnectionLost");
                }
                Ok(SessionPollResult::ReconnectFailed(status)) => {
                    shared.send(DriverEvent::State(
                        SourceState::Connecting,
                        Some(format!("reconnect failed: {status}")),
                    ));
                }
                Ok(_) => {}
                Err(status) => {
                    shared.send(DriverEvent::State(
                        SourceState::Failed,
                        Some(format!("session ended: {status}")),
                    ));
                    return;
                }
            }
        }
    }

    fn current_session(&self) -> Result<Arc<Session>, DriverError> {
        self.session
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| DriverError("not connected".into()))
    }

    async fn subscribe(
        shared: Arc<Shared>,
        session: Arc<Session>,
        publishing: Duration,
    ) -> Result<(), StatusCode> {
        shared.resolve(&session).await;
        let nodes: Vec<NodeId> = shared.by_node.read().unwrap().keys().cloned().collect();
        if nodes.is_empty() {
            return Ok(());
        }
        // One read first: gives every point an initial value and tells us each node's type,
        // including write-only points that are never subscribed.
        let to_read: Vec<ReadValueId> = nodes.iter().cloned().map(Into::into).collect();
        let initial = session
            .read(&to_read, TimestampsToReturn::Source, 0.0)
            .await
            .map_err(|e| e.status())?;
        for (node, data) in nodes.iter().zip(initial.iter()) {
            shared.on_data(node, data);
        }

        Self::monitor(&shared, &session, publishing).await
    }

    /// (Re)creates the subscription so that exactly the nodes of the variables in use are
    /// monitored. A newly monitored node reports its current value at once.
    async fn monitor(
        shared: &Arc<Shared>,
        session: &Arc<Session>,
        publishing: Duration,
    ) -> Result<(), StatusCode> {
        let _one_at_a_time = shared.monitor_lock.lock().await;
        shared.monitor_stale.store(true, Ordering::SeqCst);
        let existing = session
            .subscription_state()
            .lock()
            .subscription_ids()
            .unwrap_or_default();
        for id in existing {
            session
                .delete_subscription(id)
                .await
                .map_err(|e| e.status())?;
        }
        let monitored: Vec<MonitoredItemCreateRequest> = {
            let demand = shared.demand.read().unwrap();
            shared
                .by_node
                .read()
                .unwrap()
                .iter()
                .filter(|(_, points)| {
                    points.iter().any(|p| {
                        p.readable()
                            && demand
                                .as_ref()
                                .is_none_or(|wanted| wanted.contains(&p.variable_id))
                    })
                })
                .map(|(node, _)| node.clone().into())
                .collect()
        };
        if monitored.is_empty() {
            shared.monitor_stale.store(false, Ordering::SeqCst);
            return Ok(());
        }
        let callback_shared = shared.clone();
        let subscription = session
            .create_subscription(
                publishing,
                60,
                10,
                0,
                0,
                true,
                DataChangeCallback::new(move |data, item| {
                    callback_shared.on_data(&item.item_to_monitor().node_id, &data);
                }),
            )
            .await
            .map_err(|e| e.status())?;
        let results = session
            .create_monitored_items(subscription, TimestampsToReturn::Source, monitored)
            .await
            .map_err(|e| e.status())?;
        // A node the server refuses to monitor must not look like a silent, healthy point.
        for result in results {
            if result.result.status_code.is_bad() {
                let node = result.item_to_monitor.node_id.clone();
                let mut data = DataValue::null();
                data.status = Some(result.result.status_code);
                shared.on_data(&node, &data);
            }
        }
        shared.monitor_stale.store(false, Ordering::SeqCst);
        Ok(())
    }
}

// Once handed to the transport, a timeout, service error, malformed response or
// ambiguous operation status does not prove the write failed. Only a definite
// per-node refusal can rule out application. Good (exact) confirms application;
// e.g. GoodCompletesAsynchronously does not.
async fn await_write_reply<E: std::fmt::Display>(
    timeout: Duration,
    reply: impl std::future::Future<Output = Result<Vec<StatusCode>, E>>,
) -> Result<(), DriverError> {
    let results = tokio::time::timeout(timeout, reply)
        .await
        .map_err(|_| DriverError::outcome_unknown("the device did not answer the write in time"))?
        .map_err(|e| DriverError::outcome_unknown(format!("write failed: {e}")))?;
    match results.as_slice() {
        [status] if *status == StatusCode::Good => Ok(()),
        [status]
            if matches!(
                *status,
                StatusCode::BadUserAccessDenied
                    | StatusCode::BadNotWritable
                    | StatusCode::BadTypeMismatch
                    | StatusCode::BadNodeIdUnknown
                    | StatusCode::BadAttributeIdInvalid
                    | StatusCode::BadOutOfRange
            ) =>
        {
            Err(DriverError(format!(
                "the device rejected the write: {status}"
            )))
        }
        [status] => Err(DriverError::outcome_unknown(format!(
            "write returned ambiguous status: {status}"
        ))),
        _ => Err(DriverError::outcome_unknown(
            "the device returned an unexpected number of write results",
        )),
    }
}

#[async_trait]
impl Driver for OpcUaDriver {
    /// Validates the settings and starts the connection task. Returns at once: connecting,
    /// retrying and reconnecting are reported as source states, never by blocking the engine.
    async fn start(&mut self) -> Result<(), DriverError> {
        let plan = self.plan()?;
        self.publishing = Some(plan.publishing);
        self.task = Some(tokio::spawn(Self::run(
            plan,
            self.shared.clone(),
            self.session.clone(),
        )));
        Ok(())
    }

    async fn write(&mut self, point: &RuntimePoint, value: &Value) -> Result<(), DriverError> {
        let session = self
            .current_session()
            .map_err(|e| DriverError::not_transmitted(e.to_string()))?;
        let node = self
            .shared
            .node_of
            .read()
            .unwrap()
            .get(&point.mapping_id)
            .cloned()
            .ok_or_else(|| {
                DriverError::not_transmitted(
                    "the address is not resolved on this server (unknown namespace?)",
                )
            })?;
        let kind = self.shared.kinds.lock().unwrap().get(&node).copied();
        let variant = to_variant(value, kind).map_err(DriverError::not_transmitted)?;
        let request = WriteValue {
            node_id: node,
            attribute_id: AttributeId::Value as u32,
            index_range: NumericRange::None,
            value: DataValue::value_only(variant),
        };
        await_write_reply(REQUEST_TIMEOUT, session.write(&[request])).await
    }

    async fn set_demand(&mut self, demand: Option<&BTreeSet<String>>) {
        *self.shared.demand.write().unwrap() = demand.cloned();
        self.shared.monitor_stale.store(true, Ordering::SeqCst);
        // Not connected (or the change fails): applied when the connection is (back) up.
        let Ok(session) = self.current_session() else {
            return;
        };
        let Some(publishing) = self.publishing else {
            return;
        };
        let _ = tokio::time::timeout(
            REQUEST_TIMEOUT,
            Self::monitor(&self.shared, &session, publishing),
        )
        .await;
    }

    async fn browse(&mut self, node_id: Option<&str>) -> Result<Vec<BrowseNode>, DriverError> {
        let session = self.current_session()?;
        let node = match node_id {
            Some(text) => NodeId::from_str(text.trim())
                .map_err(|_| DriverError(format!("'{text}' is not a node id")))?,
            None => NodeId::objects_folder_id(),
        };
        let description = BrowseDescription {
            node_id: node,
            browse_direction: BrowseDirection::Forward,
            reference_type_id: ReferenceTypeId::HierarchicalReferences.into(),
            include_subtypes: true,
            node_class_mask: 0,
            result_mask: 0x3f,
        };
        let results = tokio::time::timeout(
            REQUEST_TIMEOUT,
            session.browse(&[description], MAX_BROWSE_REFERENCES, None),
        )
        .await
        .map_err(|_| DriverError("the device did not answer the browse in time".into()))?
        .map_err(|e| DriverError(format!("browse failed: {e}")))?;
        let references = results
            .into_iter()
            .next()
            .and_then(|r| r.references)
            .unwrap_or_default();
        // Details of the variables, in one read: data type, array-ness and write permission.
        let variables: Vec<NodeId> = references
            .iter()
            .filter(|r| r.node_class == NodeClass::Variable)
            .map(|r| r.node_id.node_id.clone())
            .collect();
        let mut to_read: Vec<ReadValueId> = vec![ReadValueId {
            node_id: NodeId::new(0, 2255u32), // Server.NamespaceArray
            attribute_id: AttributeId::Value as u32,
            ..Default::default()
        }];
        for node in &variables {
            for attribute in [
                AttributeId::DataType,
                AttributeId::ValueRank,
                AttributeId::UserAccessLevel,
            ] {
                to_read.push(ReadValueId {
                    node_id: node.clone(),
                    attribute_id: attribute as u32,
                    ..Default::default()
                });
            }
        }
        // Best effort: a server that refuses these reads still gets a plain listing.
        let details = tokio::time::timeout(
            REQUEST_TIMEOUT,
            session.read(&to_read, TimestampsToReturn::Neither, 0.0),
        )
        .await
        .ok()
        .and_then(Result::ok)
        .unwrap_or_default();
        let namespaces: Vec<String> = match details.first().and_then(|d| d.value.as_ref()) {
            Some(Variant::Array(array)) => array
                .values
                .iter()
                .map(|v| match v {
                    Variant::String(s) => s.value().clone().unwrap_or_default(),
                    _ => String::new(),
                })
                .collect(),
            _ => Vec::new(),
        };
        let mut facts: HashMap<NodeId, (Option<ValueType>, Option<bool>)> = HashMap::new();
        for (index, node) in variables.iter().enumerate() {
            let at = |offset: usize| {
                details
                    .get(1 + index * 3 + offset)
                    .and_then(|d| d.value.as_ref())
            };
            let scalar = matches!(at(1), Some(Variant::Int32(rank)) if *rank < 0);
            let value_type = match at(0) {
                Some(Variant::NodeId(data_type)) if scalar => suggested_type(data_type),
                _ => None,
            };
            let writable = match at(2) {
                Some(Variant::Byte(level)) => Some(level & 0x02 != 0),
                _ => None,
            };
            facts.insert(node.clone(), (value_type, writable));
        }
        Ok(references
            .into_iter()
            .map(|reference| {
                let node = &reference.node_id.node_id;
                let (value_type, writable) = facts.get(node).copied().unwrap_or((None, None));
                let stable_node_id = namespaces
                    .get(node.namespace as usize)
                    .filter(|uri| node.namespace != 0 && !uri.is_empty() && !uri.contains(';'))
                    .map(|uri| {
                        let text = node.to_string();
                        let identifier = text.split_once(';').map_or(text.as_str(), |(_, id)| id);
                        format!("nsu={uri};{identifier}")
                    });
                BrowseNode {
                    value_type,
                    writable,
                    stable_node_id,
                    node_id: reference.node_id.node_id.to_string(),
                    display_name: reference
                        .display_name
                        .text
                        .value()
                        .clone()
                        .unwrap_or_default(),
                    node_class: match reference.node_class {
                        NodeClass::Variable => "VARIABLE",
                        NodeClass::Object => "OBJECT",
                        _ => "OTHER",
                    }
                    .into(),
                    // Structured variables expose their members as children.
                    has_children: reference.node_class == NodeClass::Object
                        || (reference.node_class == NodeClass::Variable && value_type.is_none()),
                }
            })
            .collect())
    }

    async fn stop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
            let _ = task.await;
        }
        let session = self.session.lock().unwrap().take();
        if let Some(session) = session {
            let _ = tokio::time::timeout(REQUEST_TIMEOUT, session.disconnect()).await;
        }
        self.shared
            .send(DriverEvent::State(SourceState::Disconnected, None));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn write_replies_distinguish_definite_and_unknown_outcomes() {
        let bound = Duration::from_millis(20);
        for statuses in [
            vec![],
            vec![StatusCode::Good, StatusCode::Good],
            vec![StatusCode::BadTimeout],
            vec![StatusCode::GoodCompletesAsynchronously],
        ] {
            let error = await_write_reply(bound, async { Ok::<_, &str>(statuses) })
                .await
                .unwrap_err();
            assert!(error.is_outcome_unknown(), "{error}");
        }
        let transport =
            await_write_reply(bound, async { Err::<Vec<StatusCode>, _>("transport lost") })
                .await
                .unwrap_err();
        assert!(transport.is_outcome_unknown());
        let timeout = await_write_reply(
            bound,
            std::future::pending::<Result<Vec<StatusCode>, &str>>(),
        )
        .await
        .unwrap_err();
        assert!(timeout.is_outcome_unknown());
        let refused = await_write_reply(bound, async {
            Ok::<_, &str>(vec![StatusCode::BadUserAccessDenied])
        })
        .await
        .unwrap_err();
        assert!(!refused.is_outcome_unknown());
        assert!(
            await_write_reply(bound, async { Ok::<_, &str>(vec![StatusCode::Good]) })
                .await
                .is_ok()
        );
    }

    #[test]
    fn device_values_convert_only_when_they_fit() {
        assert_eq!(
            from_variant(&Variant::Int16(-5), ValueType::Int),
            Ok(Value::Int(-5))
        );
        assert_eq!(
            from_variant(&Variant::UInt32(u32::MAX), ValueType::Int),
            Err("OutOfRange")
        );
        assert_eq!(
            from_variant(&Variant::UInt32(u32::MAX), ValueType::Long),
            Ok(Value::Long(u32::MAX as i64))
        );
        assert_eq!(
            from_variant(&Variant::UInt64(u64::MAX), ValueType::Long),
            Err("OutOfRange")
        );
        assert_eq!(
            from_variant(&Variant::Double(1.5), ValueType::Float),
            Ok(Value::Float(1.5))
        );
        assert_eq!(
            from_variant(&Variant::Double(1e300), ValueType::Float),
            Err("OutOfRange")
        );
        assert_eq!(
            from_variant(&Variant::Float(2.5), ValueType::Double),
            Ok(Value::Double(2.5))
        );
        assert_eq!(
            from_variant(&Variant::Boolean(true), ValueType::Int),
            Err("TypeMismatch")
        );
        assert_eq!(
            from_variant(&Variant::Int32(1), ValueType::Bool),
            Err("TypeMismatch")
        );
        assert_eq!(
            from_variant(&Variant::Double(1.0), ValueType::Int),
            Err("TypeMismatch")
        );
    }

    #[test]
    fn writes_use_the_nodes_own_type_and_never_truncate() {
        assert_eq!(
            to_variant(&Value::Int(7), Some(NodeKind::Int16)),
            Ok(Variant::Int16(7))
        );
        assert!(to_variant(&Value::Int(70_000), Some(NodeKind::Int16)).is_err());
        assert!(to_variant(&Value::Int(-1), Some(NodeKind::UInt16)).is_err());
        assert_eq!(to_variant(&Value::Int(7), None), Ok(Variant::Int32(7)));
        assert_eq!(
            to_variant(&Value::Long(7), Some(NodeKind::UInt64)),
            Ok(Variant::UInt64(7))
        );
        assert_eq!(
            to_variant(&Value::Float(1.5), Some(NodeKind::Double)),
            Ok(Variant::Double(1.5))
        );
        assert!(to_variant(&Value::Double(1e300), Some(NodeKind::Float)).is_err());
        assert!(to_variant(&Value::Bool(true), Some(NodeKind::Int32)).is_err());
        assert!(to_variant(&Value::Int(1), Some(NodeKind::Boolean)).is_err());
        assert!(to_variant(&Value::String("x".into()), Some(NodeKind::Double)).is_err());
    }
}
