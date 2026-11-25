//! Thread safety, threaded scanning, clean shutdown and regressions.
#![allow(clippy::unwrap_used)]

mod common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use common::*;
use hmi_tag_engine::*;
use serde_json::json;

fn motor(i: u16) -> TagValue {
    TagValue::Struct(
        [
            ("running".to_owned(), TagValue::Bool(i.is_multiple_of(2))),
            ("speed".to_owned(), TagValue::Float32(f32::from(i))),
            (
                "codes".to_owned(),
                TagValue::Array(vec![TagValue::UInt16(i); 4]),
            ),
        ]
        .into_iter()
        .collect(),
    )
}

fn consistent(v: &TagValue) -> bool {
    let TagValue::Struct(m) = v else { return false };
    let TagValue::Float32(s) = m["speed"] else {
        return false;
    };
    let TagValue::Array(codes) = &m["codes"] else {
        return false;
    };
    codes.iter().all(|c| c.as_f64() == Some(f64::from(s)))
}

#[test]
fn concurrent_reads_and_writes_never_observe_torn_values() {
    let r = Arc::new(rig());
    let (_sub, rx) = r
        .engine
        .subscribe(Some(&[tid("int-motor"), tid("int-real")]))
        .unwrap();
    drain(&rx);
    let mut handles = Vec::new();
    for w in 0..4u16 {
        let r = r.clone();
        handles.push(std::thread::spawn(move || {
            for i in 0..250u16 {
                r.engine
                    .write(&tid("int-motor"), motor(w * 1000 + i))
                    .unwrap();
                r.engine
                    .write(&tid("int-real"), TagValue::Float32(f32::from(i % 100)))
                    .unwrap();
            }
        }));
    }
    for _ in 0..4 {
        let r = r.clone();
        handles.push(std::thread::spawn(move || {
            for _ in 0..500 {
                let v = r.engine.read(&tid("int-motor")).unwrap().value;
                assert!(consistent(&v), "torn struct {v}");
                r.engine.read(&tid("int-real")).unwrap();
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    let ev = drain(&rx);
    let seqs: Vec<u64> = ev
        .iter()
        .filter_map(|e| match e {
            TagEvent::Changed { snapshot } => {
                if snapshot.tag.as_str() == "int-motor" {
                    assert!(consistent(&snapshot.value));
                }
                Some(snapshot.sequence)
            }
            _ => None,
        })
        .collect();
    assert!(
        seqs.windows(2).all(|w| w[0] < w[1]),
        "notifications in global order"
    );
    // The consumer deliberately did not drain during the burst. Latest-state
    // delivery keeps one final value per tag, not a historical audit log.
    assert_eq!(seqs.len(), 2);
    for e in ev {
        if let TagEvent::Changed { snapshot } = e {
            assert_eq!(snapshot, r.engine.read(&snapshot.tag).unwrap());
        }
    }
}

#[test]
fn concurrent_member_writes_are_atomic_read_modify_write() {
    let r = Arc::new(rig());
    let mut hs = Vec::new();
    // Each thread owns one array element; no update may be lost.
    for k in 0..4u32 {
        let r = r.clone();
        hs.push(std::thread::spawn(move || {
            for i in 0..200u16 {
                r.engine
                    .write_member(
                        &tid("int-motor"),
                        &MemberPath::parse(&format!("codes[{k}]")).unwrap(),
                        TagValue::UInt16(i),
                    )
                    .unwrap();
            }
        }));
    }
    for h in hs {
        h.join().unwrap();
    }
    assert_eq!(
        r.engine.read(&tid("int-motor")).unwrap().value.to_json()["codes"],
        json!([199, 199, 199, 199])
    );
}

#[test]
fn threaded_scanning_tracks_the_plc_and_writes_interleave_with_polls() {
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let hub = SimulatorHub::new(clock.clone());
    let mut v = project_json();
    v["connections"][0]["parameters"] =
        json!({ "timeoutMs": 500, "reconnectMs": 10, "defaultScanMs": 20 });
    for t in v["tags"].as_array_mut().unwrap() {
        if t.get("scanRateMs").is_some() {
            t["scanRateMs"] = json!(10);
        }
    }
    let p = project_from(v);
    let (engine, _) = TagEngine::new(
        &p,
        &simulator_registry(&hub),
        clock,
        EngineConfig {
            scan_mode: ScanMode::Threaded,
            max_idle_ms: 5,
            ..EngineConfig::default()
        },
    );
    let engine = Arc::new(engine);
    let plc = hub.plc("conn-sim");
    plc.set_latency(Duration::from_millis(2));
    let wait_for = |pred: &dyn Fn() -> bool| {
        let t0 = Instant::now();
        while !pred() {
            assert!(t0.elapsed() < Duration::from_secs(10), "timed out waiting");
            std::thread::sleep(Duration::from_millis(5));
        }
    };
    wait_for(&|| engine.connection_status(&cid("conn-sim")) == Some(ConnectionStatus::Connected));
    plc.set("DB1.Count", TagValue::UInt32(42));
    wait_for(&|| engine.read(&tid("sim-count")).unwrap().value == TagValue::UInt32(42));
    // Writers from several threads while the scanner polls.
    let mut hs = Vec::new();
    for w in 0..3 {
        let e = engine.clone();
        hs.push(std::thread::spawn(move || {
            for i in 0..30u32 {
                e.write(&tid("sim-speed"), TagValue::Float32((w * 100 + i) as f32))
                    .unwrap();
                e.write_json(
                    &tid("sim-motor"),
                    Some("running"),
                    &json!(i.is_multiple_of(2)),
                )
                .unwrap();
            }
        }));
    }
    for h in hs {
        h.join().unwrap();
    }
    // Connection loss and recovery in threaded mode.
    plc.set_online(false);
    wait_for(&|| {
        engine.read(&tid("sim-count")).unwrap().quality
            == Quality::bad(QualityReason::ConnectionLost)
    });
    plc.set_online(true);
    wait_for(&|| engine.read(&tid("sim-count")).unwrap().quality.is_good());
    // Clean, bounded shutdown with the scanner mid-flight.
    plc.set_latency(Duration::from_millis(400));
    std::thread::sleep(Duration::from_millis(30));
    let t0 = Instant::now();
    let e2 = engine.clone();
    within(10, move || e2.shutdown());
    assert!(
        t0.elapsed() < Duration::from_secs(5),
        "shutdown took {:?}",
        t0.elapsed()
    );
    assert_eq!(
        engine.connection_status(&cid("conn-sim")),
        Some(ConnectionStatus::Stopped)
    );
    assert!(matches!(
        engine.read(&tid("sim-count")),
        Err(TagError::ShutDown)
    ));
}

/// Regression (found in self-review before the first commit): the scan
/// thread's wake-time computation took the status lock and then the driver
/// lock, the reverse of the documented order, which could deadlock against
/// a writer holding the driver lock while publishing a status change.
#[test]
fn regression_no_deadlock_between_scanner_and_writers_during_reconnects() {
    within(30, || {
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);
        let hub = SimulatorHub::new(clock.clone());
        let mut v = project_json();
        v["connections"][0]["parameters"] =
            json!({ "timeoutMs": 50, "reconnectMs": 1, "defaultScanMs": 1 });
        let p = project_from(v);
        let (engine, _) = TagEngine::new(
            &p,
            &simulator_registry(&hub),
            clock,
            EngineConfig {
                scan_mode: ScanMode::Threaded,
                max_idle_ms: 1,
                ..EngineConfig::default()
            },
        );
        let engine = Arc::new(engine);
        let plc = hub.plc("conn-sim");
        let (_sub, rx) = engine.subscribe(None).unwrap();
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut hs = Vec::new();
        for _ in 0..3 {
            let e = engine.clone();
            let stop = stop.clone();
            hs.push(std::thread::spawn(move || {
                let mut n = 0u32;
                while !stop.load(std::sync::atomic::Ordering::SeqCst) {
                    let _ = e.write(&tid("sim-speed"), TagValue::Float32((n % 500) as f32));
                    let _ = e.read_device(&[tid("sim-count")]);
                    n += 1;
                }
            }));
        }
        // Flap the connection so status changes race with writes.
        for i in 0..200u32 {
            plc.set_online(i.is_multiple_of(2));
            std::thread::sleep(Duration::from_millis(1));
            while rx.try_recv().is_ok() {}
        }
        plc.set_online(true);
        stop.store(true, std::sync::atomic::Ordering::SeqCst);
        for h in hs {
            h.join().unwrap();
        }
        engine.shutdown();
    });
}

#[test]
fn drop_shuts_down_and_ends_subscriptions() {
    let r = connected();
    let (_sub, rx) = r.engine.subscribe(Some(&[tid("int-bool")])).unwrap();
    drain(&rx);
    drop(r);
    let ev: Vec<TagEvent> = within(5, move || rx.iter().collect());
    assert_eq!(ev.last(), Some(&TagEvent::Closed));
}

#[test]
fn engine_is_send_and_sync() {
    fn check<T: Send + Sync>() {}
    check::<TagEngine>();
    check::<SimPlc>();
    check::<SimulatorHub>();
}

#[test]
fn regression_internal_write_waiting_at_shutdown_is_refused() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Barrier;

    struct PausingClock {
        armed: AtomicBool,
        entered: Barrier,
        release: Barrier,
    }
    impl Clock for PausingClock {
        fn now(&self) -> Timestamp {
            if self.armed.swap(false, Ordering::SeqCst) {
                self.entered.wait();
                self.release.wait();
            }
            START
        }
    }
    within(10, || {
        let clock = Arc::new(PausingClock {
            armed: AtomicBool::new(false),
            entered: Barrier::new(2),
            release: Barrier::new(2),
        });
        let hub = SimulatorHub::new(clock.clone());
        let (engine, _) = TagEngine::new(
            &project(),
            &simulator_registry(&hub),
            clock.clone(),
            EngineConfig {
                scan_mode: ScanMode::Manual,
                ..EngineConfig::default()
            },
        );
        let engine = Arc::new(engine);
        clock.armed.store(true, Ordering::SeqCst);
        let writer = engine.clone();
        let handle =
            std::thread::spawn(move || writer.write(&tid("int-bool"), TagValue::Bool(true)));
        clock.entered.wait();
        engine.shutdown();
        clock.release.wait();
        assert!(matches!(handle.join().unwrap(), Err(TagError::ShutDown)));
    });
}

#[test]
fn regression_cancel_token_huge_duration_does_not_overflow() {
    let token = CancelToken::new();
    token.cancel();
    assert!(token.wait(Duration::MAX));
}
