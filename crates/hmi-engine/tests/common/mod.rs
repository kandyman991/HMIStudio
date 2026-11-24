//! Shared harness: drives the engine state machine through its request/message channels.
#![allow(dead_code)]

use std::time::Duration;

use hmi_engine::protocol::*;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::time::timeout;

pub struct Harness {
    tx: Option<UnboundedSender<Request>>,
    rx: UnboundedReceiver<Message>,
    next_id: u64,
    pub seen: Vec<Message>,
    /// Every message ever received (never cleared).
    pub all: Vec<Message>,
    /// Latest value per variable, from every message received.
    pub latest: std::collections::HashMap<String, PointValue>,
    engine: tokio::task::JoinHandle<()>,
}

impl Harness {
    pub async fn new() -> Self {
        let (tx, requests) = unbounded_channel();
        let (out, rx) = unbounded_channel();
        let engine = tokio::spawn(hmi_engine::engine::run(requests, out));
        let mut harness = Harness {
            tx: Some(tx),
            rx,
            next_id: 1,
            seen: Vec::new(),
            all: Vec::new(),
            latest: Default::default(),
            engine,
        };
        let id = harness.send(|id| Request::Hello {
            id,
            protocol: PROTOCOL_VERSION,
        });
        assert!(matches!(harness.reply(id).await, Message::HelloOk { .. }));
        harness
    }

    pub fn send(&mut self, make: impl FnOnce(u64) -> Request) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.tx.as_ref().unwrap().send(make(id)).unwrap();
        id
    }

    pub async fn next(&mut self) -> Message {
        let message = match timeout(Duration::from_secs(20), self.rx.recv()).await {
            Ok(m) => m.expect("engine ended"),
            Err(_) => panic!(
                "engine went quiet; last messages: {:?}",
                self.all.iter().rev().take(6).collect::<Vec<_>>()
            ),
        };
        self.seen.push(message.clone());
        self.all.push(message.clone());
        if let Message::Values { values } = &message {
            for value in values {
                self.latest.insert(value.variable_id.clone(), value.clone());
            }
        }
        message
    }

    pub async fn reply(&mut self, id: u64) -> Message {
        loop {
            let message = self.next().await;
            let matches = match &message {
                Message::HelloOk { id: i, .. }
                | Message::Ok { id: i }
                | Message::Error { id: i, .. }
                | Message::WriteResult { id: i, .. }
                | Message::BrowseResult { id: i, .. } => *i == id,
                _ => false,
            };
            if matches {
                return message;
            }
        }
    }

    pub async fn ok(&mut self, make: impl FnOnce(u64) -> Request) {
        let id = self.send(make);
        let reply = self.reply(id).await;
        assert!(
            matches!(reply, Message::Ok { .. }),
            "expected ok, got {reply:?}"
        );
    }

    pub async fn wait_for(
        &mut self,
        what: &str,
        mut accept: impl FnMut(&Message) -> bool,
    ) -> Message {
        if let Some(found) = self.seen.iter().find(|m| accept(m)).cloned() {
            return found;
        }
        loop {
            let message = self.next().await;
            if accept(&message) {
                return message;
            }
            let _ = what;
        }
    }

    pub async fn connected(&mut self, source: &str) {
        self.wait_for("connected", |m| {
            matches!(m, Message::SourceState { source_id, state: SourceState::Connected, .. } if source_id == source)
        })
        .await;
    }

    /// The variable's current good value once it satisfies `accept` (the latest known value
    /// counts, so a value reported before this call is not missed).
    pub async fn value(&mut self, variable: &str, accept: impl Fn(&Value) -> bool) -> Value {
        let matches =
            |v: &PointValue| v.quality == Quality::Good && v.value.as_ref().is_some_and(&accept);
        if let Some(found) = self.latest.get(variable).filter(|v| matches(v)) {
            return found.value.clone().unwrap();
        }
        loop {
            if let Message::Values { values } = self.next().await {
                if let Some(found) = values
                    .iter()
                    .find(|v| v.variable_id == variable && matches(v))
                {
                    return found.value.clone().unwrap();
                }
            }
        }
    }

    pub async fn write(&mut self, variable: &str, value: Value) -> Message {
        let id = self.send(|id| Request::Write {
            id,
            variable_id: variable.into(),
            value,
            actor: "tester".into(),
        });
        self.reply(id).await
    }

    pub async fn shutdown(mut self) {
        let id = self.send(|id| Request::Shutdown { id });
        let _ = self.reply(id).await;
        self.tx = None;
        let _ = timeout(Duration::from_secs(10), self.engine).await;
    }
}

pub mod tempdir {
    pub struct Dir {
        pub path: std::path::PathBuf,
    }
    impl Dir {
        pub fn new() -> Self {
            use std::sync::atomic::{AtomicU32, Ordering};
            static N: AtomicU32 = AtomicU32::new(0);
            let path = std::env::temp_dir().join(format!(
                "hmi-engine-test-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&path).unwrap();
            Dir { path }
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}
