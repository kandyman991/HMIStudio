//! Single-session child supervision. No automatic restart or request replay.
use crate::bridge_dispatch::{Dispatcher, Message, ReplyKind};
use crate::bridge_frame::{read_frame, write_frame, MAX_FRAME_BYTES};
use crate::driver::CancelToken;
use serde_json::{json, Value};
use std::io::{self, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::{Duration, Instant};

/// A successfully handshaken child. Dropping it invalidates the session.
pub struct Supervisor {
    child: Child,
    outgoing: Option<SyncSender<Vec<u8>>>,
    incoming: Receiver<io::Result<Vec<u8>>>,
    valid: bool,
    dispatch: Dispatcher,
    transport_failed: Arc<AtomicBool>,
    reader_closed: Arc<AtomicBool>,
}

impl Supervisor {
    /// Launch an explicitly selected absolute executable, without a shell.
    pub fn start(executable: &Path, timeout: Duration) -> io::Result<Self> {
        Self::start_cancellable(executable, timeout, &CancelToken::new())
    }

    pub(crate) fn start_cancellable(
        executable: &Path,
        timeout: Duration,
        cancel: &CancelToken,
    ) -> io::Result<Self> {
        Self::launch(executable, timeout, cancel, false)
    }

    pub(crate) fn start_isolated(executable: &Path, timeout: Duration) -> io::Result<Self> {
        Self::launch(executable, timeout, &CancelToken::new(), true)
    }

    fn launch(
        executable: &Path,
        timeout: Duration,
        cancel: &CancelToken,
        isolated: bool,
    ) -> io::Result<Self> {
        check_cancel(cancel)?;
        if !executable.is_absolute() || timeout.is_zero() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "absolute executable and positive timeout required",
            ));
        }
        let mut command = Command::new(executable);
        if isolated {
            command.arg("--isolated");
        }
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let (Some(mut stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(io::Error::other("missing child pipes"));
        };
        let (tx, writes) = mpsc::sync_channel::<Vec<u8>>(1);
        let (replies, rx) = mpsc::sync_channel(if isolated { 32 } else { 8 });
        let errors = replies.clone();
        let transport_failed = Arc::new(AtomicBool::new(false));
        let writer_failed = transport_failed.clone();
        let reader_failed = transport_failed.clone();
        let reader_closed = Arc::new(AtomicBool::new(false));
        let closed = reader_closed.clone();
        thread::spawn(move || {
            while let Ok(frame) = writes.recv() {
                if let Err(error) = write_frame(&mut stdin, &frame) {
                    writer_failed.store(true, Ordering::Release);
                    let _ = errors.try_send(Err(error));
                    break;
                }
            }
        });
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                match read_frame(&mut reader) {
                    Ok(Some(frame)) => {
                        if replies.try_send(Ok(frame)).is_err() {
                            reader_failed.store(true, Ordering::Release);
                            break;
                        }
                    }
                    Ok(None) => {
                        closed.store(true, Ordering::Release);
                        break;
                    }
                    Err(error) => {
                        reader_failed.store(true, Ordering::Release);
                        let _ = replies.try_send(Err(error));
                        break;
                    }
                }
            }
        });
        let mut session = Self {
            child,
            outgoing: Some(tx),
            incoming: rx,
            valid: true,
            dispatch: Dispatcher::default(),
            transport_failed,
            reader_closed,
        };
        let response = session.exchange(
            json!({"type":"hello","protocol":3}),
            ReplyKind::Hello,
            timeout,
            cancel,
        )?;
        if !matches!(response, Message::HelloOk { protocol: 3, drivers, .. }
            if !isolated || drivers == ["loopback"])
        {
            session.invalidate()?;
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "protocol handshake rejected",
            ));
        }
        Ok(session)
    }

    pub(crate) fn configure_and_start(
        &mut self,
        config: Value,
        deadline: Instant,
        cancel: &CancelToken,
    ) -> io::Result<()> {
        let remaining = || {
            deadline
                .checked_duration_since(Instant::now())
                .filter(|d| !d.is_zero())
                .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "baseline timeout"))
        };
        // The engine acknowledges start before emitting loopback events.
        if self.poll_event()?.is_some()
            || !matches!(
                self.exchange(config, ReplyKind::Ok, remaining()?, cancel)?,
                Message::Ok { .. }
            )
            || self.dispatch.pop_event().is_some()
            || !matches!(
                self.exchange(json!({"type":"start"}), ReplyKind::Ok, remaining()?, cancel)?,
                Message::Ok { .. }
            )
            || self.dispatch.pop_event().is_some()
        {
            self.invalidate()?;
            return Err(io::Error::other(
                "loopback configuration/start rejected or premature event",
            ));
        }
        Ok(())
    }

    pub(crate) fn simulation_command(
        &mut self,
        request: Value,
        timeout: Duration,
    ) -> io::Result<Message> {
        self.exchange(request, ReplyKind::Write, timeout, &CancelToken::new())
    }

    // No generic request API or write-capable connection is exposed.
    fn exchange(
        &mut self,
        mut request: Value,
        kind: ReplyKind,
        timeout: Duration,
        cancel: &CancelToken,
    ) -> io::Result<Message> {
        let result = (|| {
            check_cancel(cancel)?;
            let deadline = Instant::now()
                .checked_add(timeout)
                .filter(|_| !timeout.is_zero())
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "invalid reply timeout")
                })?;
            if !self.valid
                || self.transport_failed.load(Ordering::Acquire)
                || self.reader_closed.load(Ordering::Acquire)
                || self.child.try_wait()?.is_some()
            {
                return Err(io::Error::new(io::ErrorKind::BrokenPipe, "session closed"));
            }
            let id = self.dispatch.begin(kind)?;
            request["id"] = json!(id);
            let frame = serde_json::to_vec(&request)?;
            if frame.len() > MAX_FRAME_BYTES {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "oversized request",
                ));
            }
            self.outgoing
                .as_ref()
                .ok_or_else(|| io::Error::other("session closed"))?
                .try_send(frame)
                .map_err(|_| io::Error::other("request queue unavailable"))?;
            loop {
                check_cancel(cancel)?;
                let remaining = deadline
                    .checked_duration_since(Instant::now())
                    .filter(|time| !time.is_zero())
                    .ok_or_else(|| {
                        io::Error::new(io::ErrorKind::TimedOut, "engine reply timeout")
                    })?;
                let frame = match self
                    .incoming
                    .recv_timeout(remaining.min(Duration::from_millis(10)))
                {
                    Ok(frame) => frame?,
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        if self.reader_closed.load(Ordering::Acquire)
                            || self.transport_failed.load(Ordering::Acquire)
                            || self.child.try_wait()?.is_some()
                        {
                            return Err(io::Error::new(
                                io::ErrorKind::BrokenPipe,
                                "engine output closed",
                            ));
                        }
                        continue;
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        return Err(io::Error::new(
                            io::ErrorKind::BrokenPipe,
                            "engine output closed",
                        ));
                    }
                };
                check_cancel(cancel)?;
                if self.transport_failed.load(Ordering::Acquire) {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "transport failed; resynchronization required",
                    ));
                }
                if let Some(reply) = self.dispatch.accept(&frame)? {
                    return Ok(reply);
                }
            }
        })();
        if result.is_err() {
            self.invalidate()?;
        }
        result
    }

    // Only the isolated loopback wrapper may configure/start. No arbitrary endpoint,
    // credentials or write operation is exposed through this API.
    pub(crate) fn start_loopback(
        &mut self,
        points: &[crate::bridge_simulation::SimPoint],
        deadline: Instant,
        cancel: &CancelToken,
    ) -> io::Result<()> {
        let config_points: Vec<_> = points
            .iter()
            .map(|p| {
                json!({
                    "mappingId": p.id, "variableId": p.id, "sourceId": "isolated-loopback",
                    "address": p.id, "valueType": p.kind, "access": "READ", "writeConfirmed": false
                })
            })
            .collect();
        let config = json!({"type":"configure","config":{"version":1,
            "sources":[{"id":"isolated-loopback","driver":"loopback","settings":{}}],
            "points":config_points}});
        self.configure_and_start(config, deadline, cancel)
    }

    /// Read-only protocol operation, for isolated fixture testing. The supervisor does
    /// not expose arbitrary configure/start or write commands.
    pub fn browse(
        &mut self,
        source_id: &str,
        node_id: Option<&str>,
        timeout: Duration,
    ) -> io::Result<Message> {
        let mut request = json!({"type":"browse","sourceId":source_id});
        if let Some(node) = node_id {
            request["nodeId"] = json!(node);
        }
        self.exchange(request, ReplyKind::Browse, timeout, &CancelToken::new())
    }

    /// Drain available events without waiting. An unexpected response or transport
    /// failure invalidates the session and discards the partial event backlog.
    pub fn poll_event(&mut self) -> io::Result<Option<Message>> {
        let result = (|| {
            if !self.valid
                || self.transport_failed.load(Ordering::Acquire)
                || self.reader_closed.load(Ordering::Acquire)
                || self.child.try_wait()?.is_some()
            {
                return Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "session invalid; resynchronization required",
                ));
            }
            for _ in 0..8 {
                match self.incoming.try_recv() {
                    Ok(frame) => {
                        self.dispatch.accept(&frame?)?;
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        return Err(io::Error::new(
                            io::ErrorKind::BrokenPipe,
                            "engine output closed",
                        ))
                    }
                }
            }
            if self.transport_failed.load(Ordering::Acquire) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "transport failed; resynchronization required",
                ));
            }
            Ok(self.dispatch.pop_event())
        })();
        if result.is_err() {
            self.invalidate()?;
        }
        result
    }

    /// Check whether the child is still alive; an exited child cannot be reused.
    pub fn is_alive(&mut self) -> io::Result<bool> {
        if !self.valid {
            return Ok(false);
        }
        if self.transport_failed.load(Ordering::Acquire)
            || self.reader_closed.load(Ordering::Acquire)
            || self.child.try_wait()?.is_some()
        {
            self.invalidate()?;
            return Ok(false);
        }
        Ok(true)
    }

    /// Invalidate before terminating and reaping the direct child.
    pub fn invalidate(&mut self) -> io::Result<()> {
        self.valid = false;
        self.dispatch.invalidate();
        self.outgoing.take();
        if self.child.try_wait()?.is_none() {
            if let Err(error) = self.child.kill() {
                if self.child.try_wait()?.is_none() {
                    return Err(error);
                }
            }
        }
        self.child.wait()?;
        Ok(())
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        let _ = self.invalidate();
    }
}

pub(crate) fn check_cancel(cancel: &CancelToken) -> io::Result<()> {
    if cancel.is_cancelled() {
        Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "operation cancelled",
        ))
    } else {
        Ok(())
    }
}
