//! Single-session child supervision. No automatic restart or request replay.
use crate::bridge_frame::{read_frame, write_frame, MAX_FRAME_BYTES};
use serde_json::{json, Value};
use std::io::{self, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread;
use std::time::Duration;

/// A successfully handshaken child. Dropping it invalidates the session.
pub struct Supervisor {
    child: Child,
    outgoing: Option<SyncSender<Vec<u8>>>,
    incoming: Receiver<io::Result<Vec<u8>>>,
    valid: bool,
}

impl Supervisor {
    /// Launch an explicitly selected absolute executable, without a shell.
    pub fn start(executable: &Path, timeout: Duration) -> io::Result<Self> {
        if !executable.is_absolute() || timeout.is_zero() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "absolute executable and positive timeout required",
            ));
        }
        let mut child = Command::new(executable)
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
        let (replies, rx) = mpsc::sync_channel(8);
        let errors = replies.clone();
        thread::spawn(move || {
            while let Ok(frame) = writes.recv() {
                if let Err(error) = write_frame(&mut stdin, &frame) {
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
                            break;
                        }
                    }
                    Ok(None) => break,
                    Err(error) => {
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
        };
        let hello = serde_json::to_vec(&json!({"type":"hello","id":1,"protocol":3}))?;
        let response = session.exchange(hello, timeout)?;
        if response.get("type").and_then(Value::as_str) != Some("helloOk")
            || response.get("id").and_then(Value::as_u64) != Some(1)
            || response.get("protocol").and_then(Value::as_u64) != Some(3)
        {
            session.invalidate()?;
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "protocol handshake rejected",
            ));
        }
        Ok(session)
    }

    // Private until request correlation and typed event dispatch are implemented.
    fn exchange(&mut self, frame: Vec<u8>, timeout: Duration) -> io::Result<Value> {
        let result = (|| {
            if !self.valid || self.child.try_wait()?.is_some() {
                return Err(io::Error::new(io::ErrorKind::BrokenPipe, "session closed"));
            }
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
            let reply = self.incoming.recv_timeout(timeout).map_err(|e| match e {
                mpsc::RecvTimeoutError::Timeout => {
                    io::Error::new(io::ErrorKind::TimedOut, "engine reply timeout")
                }
                mpsc::RecvTimeoutError::Disconnected => {
                    io::Error::new(io::ErrorKind::BrokenPipe, "engine output closed")
                }
            })??;
            serde_json::from_slice(&reply).map_err(io::Error::other)
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
        if self.child.try_wait()?.is_some() {
            self.invalidate()?;
            return Ok(false);
        }
        Ok(true)
    }

    /// Invalidate before terminating and reaping the direct child.
    pub fn invalidate(&mut self) -> io::Result<()> {
        self.valid = false;
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
