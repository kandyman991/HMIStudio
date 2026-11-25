#![allow(dead_code, clippy::unwrap_used)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use hmi_project_model::model::{Project, TagId};
use hmi_project_model::serialization::parse_value_with;
use hmi_project_model::MigrationRegistry;
use hmi_tag_engine::*;
use serde_json::{json, Value};

pub const START: Timestamp = Timestamp(1_700_000_000_000_000);

pub fn tid(s: &str) -> TagId {
    TagId::new(s).unwrap()
}

/// Synthetic project: internal tags, simulator tags (struct, array,
/// generator, write-only), and an OPC UA connection without a driver.
pub fn project_json() -> Value {
    json!({
        "format": "hmi-studio.project",
        "schemaVersion": 1,
        "id": "prj-tags",
        "metadata": { "name": "Tag engine tests (synthetic)" },
        "navigation": { "startScreen": "scr-main" },
        "localization": { "defaultLocale": "en", "locales": ["en"] },
        "units": [ { "id": "unit-pct", "symbol": "%" } ],
        "dataTypes": [ {
            "id": "dt-motor", "name": "Motor",
            "members": [
                { "name": "running", "dataType": "bool" },
                { "name": "speed", "dataType": "float32" },
                { "name": "codes", "dataType": { "array": { "element": "uint16", "length": 4 } } }
            ]
        } ],
        "connections": [
            { "id": "conn-sim", "name": "Simulator", "driver": "simulator",
              "parameters": { "timeoutMs": 200, "reconnectMs": 0, "defaultScanMs": 1000 } },
            { "id": "conn-opc", "name": "Future OPC UA", "driver": "opcUa", "endpoint": "opc.tcp://192.0.2.1:4840" }
        ],
        "tags": [
            { "id": "int-bool", "name": "Internal/Bool", "dataType": "bool", "access": "readWrite" },
            { "id": "int-real", "name": "Internal/Real", "dataType": "float32", "access": "readWrite",
              "unit": "unit-pct", "range": { "min": 0, "max": 100 }, "initialValue": 12.5 },
            { "id": "int-ro", "name": "Internal/ReadOnly", "dataType": "int16", "initialValue": 5 },
            { "id": "int-wo", "name": "Internal/WriteOnly", "dataType": "int32", "access": "write" },
            { "id": "int-text", "name": "Internal/Text", "dataType": "string", "access": "readWrite", "initialValue": "hello" },
            { "id": "int-motor", "name": "Internal/Motor", "dataType": { "struct": "dt-motor" }, "access": "readWrite" },
            { "id": "sim-motor", "name": "Sim/Motor", "dataType": { "struct": "dt-motor" }, "connection": "conn-sim",
              "address": "DB1.Motor", "access": "readWrite", "scanRateMs": 100 },
            { "id": "sim-speed", "name": "Sim/Speed", "dataType": "float32", "connection": "conn-sim",
              "address": "DB1.Speed", "access": "readWrite", "range": { "min": 0, "max": 600 }, "scanRateMs": 100,
              "initialValue": 42.5 },
            { "id": "sim-count", "name": "Sim/Count", "dataType": "uint32", "connection": "conn-sim", "address": "DB1.Count" },
            { "id": "sim-status", "name": "Sim/Status", "dataType": "uint16", "connection": "conn-sim", "address": "DB1.Status" },
            { "id": "sim-level", "name": "Sim/Level", "dataType": "int16", "connection": "conn-sim", "address": "DB1.Level" },
            { "id": "sim-arr", "name": "Sim/Array", "dataType": { "array": { "element": "int16", "length": 3 } },
              "connection": "conn-sim", "address": "DB1.Arr", "access": "readWrite" },
            { "id": "sim-sine", "name": "Sim/Sine", "dataType": "float64", "connection": "conn-sim",
              "address": "sine:offset=10,amplitude=2,period=4s" },
            { "id": "sim-cmd", "name": "Sim/Command", "dataType": "bool", "connection": "conn-sim",
              "address": "DB1.Cmd", "access": "write" },
            { "id": "opc-tag", "name": "Opc/Tag", "dataType": "float64", "connection": "conn-opc", "address": "ns=2;s=X" }
        ],
        "screens": [ { "id": "scr-main", "name": "Main", "root": { "id": "n-root", "element": { "type": "column" } } } ]
    })
}

pub fn project_from(v: Value) -> Project {
    parse_value_with(v, &MigrationRegistry::builtin())
        .unwrap_or_else(|e| panic!("{e}"))
        .project
}

pub fn project() -> Project {
    project_from(project_json())
}

pub struct Rig {
    pub engine: TagEngine,
    pub report: InitReport,
    pub clock: ManualClock,
    pub hub: SimulatorHub,
    pub plc: SimPlc,
}

pub fn rig_with(p: &Project, mode: ScanMode) -> Rig {
    let clock = ManualClock::new(START);
    let shared: Arc<dyn Clock> = Arc::new(clock.clone());
    let hub = SimulatorHub::new(shared.clone());
    let registry = simulator_registry(&hub);
    let (engine, report) = TagEngine::new(
        p,
        &registry,
        shared,
        EngineConfig {
            scan_mode: mode,
            max_idle_ms: 5,
            ..EngineConfig::default()
        },
    );
    let plc = hub.plc("conn-sim");
    Rig {
        engine,
        report,
        clock,
        hub,
        plc,
    }
}

pub fn rig() -> Rig {
    rig_with(&project(), ScanMode::Manual)
}

/// Connected rig with one full poll done.
pub fn connected() -> Rig {
    let r = rig();
    r.engine.poll_all();
    assert_eq!(
        r.engine
            .connection_status(&hmi_project_model::model::ConnectionId::new("conn-sim").unwrap()),
        Some(ConnectionStatus::Connected)
    );
    r
}

pub fn cid(s: &str) -> hmi_project_model::model::ConnectionId {
    hmi_project_model::model::ConnectionId::new(s).unwrap()
}

/// Drains currently queued events.
pub fn drain(rx: &EventReceiver) -> Vec<TagEvent> {
    let mut v = Vec::new();
    while let Ok(e) = rx.try_recv() {
        v.push(e);
    }
    v
}

pub fn changed_tags(events: &[TagEvent]) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| match e {
            TagEvent::Changed { snapshot } => Some(snapshot.tag.to_string()),
            _ => None,
        })
        .collect()
}

/// Runs `f` on a thread and fails if it does not finish within `secs`
/// (deadlock / hang detector).
pub fn within<T: Send + 'static>(secs: u64, f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(Duration::from_secs(secs))
        .unwrap_or_else(|_| panic!("operation did not finish within {secs}s (deadlock?)"))
}

pub fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../project-model/fixtures/valid")
        .join(name)
}
