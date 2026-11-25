//! Generation-scoped resynchronization of an explicitly isolated read-only loopback child.
use crate::bridge_dispatch::{Message, PointValue, Quality, SourceState, Value, ValueType};
use crate::bridge_supervisor::{check_cancel, Supervisor};
use crate::driver::CancelToken;
use std::{
    collections::BTreeMap,
    io,
    path::Path,
    time::{Duration, Instant},
};

/// Keep initial loopback reports below the transport burst budget (2 source events + 4 values).
pub const MAX_SIM_POINTS: usize = 4;

#[derive(Debug, Clone)]
pub struct SimPoint {
    pub id: String,
    pub kind: ValueType,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub generation: u64,
    pub values: BTreeMap<String, PointValue>,
}

#[derive(Default)]
pub struct SimulationBridge {
    generation: u64,
    session: Option<Supervisor>,
    baseline: Option<Baseline>,
}

struct Baseline {
    expected: BTreeMap<String, ValueType>,
    values: BTreeMap<String, PointValue>,
    connected: bool,
}

fn invalid(reason: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, reason)
}

impl Baseline {
    fn new(points: &[SimPoint]) -> io::Result<Self> {
        if points.is_empty() || points.len() > MAX_SIM_POINTS {
            return Err(invalid("isolated simulation point limit"));
        }
        let mut expected = BTreeMap::new();
        for p in points {
            if p.id.is_empty()
                || p.id.len() > 128
                || expected.insert(p.id.clone(), p.kind).is_some()
            {
                return Err(invalid("invalid or duplicate simulation point ID"));
            }
        }
        Ok(Self {
            expected,
            values: BTreeMap::new(),
            connected: false,
        })
    }

    fn apply(&mut self, event: Message) -> io::Result<()> {
        match event {
            Message::SourceState {
                source_id, state, ..
            } if source_id == "isolated-loopback" => match state {
                SourceState::Connected => self.connected = true,
                SourceState::Connecting if !self.connected => self.values.clear(),
                _ => return Err(invalid("source lost; fresh generation required")),
            },
            Message::Values { values } if self.connected => {
                for value in values {
                    let kind = self
                        .expected
                        .get(&value.variable_id)
                        .ok_or_else(|| invalid("unexpected point"))?;
                    let actual = match &value.value {
                        Some(Value::Bool(_)) => ValueType::Bool,
                        Some(Value::Int(_)) => ValueType::Int,
                        Some(Value::Long(_)) => ValueType::Long,
                        Some(Value::Float(v)) if v.is_finite() => ValueType::Float,
                        Some(Value::Double(v)) if v.is_finite() => ValueType::Double,
                        Some(Value::String(_)) => ValueType::String,
                        _ => return Err(invalid("missing or invalid simulation value")),
                    };
                    if actual != *kind || value.quality != Quality::Good {
                        return Err(invalid("simulation baseline is not good and type-correct"));
                    }
                    self.values.insert(value.variable_id.clone(), value);
                }
            }
            _ => return Err(invalid("unexpected event in read-only simulation")),
        }
        Ok(())
    }

    fn complete(&self) -> bool {
        self.connected && self.values.len() == self.expected.len()
    }
}

impl SimulationBridge {
    pub fn invalidate(&mut self) {
        // Drop the cache before terminating the old process. No old value can be reused.
        self.baseline = None;
        self.session.take();
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Explicit fresh process and complete baseline. Never replays prior commands.
    pub fn resynchronize(
        &mut self,
        executable: &Path,
        points: &[SimPoint],
        timeout: Duration,
    ) -> io::Result<Snapshot> {
        self.resynchronize_cancellable(executable, points, timeout, &CancelToken::new())
    }

    pub(crate) fn resynchronize_cancellable(
        &mut self,
        executable: &Path,
        points: &[SimPoint],
        timeout: Duration,
        cancel: &CancelToken,
    ) -> io::Result<Snapshot> {
        self.invalidate();
        check_cancel(cancel)?;
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or_else(|| invalid("generation exhausted"))?;
        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or_else(|| invalid("invalid timeout"))?;
        let remaining = || {
            deadline
                .checked_duration_since(Instant::now())
                .filter(|d| !d.is_zero())
                .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "baseline timeout"))
        };
        let mut baseline = Baseline::new(points)?;
        let mut session = Supervisor::start_cancellable(executable, remaining()?, cancel)?;
        // No unsolicited data may precede the configuration/start sequence.
        if session.poll_event()?.is_some() {
            return Err(invalid("pre-configuration event"));
        }
        session.start_loopback(points, deadline, cancel)?;
        while !baseline.complete() {
            check_cancel(cancel)?;
            remaining()?;
            match session.poll_event()? {
                Some(event) => baseline.apply(event)?,
                None => {
                    cancel.wait(Duration::from_millis(1));
                }
            }
        }
        check_cancel(cancel)?;
        remaining()?;
        self.baseline = Some(baseline);
        self.session = Some(session);
        self.refresh()
    }

    /// Reject old generation tokens without altering a newer valid session.
    pub fn refresh_for(&mut self, generation: u64) -> io::Result<Snapshot> {
        if generation != self.generation || self.baseline.is_none() {
            return Err(invalid("stale or invalid generation"));
        }
        self.refresh()
    }

    /// Check transport health and consume pending events before exposing state.
    /// On any fault, the current generation is invalid and no snapshot is returned.
    pub fn refresh(&mut self) -> io::Result<Snapshot> {
        let result = (|| {
            let session = self
                .session
                .as_mut()
                .ok_or_else(|| invalid("resynchronization required"))?;
            let baseline = self
                .baseline
                .as_mut()
                .ok_or_else(|| invalid("resynchronization required"))?;
            // Bounded work per call, fail closed if the backlog cannot be drained.
            for _ in 0..=crate::bridge_dispatch::MAX_EVENTS {
                match session.poll_event()? {
                    Some(event) => baseline.apply(event)?,
                    None => {
                        if !session.is_alive()? || !baseline.complete() {
                            return Err(invalid("baseline lost"));
                        }
                        return Ok(Snapshot {
                            generation: self.generation,
                            values: baseline.values.clone(),
                        });
                    }
                }
            }
            Err(invalid("event backlog; fresh generation required"))
        })();
        if result.is_err() {
            self.invalidate();
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn points() -> [SimPoint; 2] {
        [
            SimPoint {
                id: "a".into(),
                kind: ValueType::Long,
            },
            SimPoint {
                id: "b".into(),
                kind: ValueType::Bool,
            },
        ]
    }

    fn connected() -> Message {
        Message::SourceState {
            source_id: "isolated-loopback".into(),
            state: SourceState::Connected,
            message: None,
        }
    }

    fn value(id: &str, value: Value, quality: Quality) -> Message {
        Message::Values {
            values: vec![PointValue {
                variable_id: id.into(),
                value: Some(value),
                quality,
                status: None,
                source_timestamp_ms: None,
            }],
        }
    }

    #[test]
    fn requires_every_expected_point_and_connected_source() -> io::Result<()> {
        let mut b = Baseline::new(&points())?;
        assert!(b.apply(value("a", Value::Long(1), Quality::Good)).is_err());
        b.apply(connected())?;
        b.apply(value("a", Value::Long(1), Quality::Good))?;
        b.apply(value("a", Value::Long(2), Quality::Good))?;
        assert!(!b.complete());
        b.apply(value("b", Value::Bool(true), Quality::Good))?;
        assert!(b.complete());
        assert_eq!(b.values["a"].value, Some(Value::Long(2)));
        Ok(())
    }

    #[test]
    fn rejects_unexpected_wrong_type_bad_quality_and_source_loss() -> io::Result<()> {
        let mut b = Baseline::new(&points())?;
        b.apply(connected())?;
        assert!(b
            .apply(value("unexpected", Value::Long(1), Quality::Good))
            .is_err());
        assert!(b
            .apply(value("a", Value::Bool(true), Quality::Good))
            .is_err());
        assert!(b.apply(value("a", Value::Long(1), Quality::Bad)).is_err());
        assert!(b
            .apply(Message::SourceState {
                source_id: "isolated-loopback".into(),
                state: SourceState::Disconnected,
                message: None
            })
            .is_err());
        Ok(())
    }

    #[test]
    fn duplicate_and_unbounded_point_sets_are_refused() {
        let point = SimPoint {
            id: "a".into(),
            kind: ValueType::Long,
        };
        assert!(Baseline::new(&[point.clone(), point.clone()]).is_err());
        assert!(Baseline::new(&vec![point; MAX_SIM_POINTS + 1]).is_err());
    }
}
