//! The shared contract fixtures in `protocol/`. The Kotlin side runs the same files
//! (`core/.../runtime/EngineContractTest.kt`), so the two implementations cannot drift apart.

use std::collections::BTreeSet;

use hmi_engine::guard::{check_write, GuardState};
use hmi_engine::protocol::{Message, Request, RuntimeConfig, Value, WriteRefusal};
use serde_json::Value as Json;

fn fixture(name: &str) -> Json {
    let path = format!("{}/protocol/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
}

#[test]
fn every_request_and_message_round_trips_unchanged() {
    let fixture = fixture("messages.json");
    for example in fixture["requests"].as_array().unwrap() {
        let request: Request =
            serde_json::from_value(example.clone()).unwrap_or_else(|e| panic!("{example}: {e}"));
        assert_eq!(&serde_json::to_value(&request).unwrap(), example);
    }
    for example in fixture["messages"].as_array().unwrap() {
        let message: Message =
            serde_json::from_value(example.clone()).unwrap_or_else(|e| panic!("{example}: {e}"));
        assert_eq!(&serde_json::to_value(&message).unwrap(), example);
    }
    for example in fixture["rejectedRequests"].as_array().unwrap() {
        assert!(
            serde_json::from_value::<Request>(example.clone()).is_err(),
            "should be refused: {example}"
        );
    }
}

#[test]
fn write_guard_matches_the_shared_cases() {
    let fixture = fixture("write-guard-cases.json");
    let config: RuntimeConfig = serde_json::from_value(fixture["config"].clone()).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let connected: BTreeSet<String> = case["connected"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect();
        let value: Value = serde_json::from_value(case["value"].clone()).unwrap();
        let expected: Option<WriteRefusal> =
            serde_json::from_value(case["expected"].clone()).unwrap();
        let state = GuardState {
            config: case["configured"].as_bool().unwrap().then_some(&config),
            armed: case["armed"].as_bool().unwrap(),
            connected_sources: &connected,
        };
        let pulse = case["pulse"].as_bool().unwrap_or(false);
        let actual = check_write(&state, case["variableId"].as_str().unwrap(), &value, pulse).err();
        assert_eq!(actual, expected, "case '{name}'");
    }
}

#[test]
fn non_finite_numbers_are_never_written() {
    let fixture = fixture("write-guard-cases.json");
    let config: RuntimeConfig = serde_json::from_value(fixture["config"].clone()).unwrap();
    let connected: BTreeSet<String> = ["plc".to_owned()].into();
    let state = GuardState {
        config: Some(&config),
        armed: true,
        connected_sources: &connected,
    };
    for value in [
        Value::Double(f64::NAN),
        Value::Double(f64::INFINITY),
        Value::Double(f64::NEG_INFINITY),
    ] {
        assert_eq!(
            check_write(&state, "line.ratio", &value, false).err(),
            Some(WriteRefusal::NotFinite)
        );
    }
    for value in [Value::Float(f32::NAN), Value::Float(f32::INFINITY)] {
        assert_eq!(
            check_write(&state, "line.speed", &value, false).err(),
            Some(WriteRefusal::NotFinite)
        );
    }
}

#[test]
fn credentials_never_appear_in_debug_output() {
    let fixture = fixture("messages.json");
    let request: Request = serde_json::from_value(fixture["requests"][1].clone()).unwrap();
    let debug = format!("{request:?}");
    assert!(
        debug.contains("operator") && !debug.contains("example-only"),
        "{debug}"
    );
}
