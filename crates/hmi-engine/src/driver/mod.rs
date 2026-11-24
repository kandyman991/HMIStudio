//! Driver contract. A driver talks one protocol to one source; it sees addresses and typed
//! values, never screens, components or the project.

use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use tokio::sync::mpsc::UnboundedSender;

use crate::protocol::{
    BrowseNode, PointValue, RuntimePoint, RuntimeSource, SourceCredential, SourceState, Value,
};

pub mod loopback;
pub mod opcua;
pub mod s7;
pub mod s7_address;

/// Names of the drivers built into this engine.
pub const DRIVERS: &[&str] = &["loopback", "opcua", "s7"];

#[derive(Debug)]
pub struct DriverError(pub String);

impl std::fmt::Display for DriverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// What a driver reports back, tagged with its source by the engine.
#[derive(Debug)]
pub enum DriverEvent {
    State(SourceState, Option<String>),
    Values(Vec<PointValue>),
}

pub type EventSink = UnboundedSender<(String, DriverEvent)>;

#[async_trait]
pub trait Driver: Send {
    /// Starts connecting and reporting the readable points. Must return promptly: connecting,
    /// retrying and reconnecting happen in the background and are reported to the sink as
    /// source states. An error means the source's configuration is unusable.
    async fn start(&mut self) -> Result<(), DriverError>;

    // Note: `write` and `browse` are awaited by the engine in request order, bounded by the
    // driver's request timeout. Requests after them (including `disarmWrites`) wait at most
    // that long; a write already sent to a device cannot be recalled anyway.

    /// Writes one value. Called only after the engine's write guard has passed.
    async fn write(&mut self, point: &RuntimePoint, value: &Value) -> Result<(), DriverError>;

    /// Tells the driver which variables are in use (`None` = all of them). It should stop
    /// reading the others, and must report the current value of every variable that has just
    /// come into use, even if it did not change. The engine itself never forwards values of
    /// variables that are not in use, so a driver that keeps reading everything is only slower.
    async fn set_demand(&mut self, demand: Option<&BTreeSet<String>>);

    /// Lists child nodes; `None` means the driver's root. Read-only.
    async fn browse(&mut self, node_id: Option<&str>) -> Result<Vec<BrowseNode>, DriverError>;

    /// Disconnects. Must be safe to call in any state.
    async fn stop(&mut self);
}

pub fn create(
    source: &RuntimeSource,
    points: Vec<RuntimePoint>,
    credential: Option<SourceCredential>,
    sink: EventSink,
) -> Result<Box<dyn Driver>, DriverError> {
    match source.driver.as_str() {
        "loopback" => Ok(Box::new(loopback::LoopbackDriver::new(
            source.clone(),
            points,
            sink,
        ))),
        "opcua" => Ok(Box::new(opcua::OpcUaDriver::new(
            source.clone(),
            points,
            credential,
            sink,
        )?)),
        "s7" => {
            if credential.is_some() {
                return Err(DriverError(
                    "the S7 protocol has no user credentials; remove them for this source".into(),
                ));
            }
            Ok(Box::new(s7::S7Driver::new(source.clone(), points, sink)?))
        }
        other => Err(DriverError(format!("unknown driver '{other}'"))),
    }
}

/// Driver-specific checks of a source and its points, run at `configure` time so a bad address
/// is refused before anything connects.
pub fn validate(source: &RuntimeSource, points: &[RuntimePoint]) -> Result<(), String> {
    match source.driver.as_str() {
        "s7" => s7::validate(source, points),
        "opcua" => opcua::validate(source, points),
        _ => Ok(()),
    }
}

pub(crate) fn setting<'a>(settings: &'a BTreeMap<String, String>, key: &str) -> Option<&'a str> {
    settings
        .get(key)
        .map(String::as_str)
        .filter(|v| !v.trim().is_empty())
}
