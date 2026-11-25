use hmi_tag_engine::bridge_dispatch::{Dispatcher, Message, ReplyKind, Value, MAX_EVENTS};
use serde_json::{json, Value as Json};
use std::io;

fn frame(value: Json) -> io::Result<Vec<u8>> {
    Ok(serde_json::to_vec(&value)?)
}

#[test]
fn all_authorized_fixture_messages_round_trip() -> io::Result<()> {
    let fixture: Json =
        serde_json::from_str(include_str!("../../hmi-engine/protocol/messages.json"))?;
    for value in fixture["messages"]
        .as_array()
        .ok_or_else(|| io::Error::other("fixture messages missing"))?
    {
        let decoded: Message = serde_json::from_value(value.clone())?;
        assert_eq!(serde_json::to_value(decoded)?, *value);
    }
    Ok(())
}

#[test]
fn events_do_not_consume_request_and_ids_are_monotonic() -> io::Result<()> {
    let mut d = Dispatcher::default();
    assert_eq!(d.begin(ReplyKind::Ok)?, 1);
    assert_eq!(
        d.begin(ReplyKind::Browse).expect_err("busy").kind(),
        io::ErrorKind::WouldBlock
    );
    let event = frame(
        json!({"type":"values","values":[{"variableId":"total","quality":"GOOD","value":{"type":"long","value":9223372036854775807_i64}}]}),
    )?;
    assert!(d.accept(&event)?.is_none());
    assert!(matches!(
        d.accept(&frame(json!({"type":"ok","id":1}))?)?,
        Some(Message::Ok { id: 1 })
    ));
    assert!(
        matches!(d.pop_event(), Some(Message::Values { values }) if values[0].value == Some(Value::Long(i64::MAX)))
    );
    assert_eq!(d.begin(ReplyKind::Browse)?, 2);
    assert!(matches!(
        d.accept(&frame(
            json!({"type":"error","id":2,"code":"NOT_STARTED","message":"not started"})
        )?)?,
        Some(Message::Error { id: 2, .. })
    ));
    Ok(())
}

#[test]
fn wrong_id_wrong_kind_duplicate_and_malformed_invalidate() -> io::Result<()> {
    for invalid in [
        json!({"type":"ok","id":2}),
        json!({"type":"browseResult","id":1,"nodes":[]}),
        json!({"type":"ok","id":1,"unexpected":true}),
        json!({"type":"unknown"}),
    ] {
        let mut d = Dispatcher::default();
        d.begin(ReplyKind::Ok)?;
        assert!(d.accept(&frame(invalid)?).is_err());
        assert!(d.requires_resync());
        assert!(d.begin(ReplyKind::Ok).is_err());
    }
    let mut d = Dispatcher::default();
    d.begin(ReplyKind::Ok)?;
    let reply = frame(json!({"type":"ok","id":1}))?;
    assert!(d.accept(&reply)?.is_some());
    assert!(d.accept(&reply).is_err());
    assert!(d.requires_resync());
    Ok(())
}

#[test]
fn overflow_invalidates_and_discards_backlog() -> io::Result<()> {
    let mut d = Dispatcher::default();
    let event = frame(json!({"type":"sourceState","sourceId":"sim","state":"CONNECTED"}))?;
    for _ in 0..MAX_EVENTS {
        assert!(d.accept(&event)?.is_none());
    }
    assert!(d.accept(&event).is_err());
    assert!(d.requires_resync());
    assert!(d.pop_event().is_none());
    assert!(d.begin(ReplyKind::Ok).is_err());
    Ok(())
}

#[test]
fn byte_budget_also_bounds_event_backlog() -> io::Result<()> {
    let mut d = Dispatcher::default();
    let event = frame(
        json!({"type":"sourceState","sourceId":"sim","state":"CONNECTED","message":"x".repeat(900_000)}),
    )?;
    for _ in 0..4 {
        assert!(d.accept(&event)?.is_none());
    }
    assert!(d.accept(&event).is_err());
    assert!(d.requires_resync());
    assert!(d.pop_event().is_none());
    Ok(())
}
