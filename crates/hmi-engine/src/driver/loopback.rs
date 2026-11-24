//! In-memory driver with no I/O at all. Each address is a named cell: readable points report
//! the cell's value, writes store into it. Used for tests, demos and commissioning the write
//! path without a device.

use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;

use super::{Driver, DriverError, DriverEvent, EventSink};
use crate::protocol::{
    BrowseNode, PointValue, Quality, RuntimePoint, RuntimeSource, SourceState, Value, ValueType,
};

pub struct LoopbackDriver {
    source: RuntimeSource,
    points: Vec<RuntimePoint>,
    cells: BTreeMap<String, Value>,
    sink: EventSink,
    started: bool,
}

impl LoopbackDriver {
    pub fn new(source: RuntimeSource, points: Vec<RuntimePoint>, sink: EventSink) -> Self {
        Self {
            source,
            points,
            cells: BTreeMap::new(),
            sink,
            started: false,
        }
    }

    fn send(&self, event: DriverEvent) {
        let _ = self.sink.send((self.source.id.clone(), event));
    }

    fn zero(value_type: ValueType) -> Value {
        match value_type {
            ValueType::Bool => Value::Bool(false),
            ValueType::Int => Value::Int(0),
            ValueType::Long => Value::Long(0),
            ValueType::Float => Value::Float(0.0),
            ValueType::Double => Value::Double(0.0),
            ValueType::String => Value::String(String::new()),
        }
    }

    fn report(&self, address: &str) {
        let Some(value) = self.cells.get(address) else {
            return;
        };
        let values: Vec<PointValue> = self
            .points
            .iter()
            .filter(|p| p.readable() && p.address == address)
            .map(|p| {
                if p.value_type == value.value_type() {
                    PointValue {
                        variable_id: p.variable_id.clone(),
                        value: Some(value.clone()),
                        quality: Quality::Good,
                        status: None,
                        source_timestamp_ms: None,
                    }
                } else {
                    PointValue {
                        variable_id: p.variable_id.clone(),
                        value: None,
                        quality: Quality::Bad,
                        status: Some("TypeMismatch".into()),
                        source_timestamp_ms: None,
                    }
                }
            })
            .collect();
        if !values.is_empty() {
            self.send(DriverEvent::Values(values));
        }
    }
}

#[async_trait]
impl Driver for LoopbackDriver {
    async fn start(&mut self) -> Result<(), DriverError> {
        // `fail=true` lets tests exercise a source that cannot connect.
        if super::setting(&self.source.settings, "fail") == Some("true") {
            self.send(DriverEvent::State(
                SourceState::Failed,
                Some("loopback configured to fail".into()),
            ));
            return Err(DriverError("loopback configured to fail".into()));
        }
        self.send(DriverEvent::State(SourceState::Connecting, None));
        for point in &self.points {
            self.cells
                .entry(point.address.clone())
                .or_insert_with(|| Self::zero(point.value_type));
        }
        self.started = true;
        self.send(DriverEvent::State(SourceState::Connected, None));
        let addresses: Vec<String> = self.cells.keys().cloned().collect();
        for address in addresses {
            self.report(&address);
        }
        Ok(())
    }

    async fn write(&mut self, point: &RuntimePoint, value: &Value) -> Result<(), DriverError> {
        if !self.started {
            return Err(DriverError("not connected".into()));
        }
        if super::setting(&self.source.settings, "rejectWrites") == Some("true") {
            return Err(DriverError(
                "device rejected the write (BadUserAccessDenied)".into(),
            ));
        }
        self.cells.insert(point.address.clone(), value.clone());
        self.report(&point.address);
        Ok(())
    }

    async fn set_demand(&mut self, _demand: Option<&BTreeSet<String>>) {
        // Nothing to save: report everything again and let the engine keep what is in use.
        if self.started {
            let addresses: Vec<String> = self.cells.keys().cloned().collect();
            for address in addresses {
                self.report(&address);
            }
        }
    }

    async fn browse(&mut self, node_id: Option<&str>) -> Result<Vec<BrowseNode>, DriverError> {
        if node_id.is_some() {
            return Ok(Vec::new());
        }
        Ok(self
            .cells
            .keys()
            .map(|address| BrowseNode {
                node_id: address.clone(),
                display_name: address.clone(),
                node_class: "VARIABLE".into(),
                has_children: false,
                value_type: None,
                writable: None,
                stable_node_id: None,
            })
            .collect())
    }

    async fn stop(&mut self) {
        if self.started {
            self.started = false;
            self.send(DriverEvent::State(SourceState::Disconnected, None));
        }
    }
}
