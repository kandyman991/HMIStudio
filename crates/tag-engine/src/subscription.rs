//! Bounded latest-state subscription mailbox. Lifecycle events are ordering barriers.
use crate::engine::TagEvent;
use std::collections::VecDeque;
use std::io::{self, Write};
use std::sync::mpsc::{RecvError, TryRecvError};
use std::sync::{Arc, Condvar, Mutex};

pub const MAX_PENDING: usize = 256;
pub const MAX_PENDING_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_SUBSCRIPTIONS: usize = 64;

#[derive(Debug, Default)]
struct State {
    queue: VecDeque<(TagEvent, usize)>,
    bytes: usize,
    closed: bool,
    receiver_gone: bool,
}

#[derive(Debug, Default)]
struct Mailbox {
    state: Mutex<State>,
    ready: Condvar,
}

// Count the serialized payload without allocating a second potentially large buffer.
struct BudgetWriter {
    remaining: usize,
}
impl Write for BudgetWriter {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.remaining = self
            .remaining
            .checked_sub(data.len())
            .ok_or_else(|| io::Error::other("subscription byte budget exceeded"))?;
        Ok(data.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn serialized_size(event: &TagEvent, limit: usize) -> Option<usize> {
    let mut writer = BudgetWriter { remaining: limit };
    serde_json::to_writer(&mut writer, event).ok()?;
    Some(limit - writer.remaining)
}

pub(crate) struct Sender(Arc<Mailbox>);

#[derive(Debug)]
pub struct EventReceiver(Arc<Mailbox>);

pub(crate) fn channel() -> (Sender, EventReceiver) {
    let m = Arc::new(Mailbox::default());
    (Sender(m.clone()), EventReceiver(m))
}

impl Sender {
    pub(crate) fn cancel(&self) {
        let mut s = self.0.state.lock().unwrap_or_else(|e| e.into_inner());
        s.queue.clear();
        s.bytes = 0;
        s.closed = true;
        self.0.ready.notify_all();
    }

    pub(crate) fn receiver_gone(&self) -> bool {
        self.0
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .receiver_gone
    }

    pub(crate) fn send(&self, event: TagEvent) -> Result<(), ()> {
        let mut s = self.0.state.lock().unwrap_or_else(|e| e.into_inner());
        if s.closed || s.receiver_gone {
            return Err(());
        }
        // Never coalesce across a connection or lifecycle event. Move replacement
        // to the tail, preserving the sequence order of retained changes.
        if let TagEvent::Changed { snapshot } = &event {
            let old = s
                .queue
                .iter()
                .enumerate()
                .rev()
                .take_while(|(_, (e, _))| matches!(e, TagEvent::Changed { .. }))
                .find_map(|(i, (e, _))| match e {
                    TagEvent::Changed { snapshot: prior } if prior.tag == snapshot.tag => Some(i),
                    _ => None,
                });
            if let Some(i) = old {
                if let Some((_, bytes)) = s.queue.remove(i) {
                    s.bytes -= bytes;
                }
            }
        }
        // Serialized byte accounting is a conservative wire budget, not a heap measurement.
        let size = serialized_size(&event, MAX_PENDING_BYTES - s.bytes).unwrap_or(usize::MAX);
        if s.queue.len() >= MAX_PENDING || size > MAX_PENDING_BYTES - s.bytes {
            s.queue.clear();
            s.bytes = 0;
            s.queue.push_back((TagEvent::ResyncRequired, 0));
            s.closed = true;
            self.0.ready.notify_all();
            return Err(());
        }
        if matches!(event, TagEvent::Closed) {
            s.closed = true;
        }
        s.bytes += size;
        s.queue.push_back((event, size));
        self.0.ready.notify_one();
        Ok(())
    }
}

impl Drop for Sender {
    fn drop(&mut self) {
        let mut s = self.0.state.lock().unwrap_or_else(|e| e.into_inner());
        s.closed = true;
        self.0.ready.notify_all();
    }
}

impl EventReceiver {
    pub fn try_recv(&self) -> Result<TagEvent, TryRecvError> {
        let mut s = self.0.state.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((e, bytes)) = s.queue.pop_front() {
            s.bytes -= bytes;
            Ok(e)
        } else if s.closed {
            Err(TryRecvError::Disconnected)
        } else {
            Err(TryRecvError::Empty)
        }
    }

    pub fn recv(&self) -> Result<TagEvent, RecvError> {
        let mut s = self.0.state.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if let Some((e, bytes)) = s.queue.pop_front() {
                s.bytes -= bytes;
                return Ok(e);
            }
            if s.closed {
                return Err(RecvError);
            }
            s = self.0.ready.wait(s).unwrap_or_else(|e| e.into_inner());
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = TagEvent> + '_ {
        std::iter::from_fn(|| self.recv().ok())
    }
}

impl Drop for EventReceiver {
    fn drop(&mut self) {
        let mut s = self.0.state.lock().unwrap_or_else(|e| e.into_inner());
        s.receiver_gone = true;
        s.queue.clear();
        s.bytes = 0;
    }
}
