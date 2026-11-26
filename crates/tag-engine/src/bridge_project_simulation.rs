//! Compile canonical tags into a separate, host-selected loopback process.
use crate::bridge_dispatch::{Message, Quality, SourceState};
use crate::bridge_supervisor::Supervisor;
use crate::engine_driver::{decode, encode, wire_type};
use crate::{CancelToken, TagValue, TypeContext};
use hmi_project_model::{
    model::{v2::CommandKind, DataType, TagAccess, ValueRange},
    validate, Project,
};
use serde::Serialize;
use serde_json::{json, Value as Json};
use std::{
    collections::BTreeMap,
    path::Path,
    time::{Duration, Instant},
};

const SOURCE: &str = "project-memory";
const ACTOR: &str = "studio-simulation";
const TIMEOUT: Duration = Duration::from_secs(5);
// The engine and isolated supervisor each have a 32-frame transport queue.
// Initial traffic includes two source events and an initial sample per readable tag.
const MAX_POINTS: usize = 24;
const MAX_INPUT_BYTES: usize = 4096;

struct Point {
    name: String,
    kind: DataType,
    access: TagAccess,
    pulse_ms: Option<u32>,
    confirm: bool,
    range: Option<ValueRange>,
    limits: Limits,
    value: Option<String>,
}

impl Point {
    fn check_write_value(&self, value: &TagValue) -> Result<(), String> {
        if let Some(range) = &self.range {
            // Keep the integer on its exact side of the comparison. The
            // engineering range is declared with float bounds.
            let outside = if let Some(integer) = value.as_i128() {
                integer < range.min.ceil() as i128 || integer > range.max.floor() as i128
            } else {
                value
                    .as_f64()
                    .is_some_and(|number| number < range.min || number > range.max)
            };
            if outside {
                return Err("Value is outside the tag's engineering range".into());
            }
        }
        self.limits.check(value)
    }
}

enum Limits {
    None,
    Integer {
        min: Option<i128>,
        max: Option<i128>,
        step: Option<i128>,
    },
    Float {
        min: Option<f64>,
        max: Option<f64>,
    },
}

impl Limits {
    fn check(&self, value: &TagValue) -> Result<(), String> {
        let allowed = match self {
            Self::None => true,
            Self::Integer { min, max, step } => value.as_i128().is_some_and(|value| {
                min.is_none_or(|bound| value >= bound)
                    && max.is_none_or(|bound| value <= bound)
                    && step.is_none_or(|step| (value - min.unwrap_or(0)) % step == 0)
            }),
            Self::Float { min, max } => {
                // Use the actual process value after narrowing to Float32.
                let number = match value {
                    TagValue::Float32(value) => Some(f64::from(*value)),
                    TagValue::Float64(value) => Some(*value),
                    _ => None,
                };
                number.is_some_and(|value| {
                    min.is_none_or(|bound| value >= bound) && max.is_none_or(|bound| value <= bound)
                })
            }
        };
        allowed
            .then_some(())
            .ok_or_else(|| "Value is outside the declared limits or step".into())
    }
}

/// Host-visible tag state. Numeric values travel as JSON text to preserve wide integers.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SimulationPoint {
    pub id: String,
    pub name: String,
    pub value: Option<String>,
    pub writable: bool,
    pub momentary: bool,
}

/// Owns one child with a fixed canonical configuration. A new project always
/// gets a new process, and no transport or configuration API leaves this type.
pub struct ProjectSimulation {
    child: Supervisor,
    points: BTreeMap<String, Point>,
    connected: bool,
}

struct Configuration {
    request: Json,
    points: BTreeMap<String, Point>,
}

impl Configuration {
    fn compile(project: &Project) -> Result<Self, String> {
        if project.tags.is_empty() || project.tags.len() > MAX_POINTS {
            return Err(format!("Simulation requires 1 to {MAX_POINTS} scalar tags"));
        }
        if validate(project).has_errors() {
            return Err("Simulation requires a valid canonical project".into());
        }
        let types = TypeContext::default();
        let mut points = BTreeMap::new();
        let mut mappings = Vec::with_capacity(project.tags.len());
        for tag in &project.tags {
            let id = tag.id.as_str();
            let wire = wire_type(&tag.data_type).map_err(|e| format!("Tag {id}: {e}"))?;
            let policy = project
                .v2
                .industrial
                .tags
                .iter()
                .find(|policy| policy.tag == tag.id);
            let pulse_ms = policy.and_then(|policy| policy.pulse_ms);
            if pulse_ms.is_some_and(|ms| !(100..=5000).contains(&ms)) {
                return Err(format!(
                    "Tag {id}: simulation pulses require 100 to 5000 ms"
                ));
            }
            let mut limits = Limits::None;
            if let Some(policy) = policy.filter(|p| p.command == CommandKind::Setpoint) {
                let numeric_limit = |number: &serde_json::Number| {
                    types
                        .value_from_json(&Json::Number(number.clone()), &tag.data_type)
                        .map_err(|e| format!("Tag {id}: {e}"))
                };
                let min = policy.limits.min.as_ref().map(numeric_limit).transpose()?;
                let max = policy.limits.max.as_ref().map(numeric_limit).transpose()?;
                limits = if tag.data_type.integer_bits().is_some() {
                    let step = policy.limits.step.as_ref().map(numeric_limit).transpose()?;
                    Limits::Integer {
                        min: min.and_then(|value| value.as_i128()),
                        max: max.and_then(|value| value.as_i128()),
                        step: step.and_then(|value| value.as_i128()),
                    }
                } else {
                    if policy.limits.step.is_some() {
                        return Err(format!(
                            "Tag {id}: floating-point steps are unsupported in simulation"
                        ));
                    }
                    let float = |value| match value {
                        TagValue::Float32(v) => Some(f64::from(v)),
                        TagValue::Float64(v) => Some(v),
                        _ => None,
                    };
                    Limits::Float {
                        min: min.and_then(float),
                        max: max.and_then(float),
                    }
                };
            }
            let access = match tag.access {
                TagAccess::Read => "READ",
                TagAccess::Write => "WRITE",
                TagAccess::ReadWrite => "READ_WRITE",
            };
            let mut mapping = json!({
                "mappingId": id, "variableId": id, "sourceId": SOURCE, "address": id,
                "valueType": wire, "access": access, "writeConfirmed": false
            });
            if let Some(ms) = pulse_ms {
                mapping["pulseMs"] = json!(ms);
            }
            mappings.push(mapping);
            points.insert(
                id.to_owned(),
                Point {
                    name: tag.name.clone(),
                    kind: tag.data_type.clone(),
                    access: tag.access,
                    pulse_ms,
                    confirm: policy.is_some_and(|p| p.confirmation_required || p.critical),
                    range: tag.range.clone(),
                    limits,
                    value: None,
                },
            );
        }
        Ok(Self {
            request: json!({"type":"configure","config":{"version":1,
                "sources":[{"id":SOURCE,"driver":"loopback","settings":{}}],
                "points":mappings}}),
            points,
        })
    }
}

impl ProjectSimulation {
    /// Uses the executable selected by the native host. Project connection
    /// declarations, endpoints, addresses, credentials and parameters stay in the editor.
    pub fn start(executable: &Path, project: &Project) -> Result<Self, String> {
        let Configuration { request, points } = Configuration::compile(project)?;
        let deadline = Instant::now() + TIMEOUT;
        let mut child =
            Supervisor::start_isolated(executable, TIMEOUT).map_err(|e| e.to_string())?;
        child
            .configure_and_start(request, deadline, &CancelToken::new())
            .map_err(|e| e.to_string())?;
        let mut simulation = Self {
            child,
            points,
            connected: false,
        };
        loop {
            simulation.drain()?;
            if simulation.connected
                && simulation
                    .points
                    .values()
                    .all(|p| !p.access.readable() || p.value.is_some())
            {
                return Ok(simulation);
            }
            if Instant::now() >= deadline {
                return Err("Simulation baseline timed out".into());
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    /// Reject stale or incompatible state before presenting data.
    pub fn read(&mut self) -> Result<Vec<SimulationPoint>, String> {
        self.drain()?;
        Ok(self
            .points
            .iter()
            .map(|(id, point)| SimulationPoint {
                id: id.clone(),
                name: point.name.clone(),
                value: point.value.clone(),
                writable: point.access.writable(),
                momentary: point.pulse_ms.is_some(),
            })
            .collect())
    }

    /// Whether the native host should ask the operator to confirm this simulation action.
    pub fn confirmation_required(&self, id: &str) -> Result<bool, String> {
        Ok(self.writable_point(id)?.confirm)
    }

    /// Submit an explicit maintained command or scalar setpoint in canonical JSON form.
    pub fn write(&mut self, id: &str, input: &str) -> Result<(), String> {
        let point = self.writable_point(id)?;
        if point.pulse_ms.is_some() {
            return Err("Momentary tags require a pulse".into());
        }
        if input.len() > MAX_INPUT_BYTES {
            return Err("Simulation value is too large".into());
        }
        let input = serde_json::from_str(input).map_err(|_| "Enter a valid JSON scalar")?;
        let value = TypeContext::default()
            .value_from_json(&input, &point.kind)
            .map_err(|e| e.to_string())?;
        point.check_write_value(&value)?;
        let wire = encode(&value, &point.kind).map_err(|e| e.to_string())?;
        self.submit(json!({"type":"write","variableId":id,"actor":ACTOR,"value":wire}))
    }

    /// Start an engine-owned momentary command using the canonical pulse duration.
    pub fn pulse(&mut self, id: &str) -> Result<(), String> {
        if self.writable_point(id)?.pulse_ms.is_none() {
            return Err("Tag has no momentary command".into());
        }
        self.submit(json!({"type":"pulse","variableId":id,"actor":ACTOR}))
    }

    fn writable_point(&self, id: &str) -> Result<&Point, String> {
        self.points
            .get(id)
            .filter(|p| p.access.writable())
            .ok_or_else(|| "Unknown or read-only simulation tag".into())
    }

    fn submit(&mut self, request: Json) -> Result<(), String> {
        self.drain()?;
        match self
            .child
            .simulation_command(request, TIMEOUT)
            .map_err(|e| e.to_string())?
        {
            Message::WriteResult { accepted: true, .. } => Ok(()),
            Message::WriteResult {
                refusal, message, ..
            } => {
                Err(message.unwrap_or_else(|| format!("Simulation command refused ({refusal:?})")))
            }
            _ => Err("Unexpected simulation command response".into()),
        }
    }

    fn drain(&mut self) -> Result<(), String> {
        let result = self.drain_events();
        if result.is_err() {
            let _ = self.child.invalidate();
        }
        result
    }

    fn drain_events(&mut self) -> Result<(), String> {
        for _ in 0..=crate::bridge_dispatch::MAX_EVENTS {
            match self.child.poll_event().map_err(|e| e.to_string())? {
                None => return Ok(()),
                Some(Message::SourceState {
                    source_id, state, ..
                }) if source_id == SOURCE => match state {
                    SourceState::Connected if !self.connected => self.connected = true,
                    SourceState::Connecting if !self.connected => {}
                    _ => return Err("Simulation source lost. Start a fresh session".into()),
                },
                Some(Message::Values { values }) if self.connected => {
                    for sample in values {
                        let point = self
                            .points
                            .get_mut(&sample.variable_id)
                            .filter(|p| p.access.readable())
                            .ok_or("Unexpected simulation tag")?;
                        let value = sample
                            .value
                            .filter(|_| sample.quality == Quality::Good)
                            .ok_or("Invalid simulation tag quality")?;
                        point.value = Some(
                            decode(&value, &point.kind)
                                .map_err(|e| e.to_string())?
                                .to_json()
                                .to_string(),
                        );
                    }
                }
                Some(Message::WriteAudit {
                    variable_id, actor, ..
                }) if actor == ACTOR && self.points.contains_key(&variable_id) => {}
                Some(Message::PulseReleased {
                    variable_id,
                    actor,
                    released: true,
                    ..
                }) if actor == ACTOR
                    && self
                        .points
                        .get(&variable_id)
                        .is_some_and(|p| p.pulse_ms.is_some()) => {}
                Some(_) => return Err("Unexpected simulation engine event".into()),
            }
        }
        Err("Simulation event backlog. Start a fresh session".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configuration_replaces_physical_routes_and_preserves_exact_limits() -> Result<(), String> {
        let mut canonical: Json = serde_json::from_str(include_str!(
            "../../project-model/fixtures/valid/minimal.hmi.json"
        ))
        .map_err(|e| e.to_string())?;
        canonical["schemaVersion"] = json!(2);
        canonical["connections"] = json!([{"id":"conn-production","name":"Physical",
            "driver":"siemensS7","endpoint":"192.0.2.1","credentialRef":"credential-production"}]);
        canonical["tags"] = json!([{"id":"tag-wide","name":"Wide","dataType":"int64",
            "connection":"conn-production","address":"DB1.DINT0","access":"readWrite",
            "range":{"min":9007199254740992_f64,"max":9007199254740994_f64}}]);
        canonical["v2"] = json!({"industrial":{"tags":[{
            "tag":"tag-wide","command":"setpoint","confirmationRequired":true,
            "limits":{"min":9007199254740992_i64,"max":9007199254740994_i64,"step":2}
        }]}});
        let project: Project = serde_json::from_value(canonical).map_err(|e| e.to_string())?;
        let configuration = Configuration::compile(&project)?;
        assert_eq!(
            configuration.request,
            json!({"type":"configure","config":{
                "version":1,"sources":[{"id":SOURCE,"driver":"loopback","settings":{}}],
                "points":[{"mappingId":"tag-wide","variableId":"tag-wide","sourceId":SOURCE,
                    "address":"tag-wide","valueType":"LONG","access":"READ_WRITE","writeConfirmed":false}]
            }})
        );
        let point = &configuration.points["tag-wide"];
        assert!(point.confirm);
        assert!(point
            .check_write_value(&TagValue::Int64(9007199254740994))
            .is_ok());
        assert_eq!(
            point.check_write_value(&TagValue::Int64(9007199254740995)),
            Err("Value is outside the tag's engineering range".into())
        );
        assert!(point
            .limits
            .check(&TagValue::Int64(9007199254740993))
            .is_err());
        assert!(point
            .limits
            .check(&TagValue::Int64(9007199254740994))
            .is_ok());
        assert!(point
            .limits
            .check(&TagValue::Int64(9007199254740996))
            .is_err());
        Ok(())
    }
}
