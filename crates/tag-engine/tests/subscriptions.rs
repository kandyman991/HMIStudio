//! Subscription lifecycle and change notifications.
#![allow(clippy::unwrap_used)]

mod common;

use common::*;
use hmi_tag_engine::*;

#[test]
fn subscribe_delivers_initial_state_then_changes() {
    let r = rig();
    let (sub, rx) = r
        .engine
        .subscribe(Some(&[tid("int-bool"), tid("int-real")]))
        .unwrap();
    assert_eq!(
        changed_tags(&drain(&rx)),
        ["int-bool", "int-real"],
        "initial snapshots, project order"
    );
    r.engine
        .write(&tid("int-bool"), TagValue::Bool(true))
        .unwrap();
    r.engine
        .write(&tid("int-text"), TagValue::String("x".into()))
        .unwrap(); // not subscribed
    let ev = drain(&rx);
    assert_eq!(changed_tags(&ev), ["int-bool"]);
    let TagEvent::Changed { snapshot } = &ev[0] else {
        panic!()
    };
    assert_eq!(snapshot.value, TagValue::Bool(true));
    assert!(sub.id() > 0);
}

#[test]
fn no_notification_without_change() {
    let r = connected();
    let (_sub, rx) = r.engine.subscribe(Some(&[tid("sim-count")])).unwrap();
    drain(&rx);
    r.engine.poll_all();
    r.engine.poll_all();
    assert!(drain(&rx).is_empty(), "same value and quality: no event");
    r.engine
        .write(&tid("int-bool"), TagValue::Bool(false))
        .unwrap(); // already false
    r.plc.set("DB1.Count", TagValue::UInt32(3));
    r.engine.poll_all();
    assert_eq!(changed_tags(&drain(&rx)), ["sim-count"]);
    // Quality-only change is an event.
    r.plc.set_online(false);
    r.engine.poll_all();
    let ev = drain(&rx);
    let TagEvent::Changed { snapshot } = &ev[0] else {
        panic!("{ev:?}")
    };
    assert_eq!(
        snapshot.quality,
        Quality::bad(QualityReason::ConnectionLost)
    );
    assert_eq!(snapshot.value, TagValue::UInt32(3));
}

#[test]
fn unfiltered_subscriptions_see_connection_status_and_global_order() {
    let r = rig();
    let (_sub, rx) = r.engine.subscribe(None).unwrap();
    let initial = drain(&rx);
    // Every readable tag once; write-only tags never.
    let tags = changed_tags(&initial);
    assert!(!tags.contains(&"int-wo".to_owned()) && !tags.contains(&"sim-cmd".to_owned()));
    assert_eq!(tags.len(), r.engine.tag_ids().len() - 2);
    r.engine.poll_all();
    let ev = drain(&rx);
    let statuses: Vec<&ConnectionStatus> = ev
        .iter()
        .filter_map(|e| match e {
            TagEvent::Connection { status, .. } => Some(status),
            _ => None,
        })
        .collect();
    assert_eq!(
        statuses,
        [&ConnectionStatus::Connecting, &ConnectionStatus::Connected]
    );
    let seqs: Vec<u64> = ev
        .iter()
        .filter_map(|e| match e {
            TagEvent::Changed { snapshot } => Some(snapshot.sequence),
            _ => None,
        })
        .collect();
    assert!(seqs.windows(2).all(|w| w[0] < w[1]), "{seqs:?}");
}

#[test]
fn dropping_the_subscription_or_receiver_unsubscribes() {
    let r = rig();
    let (sub, rx) = r.engine.subscribe(None).unwrap();
    let (_sub2, rx2) = r.engine.subscribe(Some(&[tid("int-bool")])).unwrap();
    assert_eq!(r.engine.subscription_count(), 2);
    drop(sub);
    assert_eq!(r.engine.subscription_count(), 1);
    drop(rx);
    // A dropped receiver is pruned on the next delivery attempt.
    drop(rx2);
    r.engine
        .write(&tid("int-bool"), TagValue::Bool(true))
        .unwrap();
    assert_eq!(r.engine.subscription_count(), 0);
    assert!(matches!(
        r.engine.subscribe(Some(&[tid("nope")])),
        Err(TagError::UnknownTag { .. })
    ));
}

#[test]
fn shutdown_closes_subscriptions_cleanly() {
    let r = connected();
    let (_sub, rx) = r
        .engine
        .subscribe(Some(&[tid("sim-speed"), tid("int-bool")]))
        .unwrap();
    drain(&rx);
    r.engine.shutdown();
    let ev: Vec<TagEvent> = rx.iter().collect(); // ends: senders dropped
    assert_eq!(ev.last(), Some(&TagEvent::Closed));
    let bad: Vec<_> = ev
        .iter()
        .filter_map(|e| match e {
            TagEvent::Changed { snapshot } => Some(snapshot.quality),
            _ => None,
        })
        .collect();
    assert_eq!(bad, [Quality::bad(QualityReason::Shutdown); 2]);
    assert_eq!(r.engine.subscription_count(), 0);
    assert!(matches!(r.engine.subscribe(None), Err(TagError::ShutDown)));
    assert!(matches!(
        r.engine.read(&tid("int-bool")),
        Err(TagError::ShutDown)
    ));
    assert!(matches!(
        r.engine.write(&tid("int-bool"), TagValue::Bool(true)),
        Err(TagError::ShutDown)
    ));
    assert_eq!(
        r.engine.connection_status(&cid("conn-sim")),
        Some(ConnectionStatus::Stopped)
    );
    assert_eq!(r.engine.poll_all(), 0);
    r.engine.shutdown(); // idempotent
}

#[test]
fn events_serialize_for_ipc() {
    let r = rig();
    let (_s, rx) = r.engine.subscribe(Some(&[tid("int-motor")])).unwrap();
    let ev = drain(&rx);
    let v = serde_json::to_value(&ev[0]).unwrap();
    assert_eq!(v["event"], "changed");
    assert_eq!(v["snapshot"]["tag"], "int-motor");
    assert_eq!(v["snapshot"]["quality"]["level"], "good");
    assert_eq!(
        v["snapshot"]["value"]["codes"],
        serde_json::json!([0, 0, 0, 0])
    );
    assert_eq!(v["snapshot"]["timestamp"], START.0);
}
