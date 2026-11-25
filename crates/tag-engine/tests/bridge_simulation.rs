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
