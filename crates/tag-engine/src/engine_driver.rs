//! Explicitly registered, isolated read-only communication-engine adapter.
//! No physical endpoint, arbitrary command, arming or write replay is supported.
use crate::bridge_dispatch::{Quality, Value, ValueType};
use crate::bridge_simulation::{SimPoint, SimulationBridge, MAX_SIM_POINTS};
use crate::driver::*;
use crate::value::{TagValue, TypeContext};
use hmi_project_model::model::{Connection, DataType, DriverKind, Tag, TagId};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    path::PathBuf,
};

fn err(kind: DriverErrorKind, message: &str) -> DriverError {
    DriverError::new(kind, message)
}

fn mismatch() -> DriverError {
    err(
        DriverErrorKind::TypeMismatch,
        "value cannot be represented exactly in the declared type",
    )
}

/// Protocol 3 has only signed integer transports. UInt64 values above i64::MAX
/// are not representable and must be rejected, never rounded through a float.
pub fn wire_type(kind: &DataType) -> Result<ValueType, DriverError> {
    Ok(match kind {
        DataType::Bool => ValueType::Bool,
        DataType::Int8 | DataType::Int16 | DataType::Int32 | DataType::UInt8 | DataType::UInt16 => {
            ValueType::Int
        }
        DataType::Int64 | DataType::UInt32 | DataType::UInt64 => ValueType::Long,
        DataType::Float32 => ValueType::Float,
        DataType::Float64 => ValueType::Double,
        DataType::String => ValueType::String,
        _ => {
            return Err(err(
                DriverErrorKind::Unsupported,
                "unsupported protocol-3 type",
            ))
        }
    })
}

/// Encode only the exact declared type. This is a conversion primitive, not a write API.
pub fn encode(value: &TagValue, kind: &DataType) -> Result<Value, DriverError> {
    wire_type(kind)?;
    Ok(match (kind, value) {
        (DataType::Bool, TagValue::Bool(v)) => Value::Bool(*v),
        (DataType::Int8, TagValue::Int8(v)) => Value::Int(i32::from(*v)),
        (DataType::Int16, TagValue::Int16(v)) => Value::Int(i32::from(*v)),
        (DataType::Int32, TagValue::Int32(v)) => Value::Int(*v),
        (DataType::Int64, TagValue::Int64(v)) => Value::Long(*v),
        (DataType::UInt8, TagValue::UInt8(v)) => Value::Int(i32::from(*v)),
        (DataType::UInt16, TagValue::UInt16(v)) => Value::Int(i32::from(*v)),
        (DataType::UInt32, TagValue::UInt32(v)) => Value::Long(i64::from(*v)),
        (DataType::UInt64, TagValue::UInt64(v)) => {
            Value::Long(i64::try_from(*v).map_err(|_| mismatch())?)
        }
        (DataType::Float32, TagValue::Float32(v)) if v.is_finite() => Value::Float(*v),
        (DataType::Float64, TagValue::Float64(v)) if v.is_finite() => Value::Double(*v),
        (DataType::String, TagValue::String(v)) => Value::String(v.clone()),
        _ => return Err(mismatch()),
    })
}

/// Decode without cross-kind coercion, unchecked casts or integer-to-float conversion.
pub fn decode(value: &Value, kind: &DataType) -> Result<TagValue, DriverError> {
    wire_type(kind)?;
    Ok(match (kind, value) {
        (DataType::Bool, Value::Bool(v)) => TagValue::Bool(*v),
        (DataType::Int8, Value::Int(v)) => {
            TagValue::Int8(i8::try_from(*v).map_err(|_| mismatch())?)
        }
        (DataType::Int16, Value::Int(v)) => {
            TagValue::Int16(i16::try_from(*v).map_err(|_| mismatch())?)
        }
        (DataType::Int32, Value::Int(v)) => TagValue::Int32(*v),
        (DataType::Int64, Value::Long(v)) => TagValue::Int64(*v),
        (DataType::UInt8, Value::Int(v)) => {
            TagValue::UInt8(u8::try_from(*v).map_err(|_| mismatch())?)
        }
        (DataType::UInt16, Value::Int(v)) => {
            TagValue::UInt16(u16::try_from(*v).map_err(|_| mismatch())?)
        }
        (DataType::UInt32, Value::Long(v)) => {
            TagValue::UInt32(u32::try_from(*v).map_err(|_| mismatch())?)
        }
        (DataType::UInt64, Value::Long(v)) => {
            TagValue::UInt64(u64::try_from(*v).map_err(|_| mismatch())?)
        }
        (DataType::Float32, Value::Float(v)) if v.is_finite() => TagValue::Float32(*v),
        (DataType::Float64, Value::Double(v)) if v.is_finite() => TagValue::Float64(*v),
        (DataType::String, Value::String(v)) => TagValue::String(v.clone()),
        _ => return Err(mismatch()),
    })
}

/// Host-supplied executable, never resolved from project parameters.
pub struct EngineDriverFactory {
    executable: PathBuf,
}

impl EngineDriverFactory {
    pub fn new(executable: PathBuf) -> Self {
        Self { executable }
    }
}

impl DriverFactory for EngineDriverFactory {
    fn create(
        &self,
        connection: &Connection,
        tags: &[&Tag],
        _types: &TypeContext,
    ) -> Result<Box<dyn Driver>, DriverError> {
        if connection.driver != DriverKind::Simulator
            || connection.endpoint.is_some()
            || connection.credential_ref.is_some()
            || !self.executable.is_absolute()
            || tags.is_empty()
            || tags.len() > MAX_SIM_POINTS
        {
            return Err(err(DriverErrorKind::Unsupported, "isolated simulator requires host executable and 1–4 points, no endpoint or credentials"));
        }
        // Reject unsupported project configurations instead of silently changing semantics.
        let mut addresses = BTreeSet::new();
        let mut mappings = BTreeMap::new();
        let mut points = Vec::new();
        for tag in tags {
            let address = tag
                .address
                .as_ref()
                .filter(|a| !a.is_empty() && a.len() <= 128)
                .ok_or_else(|| err(DriverErrorKind::AddressInvalid, "invalid synthetic address"))?;
            if tag.access.writable() || !tag.access.readable() || tag.initial_value.is_some() {
                return Err(err(
                    DriverErrorKind::Unsupported,
                    "this adapter supports read-only zero-initialized loopback points",
                ));
            }
            if !addresses.insert(address.clone()) || tag.id.to_string().len() > 128 {
                return Err(err(
                    DriverErrorKind::AddressInvalid,
                    "duplicate address or oversized tag ID",
                ));
            }
            let kind = wire_type(&tag.data_type)?;
            points.push(SimPoint {
                id: tag.id.to_string(),
                kind,
            });
            if mappings
                .insert(tag.id.clone(), (address.clone(), tag.data_type.clone()))
                .is_some()
            {
                return Err(err(DriverErrorKind::AddressInvalid, "duplicate tag ID"));
            }
        }
        Ok(Box::new(EngineDriver {
            executable: self.executable.clone(),
            points,
            mappings,
            bridge: SimulationBridge::default(),
            generation: None,
        }))
    }
}

struct EngineDriver {
    executable: PathBuf,
    points: Vec<SimPoint>,
    mappings: BTreeMap<TagId, (String, DataType)>,
    bridge: SimulationBridge,
    generation: Option<u64>,
}

fn operation_error(e: io::Error, fallback: DriverErrorKind) -> DriverError {
    let kind = match e.kind() {
        io::ErrorKind::Interrupted => DriverErrorKind::Cancelled,
        io::ErrorKind::TimedOut => DriverErrorKind::Timeout,
        _ => fallback,
    };
    DriverError::new(kind, e.to_string())
}

impl EngineDriver {
    fn check_context(ctx: &OpContext) -> Result<(), DriverError> {
        if ctx.cancel.is_cancelled() {
            return Err(err(DriverErrorKind::Cancelled, "operation cancelled"));
        }
        if ctx.timeout.is_zero() {
            return Err(err(DriverErrorKind::Timeout, "operation timeout"));
        }
        Ok(())
    }
}

impl Driver for EngineDriver {
    fn capabilities(&self) -> DriverCapabilities {
        DriverCapabilities {
            driver: "engine-loopback".into(),
            grouped_read: true,
            grouped_write: false,
            max_items_per_read: MAX_SIM_POINTS,
            max_items_per_write: 1,
            member_writes: false,
            arrays: false,
            structs: false,
            min_scan_interval_ms: 10,
            physical: false,
        }
    }

    fn connect(&mut self, ctx: &OpContext) -> Result<(), DriverError> {
        self.disconnect();
        Self::check_context(ctx)?;
        let snapshot = self
            .bridge
            .resynchronize_cancellable(&self.executable, &self.points, ctx.timeout, &ctx.cancel)
            .map_err(|e| operation_error(e, DriverErrorKind::ConnectFailed))?;
        // Do not admit a generation whose complete baseline cannot be decoded.
        for (id, (_, kind)) in &self.mappings {
            let result = snapshot
                .values
                .get(&id.to_string())
                .and_then(|p| p.value.as_ref())
                .ok_or_else(mismatch)
                .and_then(|v| decode(v, kind));
            if let Err(e) = result {
                self.disconnect();
                return Err(e);
            }
        }
        if let Err(e) = Self::check_context(ctx) {
            self.disconnect();
            return Err(e);
        }
        self.generation = Some(snapshot.generation);
        Ok(())
    }

    fn disconnect(&mut self) {
        self.generation = None;
        self.bridge.invalidate();
    }

    fn is_connected(&self) -> bool {
        // Belief only; every read checks live transport and generation.
        self.generation.is_some()
    }

    fn read(
        &mut self,
        items: &[ReadItem],
        ctx: &OpContext,
    ) -> Result<Vec<ItemResult<TagValue>>, DriverError> {
        Self::check_context(ctx)?;
        if items.len() > MAX_SIM_POINTS {
            return Err(err(DriverErrorKind::Protocol, "read exceeds group limit"));
        }
        let generation = self
            .generation
            .ok_or_else(|| err(DriverErrorKind::NotConnected, "no synchronized generation"))?;
        let snapshot = match self.bridge.refresh_for(generation) {
            Ok(snapshot) => snapshot,
            Err(e) => {
                self.disconnect();
                return Err(operation_error(e, DriverErrorKind::ConnectionLost));
            }
        };
        Self::check_context(ctx)?;
        Ok(items
            .iter()
            .map(|item| {
                let (address, kind) = self
                    .mappings
                    .get(&item.tag)
                    .ok_or_else(|| err(DriverErrorKind::AddressInvalid, "unknown tag"))?;
                if &item.address != address {
                    return Err(err(DriverErrorKind::AddressInvalid, "address mismatch"));
                }
                if &item.data_type != kind {
                    return Err(mismatch());
                }
                let point = snapshot
                    .values
                    .get(&item.tag.to_string())
                    .ok_or_else(mismatch)?;
                if point.quality != Quality::Good {
                    return Err(err(DriverErrorKind::Protocol, "point quality not good"));
                }
                decode(point.value.as_ref().ok_or_else(mismatch)?, kind)
            })
            .collect())
    }

    fn write(
        &mut self,
        items: &[WriteItem],
        ctx: &OpContext,
    ) -> Result<Vec<ItemResult<()>>, DriverError> {
        Self::check_context(ctx)?;
        Ok(items
            .iter()
            .map(|_| {
                Err(err(
                    DriverErrorKind::Unsupported,
                    "isolated read adapter has no write operation",
                ))
            })
            .collect())
    }
}
