//! Linux-only controlled subprocess tests; no PLC or network access.
#![cfg(unix)]
use hmi_tag_engine::bridge_supervisor::Supervisor;
use std::{
    fs, io,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new(body: &str) -> io::Result<Self> {
        let dir = std::env::temp_dir().join(format!(
            "hmi-supervisor-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir)?;
        let fixture = Self(dir);
        let path = fixture.path();
        fs::write(&path, format!("#!/bin/sh\n{body}\n"))?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
        Ok(fixture)
    }
    fn path(&self) -> PathBuf {
        self.0.join("child")
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
/// Starts the supervisor, retrying only `ExecutableFileBusy` (Linux ETXTBSY). Tests run in
/// parallel: a fork in another test thread can briefly inherit this fixture's still-open write
/// descriptor until that child execs, and exec of a file open for writing fails. No child is
/// started when this happens, so a retry cannot hide a supervisor fault.
fn start_supervisor(path: &std::path::Path, timeout: Duration) -> io::Result<Supervisor> {
    let mut attempts = 0;
    loop {
        match Supervisor::start(path, timeout) {
            Err(e) if e.kind() == io::ErrorKind::ExecutableFileBusy && attempts < 50 => {
                attempts += 1;
                std::thread::sleep(Duration::from_millis(10));
            }
            other => return other,
        }
    }
}
const HELLO: &str = "read -r line; printf '%s\\n' '{\"type\":\"helloOk\",\"id\":1,\"protocol\":3,\"engineVersion\":\"test\",\"drivers\":[]}'";
fn rejected(body: &str, expected: Option<io::ErrorKind>) -> io::Result<()> {
    let fixture = Fixture::new(body)?;
    let start = Instant::now();
    let result = start_supervisor(&fixture.path(), Duration::from_millis(500));
    let error = match result {
        Ok(_) => return Err(io::Error::other("unexpected handshake success")),
        Err(e) => e,
    };
    if let Some(kind) = expected {
        assert_eq!(error.kind(), kind);
    }
    assert!(start.elapsed() < Duration::from_secs(5));
    Ok(())
}
#[test]
fn crash_before_handshake() -> io::Result<()> {
    rejected("exit 17", None)
}
#[test]
fn hung_handshake() -> io::Result<()> {
    rejected("read -r line; exec sleep 60", Some(io::ErrorKind::TimedOut))
}
#[test]
fn malformed_handshake() -> io::Result<()> {
    rejected("read -r line; printf 'not-json\\n'", None)
}
#[test]
fn wrong_protocol() -> io::Result<()> {
    rejected(
        "read -r line; printf '%s\\n' '{\"type\":\"helloOk\",\"id\":1,\"protocol\":2}'",
        Some(io::ErrorKind::InvalidData),
    )
}
#[test]
fn wrong_request_id() -> io::Result<()> {
    rejected(
        "read -r line; printf '%s\\n' '{\"type\":\"helloOk\",\"id\":2,\"protocol\":3}'",
        Some(io::ErrorKind::InvalidData),
    )
}
#[test]
fn explicit_rejection() -> io::Result<()> {
    rejected("read -r line; printf '%s\\n' '{\"type\":\"error\",\"id\":1,\"code\":\"VERSION\",\"message\":\"rejected\"}'", Some(io::ErrorKind::InvalidData))
}
#[test]
fn successful_handshake_and_invalidation() -> io::Result<()> {
    let fixture = Fixture::new(&format!("{HELLO}; exec sleep 60"))?;
    let mut supervisor = start_supervisor(&fixture.path(), Duration::from_secs(2))?;
    assert!(supervisor.is_alive()?);
    supervisor.invalidate()?;
    assert!(!supervisor.is_alive()?);
    supervisor.invalidate()?;
    Ok(())
}
#[test]
fn exit_after_handshake() -> io::Result<()> {
    let fixture = Fixture::new(HELLO)?;
    let mut supervisor = start_supervisor(&fixture.path(), Duration::from_secs(2))?;
    let start = Instant::now();
    while supervisor.is_alive()? {
        assert!(start.elapsed() < Duration::from_secs(3));
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!supervisor.is_alive()?);
    Ok(())
}

#[test]
fn correlated_browse_skips_interleaved_events() -> io::Result<()> {
    use hmi_tag_engine::bridge_dispatch::Message;
    let fixture = Fixture::new(&format!(
        r#"{HELLO}; read -r line; printf '%s\n' '{{"type":"sourceState","sourceId":"sim","state":"CONNECTED"}}' '{{"type":"browseResult","id":2,"nodes":[]}}'; exec sleep 60"#
    ))?;
    let mut s = start_supervisor(&fixture.path(), Duration::from_secs(2))?;
    assert!(matches!(
        s.browse("sim", None, Duration::from_secs(2))?,
        Message::BrowseResult { id: 2, .. }
    ));
    assert!(matches!(s.poll_event()?, Some(Message::SourceState { .. })));
    Ok(())
}

#[test]
fn stale_response_invalidates_session() -> io::Result<()> {
    let fixture = Fixture::new(&format!(
        r#"{HELLO}; read -r line; printf '%s\n' '{{"type":"browseResult","id":1,"nodes":[]}}'; exec sleep 60"#
    ))?;
    let mut s = start_supervisor(&fixture.path(), Duration::from_secs(2))?;
    assert!(s.browse("sim", None, Duration::from_secs(2)).is_err());
    assert!(!s.is_alive()?);
    assert!(s.browse("sim", None, Duration::from_secs(2)).is_err());
    Ok(())
}

#[test]
fn browse_timeout_invalidates_and_never_replays() -> io::Result<()> {
    let fixture = Fixture::new(&format!("{HELLO}; read -r line; exec sleep 60"))?;
    let mut s = start_supervisor(&fixture.path(), Duration::from_secs(2))?;
    let err = s
        .browse("sim", None, Duration::from_millis(100))
        .expect_err("no response");
    assert_eq!(err.kind(), io::ErrorKind::TimedOut);
    assert!(!s.is_alive()?);
    assert!(s.browse("sim", None, Duration::from_secs(2)).is_err());
    Ok(())
}

#[test]
fn closed_stdout_invalidates_even_if_child_remains_alive() -> io::Result<()> {
    let fixture = Fixture::new(&format!("{HELLO}; read -r line; exec 1>&-; exec sleep 60"))?;
    let mut s = start_supervisor(&fixture.path(), Duration::from_secs(2))?;
    assert!(s.browse("sim", None, Duration::from_millis(100)).is_err());
    assert!(!s.is_alive()?);
    Ok(())
}
