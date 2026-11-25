//! Explicit real child acceptance, invoked with a freshly built engine binary.
use hmi_tag_engine::bridge_dispatch::{Quality, Value, ValueType};
use hmi_tag_engine::bridge_simulation::{SimPoint, SimulationBridge};
use std::{io, path::PathBuf, time::Duration};

#[test]
#[ignore = "requires freshly built hmi-engine; run explicitly with HMI_ENGINE_TEST_BIN"]
fn real_loopback_baseline_and_new_generation() -> io::Result<()> {
    let exe = PathBuf::from(
        std::env::var_os("HMI_ENGINE_TEST_BIN")
            .ok_or_else(|| io::Error::other("HMI_ENGINE_TEST_BIN required"))?,
    );
    let mut bridge = SimulationBridge::default();
    let timeout = Duration::from_secs(5);
    let points = [
        SimPoint {
            id: "count".into(),
            kind: ValueType::Long,
        },
        SimPoint {
            id: "ready".into(),
            kind: ValueType::Bool,
        },
    ];
    let first = bridge.resynchronize(&exe, &points, timeout)?;
    assert_eq!(first.values.len(), 2);
    assert_eq!(first.values["count"].value, Some(Value::Long(0)));
    assert_eq!(first.values["ready"].quality, Quality::Good);
    assert_eq!(bridge.refresh()?, first);
    bridge.invalidate();
    assert!(bridge.refresh().is_err());
    assert!(bridge.refresh_for(first.generation).is_err());
    let second = bridge.resynchronize(
        &exe,
        &[SimPoint {
            id: "new".into(),
            kind: ValueType::String,
        }],
        timeout,
    )?;
    assert!(second.generation > first.generation);
    assert!(bridge.refresh_for(first.generation).is_err());
    assert_eq!(bridge.refresh_for(second.generation)?, second);
    assert_eq!(second.values.len(), 1);
    assert!(!second.values.contains_key("count"));
    assert_eq!(
        second.values["new"].value,
        Some(Value::String(String::new()))
    );
    assert!(bridge.resynchronize(&exe, &[], timeout).is_err());
    assert!(bridge.refresh().is_err());
    Ok(())
}

#[test]
#[ignore = "requires freshly built hmi-engine; run explicitly with HMI_ENGINE_TEST_BIN"]
fn real_child_integrates_with_tag_engine() -> io::Result<()> {
    use hmi_project_model::model::{ConnectionId, DriverKind, TagId};
    use hmi_project_model::{serialization::parse_value_with, MigrationRegistry};
    use hmi_tag_engine::{engine_driver::EngineDriverFactory, *};
    use std::sync::Arc;
    let exe = PathBuf::from(
        std::env::var_os("HMI_ENGINE_TEST_BIN")
            .ok_or_else(|| io::Error::other("HMI_ENGINE_TEST_BIN required"))?,
    );
    let json = serde_json::json!({
        "format":"hmi-studio.project","schemaVersion":1,"id":"prj-loopback",
        "metadata":{"name":"Synthetic isolated adapter"},"navigation":{"startScreen":"scr-main"},
        "localization":{"defaultLocale":"en","locales":["en"]},
        "connections":[{"id":"conn-loop","name":"Loopback","driver":"simulator","parameters":{"timeoutMs":5000}}],
        "tags":[{"id":"tag-count","name":"Count","dataType":"uint32","connection":"conn-loop","address":"count"},
                {"id":"tag-ready","name":"Ready","dataType":"bool","connection":"conn-loop","address":"ready"}],
        "screens":[{"id":"scr-main","name":"Main","root":{"id":"n-root","element":{"type":"column"}}}]
    });
    let p = parse_value_with(json, &MigrationRegistry::builtin())
        .map_err(io::Error::other)?
        .project;
    let mut registry = DriverRegistry::new();
    registry.register(
        DriverKind::Simulator,
        Arc::new(EngineDriverFactory::new(exe)),
    );
    let (engine, report) = TagEngine::new(
        &p,
        &registry,
        Arc::new(ManualClock::new(Timestamp(0))),
        EngineConfig {
            scan_mode: ScanMode::Manual,
            ..EngineConfig::default()
        },
    );
    assert!(report.issues.is_empty(), "{:?}", report.issues);
    let count = TagId::new("tag-count").map_err(io::Error::other)?;
    let ready = TagId::new("tag-ready").map_err(io::Error::other)?;
    let conn = ConnectionId::new("conn-loop").map_err(io::Error::other)?;
    assert!(!engine
        .read(&count)
        .map_err(io::Error::other)?
        .quality
        .is_good());
    assert_eq!(engine.poll_all(), 2);
    assert_eq!(
        engine.connection_status(&conn),
        Some(ConnectionStatus::Connected)
    );
    let count_value = engine.read(&count).map_err(io::Error::other)?;
    assert_eq!(count_value.value, TagValue::UInt32(0));
    assert!(count_value.quality.is_good());
    assert_eq!(
        engine.read(&ready).map_err(io::Error::other)?.value,
        TagValue::Bool(false)
    );
    engine.shutdown();
    assert_eq!(
        engine.connection_status(&conn),
        Some(ConnectionStatus::Stopped)
    );
    assert!(matches!(engine.read(&count), Err(TagError::ShutDown)));
    Ok(())
}
