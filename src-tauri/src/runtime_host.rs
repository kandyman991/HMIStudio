//! Native simulation admission and lifetime. Only typed operations cross IPC.
use hmi_tag_engine::bridge_project_simulation::{ProjectSimulation, SimulationPoint};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
pub enum RuntimeRequest {
    Start {
        revision: u64,
    },
    Read {
        session: u32,
    },
    Write {
        session: u32,
        tag: String,
        value: String,
    },
    Pulse {
        session: u32,
        tag: String,
    },
    Stop {
        session: u32,
    },
}

#[derive(Serialize)]
pub struct RuntimeReply {
    pub session: u32,
    pub points: Vec<SimulationPoint>,
}

/// Authoritative outcome of a native simulation operation. A rejected
/// command may retain a freshly checked session; a lost session never does.
#[derive(Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum RuntimeOutcome {
    Active {
        simulation: RuntimeReply,
    },
    Stopped,
    Rejected {
        message: String,
        simulation: Option<RuntimeReply>,
    },
    Lost {
        message: String,
    },
    Busy,
}

impl RuntimeRequest {
    pub fn session(&self) -> Option<u32> {
        match self {
            Self::Start { .. } => None,
            Self::Read { session }
            | Self::Write { session, .. }
            | Self::Pulse { session, .. }
            | Self::Stop { session } => Some(*session),
        }
    }
}

#[derive(Default)]
pub struct RuntimeHost {
    executable: Option<PathBuf>,
    serial: u32,
    active: Option<ActiveSimulation>,
}

struct ActiveSimulation {
    revision: u64,
    bridge: ProjectSimulation,
}

impl RuntimeHost {
    pub fn new(executable: Option<PathBuf>) -> Self {
        Self {
            executable,
            ..Self::default()
        }
    }

    pub fn stop(&mut self) {
        self.active.take();
    }

    pub fn invalidate_on_edit(&mut self, revision: u64) {
        if self
            .active
            .as_ref()
            .is_some_and(|active| active.revision != revision)
        {
            self.stop();
        }
    }

    pub fn start(
        &mut self,
        snapshot: &hmi_engineering_core::ProjectSnapshot,
    ) -> Result<RuntimeReply, String> {
        if self.active.is_some() {
            return Err("Stop the active simulation before starting a new one".into());
        }
        let executable = self
            .executable
            .as_deref()
            .filter(|path| path.is_absolute())
            .ok_or("No simulation engine configured by the desktop host")?;
        let session = self
            .serial
            .checked_add(1)
            .ok_or("Simulation session IDs exhausted")?;
        let mut bridge = ProjectSimulation::start(executable, &snapshot.project)?;
        let points = bridge.read()?;
        self.serial = session;
        self.active = Some(ActiveSimulation {
            revision: snapshot.revision,
            bridge,
        });
        Ok(RuntimeReply { session, points })
    }

    /// Refresh only the requested session. Start retries may discover an
    /// already-active session after an interrupted IPC response.
    pub fn refresh(&mut self, session: Option<u32>) -> Result<Option<RuntimeReply>, String> {
        if session.is_some_and(|id| id != self.serial) {
            return Ok(None);
        }
        let Some(active) = self.active.as_mut() else {
            return Ok(None);
        };
        match active.bridge.read() {
            Ok(points) => Ok(Some(RuntimeReply {
                session: self.serial,
                points,
            })),
            Err(error) => {
                self.stop();
                Err(error)
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn child_process_id(&self) -> u32 {
        self.active.as_ref().unwrap().bridge.process_id()
    }

    pub fn run(
        &mut self,
        request: RuntimeRequest,
        confirm: impl FnOnce() -> bool,
    ) -> Result<Option<RuntimeReply>, String> {
        let session = match &request {
            RuntimeRequest::Read { session }
            | RuntimeRequest::Write { session, .. }
            | RuntimeRequest::Pulse { session, .. }
            | RuntimeRequest::Stop { session } => *session,
            RuntimeRequest::Start { .. } => {
                return Err("Start requires an active project snapshot".into())
            }
        };
        if matches!(request, RuntimeRequest::Stop { .. }) && session == self.serial {
            self.stop();
            return Ok(None);
        }
        let active = self
            .active
            .as_mut()
            .filter(|_| session == self.serial)
            .ok_or("Simulation session changed. Start a new simulation")?;
        let outcome = match request {
            RuntimeRequest::Write { tag, value, .. } => {
                if active.bridge.confirmation_required(&tag)? && !confirm() {
                    return Err("Simulation write canceled".into());
                }
                active.bridge.write(&tag, &value)
            }
            RuntimeRequest::Pulse { tag, .. } => {
                if active.bridge.confirmation_required(&tag)? && !confirm() {
                    return Err("Simulation pulse canceled".into());
                }
                active.bridge.pulse(&tag)
            }
            _ => Ok(()),
        };
        let points = match active.bridge.read() {
            Ok(points) => points,
            Err(error) => {
                self.stop();
                return Err(format!("Simulation session lost: {error}"));
            }
        };
        outcome?;
        Ok(Some(RuntimeReply { session, points }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lost_session_reply_contains_no_cached_points() {
        let reply = RuntimeOutcome::Lost {
            message: "child exited".into(),
        };
        assert_eq!(
            serde_json::to_value(reply).unwrap(),
            serde_json::json!({"status":"lost","message":"child exited"})
        );
    }

    #[test]
    fn runtime_ipc_disallows_configuration_and_transport_fields() {
        for input in [
            r#"{"action":"start","revision":0,"driver":"s7"}"#,
            r#"{"action":"start","revision":0,"executable":"/tmp/engine"}"#,
            r#"{"action":"start","revision":0,"config":{}}"#,
            r#"{"action":"configure","session":1}"#,
            r#"{"action":"write","session":1,"tag":"x","value":"1","confirmed":true}"#,
            r#"{"action":"pulse","session":1,"tag":"x","pulseMs":250}"#,
        ] {
            assert!(
                serde_json::from_str::<RuntimeRequest>(input).is_err(),
                "{input}"
            );
        }
    }
}
