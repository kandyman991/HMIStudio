//! The engine's own write guard.
//!
//! This is deliberately independent of the Kotlin-side check: whatever a client sends, a value
//! reaches a driver only if every rule here passes. The rules and their order are identical on
//! both sides and are pinned by the shared cases in `protocol/write-guard-cases.json`.

use std::collections::BTreeSet;

use crate::protocol::{RuntimeConfig, RuntimePoint, Value, WriteRefusal};

/// Longest period writes can stay armed without being re-armed.
pub const MAX_ARM_SECONDS: u32 = 900;

/// Shortest and longest momentary pulse.
pub const MIN_PULSE_MS: u32 = 100;
pub const MAX_PULSE_MS: u32 = 5_000;

pub struct GuardState<'a> {
    pub config: Option<&'a RuntimeConfig>,
    /// True only while an armed, unexpired write period is active.
    pub armed: bool,
    pub connected_sources: &'a BTreeSet<String>,
}

/// Returns the point to write through, or the first rule the write breaks.
pub fn check_write<'a>(
    state: &GuardState<'a>,
    variable_id: &str,
    value: &Value,
    pulse: bool,
) -> Result<&'a RuntimePoint, WriteRefusal> {
    let config = state.config.ok_or(WriteRefusal::NotConfigured)?;
    let mut candidates = config
        .points
        .iter()
        .filter(|p| p.variable_id == variable_id)
        .peekable();
    if candidates.peek().is_none() {
        return Err(WriteRefusal::UnknownVariable);
    }
    let point = candidates
        .find(|p| p.writable())
        .ok_or(WriteRefusal::NotWritable)?;
    if !point.write_confirmed {
        return Err(WriteRefusal::NotConfirmed);
    }
    if value.value_type() != point.value_type {
        return Err(WriteRefusal::TypeMismatch);
    }
    if !value.is_finite() {
        return Err(WriteRefusal::NotFinite);
    }
    if pulse != point.pulse_ms.is_some() {
        return Err(WriteRefusal::WrongCommand);
    }
    if let Some(limits) = point.limits {
        match value.as_f64() {
            Some(number) if number >= limits.min && number <= limits.max => {}
            _ => return Err(WriteRefusal::OutOfLimits),
        }
    }
    if !state.armed {
        return Err(WriteRefusal::WritesDisarmed);
    }
    if !state.connected_sources.contains(&point.source_id) {
        return Err(WriteRefusal::SourceNotConnected);
    }
    Ok(point)
}

/// Structural checks on a configuration before anything connects.
pub fn validate_config(config: &RuntimeConfig, known_drivers: &[&str]) -> Result<(), String> {
    if config.version != crate::protocol::CONFIG_VERSION {
        return Err(format!(
            "runtime config version {} is not supported",
            config.version
        ));
    }
    let mut source_ids = BTreeSet::new();
    for source in &config.sources {
        if source.id.is_empty() || !source_ids.insert(source.id.as_str()) {
            return Err(format!("source id '{}' is empty or duplicated", source.id));
        }
        if !known_drivers.contains(&source.driver.as_str()) {
            return Err(format!(
                "source '{}' uses unknown driver '{}'",
                source.id, source.driver
            ));
        }
    }
    for source in &config.sources {
        let points: Vec<_> = config
            .points
            .iter()
            .filter(|p| p.source_id == source.id)
            .cloned()
            .collect();
        crate::driver::validate(source, &points)
            .map_err(|e| format!("source '{}': {e}", source.id))?;
    }
    let mut readers = BTreeSet::new();
    let mut writers = BTreeSet::new();
    for point in &config.points {
        if !source_ids.contains(point.source_id.as_str()) {
            return Err(format!(
                "point '{}' refers to unknown source '{}'",
                point.mapping_id, point.source_id
            ));
        }
        if point.address.trim().is_empty() {
            return Err(format!("point '{}' has an empty address", point.mapping_id));
        }
        if point.writable() && !point.write_confirmed {
            return Err(format!(
                "point '{}' is write-capable but not confirmed",
                point.mapping_id
            ));
        }
        if let Some(pulse) = point.pulse_ms {
            if !point.writable() || point.value_type != crate::protocol::ValueType::Bool {
                return Err(format!(
                    "point '{}': a pulse needs a writable BOOL point",
                    point.mapping_id
                ));
            }
            if !(MIN_PULSE_MS..=MAX_PULSE_MS).contains(&pulse) {
                return Err(format!(
                    "point '{}': pulseMs must be {MIN_PULSE_MS}..={MAX_PULSE_MS}",
                    point.mapping_id
                ));
            }
        }
        if let Some(limits) = point.limits {
            let numeric = matches!(
                point.value_type,
                crate::protocol::ValueType::Int
                    | crate::protocol::ValueType::Long
                    | crate::protocol::ValueType::Float
                    | crate::protocol::ValueType::Double
            );
            if !point.writable() || !numeric {
                return Err(format!(
                    "point '{}': limits need a writable numeric point",
                    point.mapping_id
                ));
            }
            if !limits.min.is_finite() || !limits.max.is_finite() || limits.min >= limits.max {
                return Err(format!(
                    "point '{}': limits need finite min < max",
                    point.mapping_id
                ));
            }
        }
        if point.readable() && !readers.insert(point.variable_id.as_str()) {
            return Err(format!(
                "variable '{}' is read through several points",
                point.variable_id
            ));
        }
        if point.writable() && !writers.insert(point.variable_id.as_str()) {
            return Err(format!(
                "variable '{}' is written through several points",
                point.variable_id
            ));
        }
    }
    Ok(())
}
