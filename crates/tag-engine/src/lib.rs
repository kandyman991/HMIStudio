//! # HMI Studio tag engine (RUST-06A)
//!
//! Framework-independent industrial tag execution on top of the RUST-03A
//! project model (`hmi-project-model`): it consumes the project's tags,
//! data types, connections and bindings as-is and adds runtime behavior.
//!
//! * [`value`] — typed runtime values, member paths, strict conversion.
//! * [`quality`] / [`time`] — value quality, timestamps, injectable clocks.
//! * [`engine`] — thread-safe tag store, access rights, ranges,
//!   subscriptions, scan groups, connection lifecycle, shutdown.
//! * [`driver`] — communication driver abstraction.
//! * [`simulator`] — in-memory simulated PLC (the only driver in RUST-06A).
//! * [`binding`] — binding compilation and read/write evaluation.
//!
//! No physical PLC connectivity, no network, no UI or Tauri dependency.
//! See `docs/TAG_ENGINE.md`.

pub mod binding;
pub mod driver;
pub mod engine;
pub mod quality;
pub mod simulator;
pub mod time;
pub mod value;

pub use binding::{
    compile, compile_for_node, BindingError, BindingScope, BoundValue, CompiledBinding,
};
pub use driver::{
    CancelToken, ConnectionStatus, Driver, DriverCapabilities, DriverError, DriverErrorKind,
    DriverFactory, DriverRegistry, OpContext, ReadItem, WriteItem,
};
pub use engine::{
    EngineConfig, InitIssue, InitReport, ScanMode, Subscription, TagEngine, TagError, TagEvent,
    TagSnapshot, WriteRequest,
};
pub use quality::{Quality, QualityLevel, QualityReason};
pub use simulator::{SimPlc, SimStep, SimulatorHub};
pub use time::{Clock, ManualClock, SystemClock, Timestamp};
pub use value::{MemberPath, Segment, TagValue, TypeContext, ValueError};

/// Registry with the simulator registered for `DriverKind::Simulator`.
pub fn simulator_registry(hub: &SimulatorHub) -> DriverRegistry {
    let mut r = DriverRegistry::new();
    r.register(
        hmi_project_model::model::DriverKind::Simulator,
        hub.factory(),
    );
    r
}

/// Bounded framing for the communication bridge.
pub mod bridge_frame;

/// Experimental communication child lifecycle.
pub mod bridge_supervisor;
