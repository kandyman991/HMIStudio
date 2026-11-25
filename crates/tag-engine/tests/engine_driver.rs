#![allow(clippy::unwrap_used)]
mod common;
use common::*;
use hmi_project_model::model::DataType as D;
use hmi_tag_engine::{
    bridge_dispatch::Value as V,
    engine_driver::{decode, encode, wire_type, EngineDriverFactory},
    *,
};
use std::time::Duration;

#[test]
fn exact_numeric_transport_and_refusal() {
    for (kind, value) in [
        (D::Bool, TagValue::Bool(true)),
        (D::Int8, TagValue::Int8(i8::MIN)),
        (D::Int16, TagValue::Int16(i16::MAX)),
        (D::Int32, TagValue::Int32(i32::MIN)),
        (D::Int64, TagValue::Int64(i64::MIN)),
        (D::Int64, TagValue::Int64(i64::MAX)),
        (D::Int64, TagValue::Int64(9_007_199_254_740_993)),
        (D::UInt8, TagValue::UInt8(u8::MAX)),
        (D::UInt16, TagValue::UInt16(u16::MAX)),
        (D::UInt32, TagValue::UInt32(u32::MAX)),
        (D::UInt64, TagValue::UInt64(i64::MAX as u64)),
        (D::Float32, TagValue::Float32(f32::MAX)),
        (D::Float64, TagValue::Float64(f64::MIN)),
        (D::String, TagValue::String("synthetic".into())),
    ] {
        let wire = encode(&value, &kind).unwrap();
        let bytes = serde_json::to_vec(&wire).unwrap();
        let restored = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(decode(&restored, &kind).unwrap(), value);
    }
    assert!(encode(&TagValue::UInt64(u64::MAX), &D::UInt64).is_err());
    assert!(encode(&TagValue::UInt64(i64::MAX as u64 + 1), &D::UInt64).is_err());
    assert!(encode(&TagValue::Int32(1), &D::Int64).is_err());
    assert!(encode(&TagValue::Float32(f32::NAN), &D::Float32).is_err());
    assert!(encode(&TagValue::Float64(f64::INFINITY), &D::Float64).is_err());
    assert!(decode(&V::Int(128), &D::Int8).is_err());
    assert!(decode(&V::Int(-1), &D::UInt8).is_err());
    assert!(decode(&V::Long(-1), &D::UInt64).is_err());
    assert!(decode(&V::Long(i64::MAX), &D::UInt32).is_err());
    assert!(decode(&V::Double(1.0), &D::Float32).is_err());
    assert!(decode(&V::Float(f32::INFINITY), &D::Float32).is_err());
    assert!(wire_type(&D::DateTime).is_err());
}

#[test]
fn rejects_nonisolated_and_unsupported_configuration_without_spawning() {
    let p = project();
    let factory = EngineDriverFactory::new(std::env::temp_dir().join("not-an-executable"));
    let types = TypeContext::from_project(&p);
    let connection = &p.connections[0];
    let tag = p.tags.iter().find(|t| t.id == tid("sim-count")).unwrap();
    assert!(factory.create(&p.connections[1], &[tag], &types).is_err());
    assert!(factory.create(connection, &[], &types).is_err());
    assert!(factory.create(connection, &[tag, tag], &types).is_err());
    let mut invalid = tag.clone();
    invalid.access = hmi_project_model::model::TagAccess::ReadWrite;
    assert!(factory.create(connection, &[&invalid], &types).is_err());
    let mut endpoint = connection.clone();
    endpoint.endpoint = Some("synthetic".into());
    assert!(factory.create(&endpoint, &[tag], &types).is_err());
    let mut driver = factory.create(connection, &[tag], &types).unwrap();
    let ctx = OpContext {
        timeout: Duration::from_secs(1),
        cancel: CancelToken::new(),
    };
    ctx.cancel.cancel();
    assert_eq!(
        driver.connect(&ctx).unwrap_err().kind,
        DriverErrorKind::Cancelled
    );
    assert!(!driver.is_connected());
}

#[cfg(unix)]
mod subprocess {
    use super::*;
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        path::PathBuf,
        sync::{
            atomic::{AtomicU64, Ordering},
            Arc,
        },
        time::Instant,
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);
    // Serialize fixture creation/spawn to avoid inherited executable-write descriptors.
    static FIXTURE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    struct Fixture(PathBuf);
    impl Fixture {
        fn new(body: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "hmi-engine-driver-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&dir).unwrap();
            let f = Self(dir);
            fs::write(f.exe(), format!("#!/bin/sh\n{body}\n")).unwrap();
            fs::set_permissions(f.exe(), fs::Permissions::from_mode(0o700)).unwrap();
            f
        }
        fn exe(&self) -> PathBuf {
            self.0.join("child")
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    const HELLO: &str = r#"read -r line; printf '%s\n' '{"type":"helloOk","id":1,"protocol":3,"engineVersion":"test","drivers":["loopback"]}'"#;

    fn single_project() -> hmi_project_model::model::Project {
        let mut p = project();
        p.connections.truncate(1);
        p.tags.retain(|t| t.id == tid("sim-count"));
        p.connections[0]
            .parameters
            .insert("timeoutMs".into(), serde_json::json!(2000));
        p
    }

    #[test]
    fn child_loss_marks_bad_and_reconnect_requires_new_baseline() {
        assert_fresh_baseline_after_fault("exit 0");
    }

    #[test]
    fn invalid_numeric_update_invalidates_generation_and_requires_fresh_baseline() {
        assert_fresh_baseline_after_fault(
            r#"printf '%s\n' '{"type":"values","values":[{"variableId":"sim-count","quality":"GOOD","value":{"type":"long","value":-1}}]}'"#,
        );
    }

    fn assert_fresh_baseline_after_fault(action: &str) {
        let _guard = FIXTURE_LOCK.lock().unwrap();
        let f = Fixture::new(&format!(
            r#"{HELLO}
dir=${{0%/*}}
read -r line; printf '%s\n' '{{"type":"ok","id":2}}'
read -r line; printf '%s\n' '{{"type":"ok","id":3}}'
printf '%s\n' '{{"type":"sourceState","sourceId":"isolated-loopback","state":"CONNECTED"}}'
if test -f "$dir/first"; then v=22; else v=11; touch "$dir/first"; fi
printf '%s\n' "{{\"type\":\"values\",\"values\":[{{\"variableId\":\"sim-count\",\"quality\":\"GOOD\",\"value\":{{\"type\":\"long\",\"value\":$v}}}}]}}"
if test "$v" = 11; then while ! test -f "$dir/lose"; do sleep 0.01; done; {action}; fi
exec sleep 60"#
        ));
        let p = single_project();
        let mut registry = DriverRegistry::new();
        registry.register(
            hmi_project_model::model::DriverKind::Simulator,
            Arc::new(EngineDriverFactory::new(f.exe())),
        );
        let (engine, report) = TagEngine::new(
            &p,
            &registry,
            Arc::new(ManualClock::new(START)),
            EngineConfig {
                scan_mode: ScanMode::Manual,
                ..EngineConfig::default()
            },
        );
        assert!(report.issues.is_empty());
        let tag = tid("sim-count");
        let mut tries = 0;
        loop {
            engine.poll_all();
            if engine.read(&tag).unwrap().quality.is_good() {
                break;
            }
            tries += 1;
            assert!(tries < 10);
        }
        assert_eq!(engine.read(&tag).unwrap().value, TagValue::UInt32(11));
        fs::write(f.0.join("lose"), "").unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            engine.poll_all();
            let s = engine.read(&tag).unwrap();
            if !s.quality.is_good() {
                assert_eq!(s.value, TagValue::UInt32(11)); // diagnostics only
                break;
            }
            assert!(Instant::now() < deadline, "generation fault not observed");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(matches!(
            engine.connection_status(&cid("conn-sim")),
            Some(ConnectionStatus::Faulted { .. })
        ));
        engine.poll_all();
        let s = engine.read(&tag).unwrap();
        assert!(s.quality.is_good());
        assert_eq!(s.value, TagValue::UInt32(22));
        engine.shutdown();
    }

    #[test]
    fn cancellation_interrupts_stalled_child_handshake() {
        let _guard = FIXTURE_LOCK.lock().unwrap();
        let f = Fixture::new(r#"read -r line; exec sleep 60"#);
        let p = single_project();
        let factory = EngineDriverFactory::new(f.exe());
        let types = TypeContext::from_project(&p);
        let mut driver = factory
            .create(&p.connections[0], &[&p.tags[0]], &types)
            .unwrap();
        let ctx = OpContext {
            timeout: Duration::from_secs(10),
            cancel: CancelToken::new(),
        };
        let cancel = ctx.cancel.clone();
        let thread = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            cancel.cancel();
        });
        let start = Instant::now();
        let result = driver.connect(&ctx);
        thread.join().unwrap();
        assert_eq!(result.unwrap_err().kind, DriverErrorKind::Cancelled);
        assert!(start.elapsed() < Duration::from_secs(2));
        assert!(!driver.is_connected());
    }
}
