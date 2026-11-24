//! A test PLC for the S7 driver, built on the independent `snap7-server` crate (not on the
//! client library the driver uses), so client and server bugs cannot cancel out.
//!
//! Layout (big-endian, like a PLC):
//! - DB10 (HMI interface, writable):
//!   `X0.0` Running = false · `X0.1` Fault = false · `INT2` Setpoint = 100 · `REAL4` Speed = 12.5 ·
//!   `STRING8.20` Name = "line-1" (22 bytes, 8..30) · `UDINT30` Total = 4000000000 · `LREAL34` Ratio = 0.25
//! - DB11: `DINT0` Counter, incremented every 100 ms by the "PLC program"
//! - M (merkers): `X0.0` = true
//!
//! Two simulator differences from a real CPU, handled here:
//! - it confirms the ISO connection with a minimal COTP CC without parameters, while real CPUs
//!   (and the Snap7 C server) send TPDU size and TSAPs, which the client requires. A small proxy
//!   in front of the simulator rewrites only that one frame; every other byte passes unchanged.
//!   The proxy also lets tests take the "PLC" offline by dropping live connections.
//! - it ignores the bit offset of single-bit writes and overwrites the whole byte. Real PLCs
//!   change only the addressed bit; that is verified separately against the exact telegram the
//!   driver sends (tests/s7.rs).

use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use snap7_server::{area, DataStore, S7Server, ServerConfig};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// Data block number of every read request seen (0 for the other areas), in order.
type Reads = Arc<std::sync::Mutex<Vec<u16>>>;

pub struct S7TestServer {
    /// Port of the proxy, i.e. the "PLC" clients connect to.
    pub port: u16,
    pub store: DataStore,
    writes: Arc<AtomicUsize>,
    reads: Reads,
    tasks: Arc<std::sync::Mutex<Vec<tokio::task::AbortHandle>>>,
}

impl S7TestServer {
    /// Number of client writes that reached DB10 (the counter in DB11 is not counted).
    pub fn writes(&self) -> usize {
        self.writes.load(Ordering::SeqCst)
    }

    /// Data block numbers of the read requests received since the last call (0 = not a DB).
    pub fn take_reads(&self) -> Vec<u16> {
        std::mem::take(&mut self.reads.lock().unwrap())
    }

    pub fn db10(&self, start: u32, count: u32) -> Vec<u8> {
        self.store.read_bytes(10, start, count)
    }

    /// Stops accepting connections and drops existing ones, like a PLC going offline.
    pub fn stop(&self) {
        for task in self.tasks.lock().unwrap().drain(..) {
            task.abort();
        }
    }
}

impl Drop for S7TestServer {
    fn drop(&mut self) {
        self.stop();
    }
}

pub async fn start(port: u16) -> Result<S7TestServer, String> {
    let store = DataStore::new();
    store.write_bytes(10, 0, &[0b0000_0000, 0]);
    store.write_bytes(10, 2, &100i16.to_be_bytes());
    store.write_bytes(10, 4, &12.5f32.to_be_bytes());
    let mut name = vec![20u8, 6];
    name.extend_from_slice(b"line-1");
    name.resize(22, 0);
    store.write_bytes(10, 8, &name);
    store.write_bytes(10, 30, &4_000_000_000u32.to_be_bytes());
    store.write_bytes(10, 34, &0.25f64.to_be_bytes());
    store.write_area(area::MARKERS, 0, 0, &[0b0000_0001]);

    let writes = Arc::new(AtomicUsize::new(0));
    let counted = writes.clone();
    store.on_write(move |event| {
        if event.area == area::DATA_BLOCK && event.db_number == 10 {
            counted.fetch_add(1, Ordering::SeqCst);
        }
    });

    // The simulator itself, on a private port.
    let inner_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let server = S7Server::bind(ServerConfig {
        bind_addr: inner_addr,
        max_connections: 8,
    })
    .await
    .map_err(|e| format!("{e}"))?;
    let inner = server.local_addr().map_err(|e| e.to_string())?;
    let tasks = Arc::new(std::sync::Mutex::new(Vec::new()));
    let served = store.clone();
    let simulator = tokio::spawn(async move {
        let _ = server.serve(served).await;
    });
    let program = store.clone();
    let counter = tokio::spawn(async move {
        let mut value: i32 = 0;
        let mut interval = tokio::time::interval(Duration::from_millis(100));
        loop {
            interval.tick().await;
            value += 1;
            program.write_bytes(11, 0, &value.to_be_bytes());
        }
    });

    // The proxy clients talk to.
    let listener = TcpListener::bind(("127.0.0.1", port))
        .await
        .map_err(|e| format!("cannot bind: {e}"))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let connections = tasks.clone();
    let reads: Reads = Default::default();
    let seen = reads.clone();
    let accept = tokio::spawn(async move {
        while let Ok((client, _)) = listener.accept().await {
            let seen = seen.clone();
            let task = tokio::spawn(async move {
                if let Ok(plc) = TcpStream::connect(inner).await {
                    let _ = proxy(client, plc, seen).await;
                }
            });
            connections.lock().unwrap().push(task.abort_handle());
        }
    });
    tasks.lock().unwrap().extend([
        simulator.abort_handle(),
        counter.abort_handle(),
        accept.abort_handle(),
    ]);
    Ok(S7TestServer {
        port,
        store,
        writes,
        reads,
        tasks,
    })
}

/// Forwards one connection, replacing the simulator's minimal COTP connection confirm with the
/// parameterised form a real CPU sends.
async fn proxy(client: TcpStream, plc: TcpStream, reads: Reads) -> std::io::Result<()> {
    let (mut client_read, mut client_write) = client.into_split();
    let (mut plc_read, mut plc_write) = plc.into_split();
    // Client to PLC, frame by frame, noting which block each read request asks for.
    let upstream = tokio::spawn(async move {
        loop {
            let mut header = [0u8; 4];
            client_read.read_exact(&mut header).await?;
            let length = u16::from_be_bytes([header[2], header[3]]) as usize;
            let mut frame = header.to_vec();
            frame.resize(length.max(4), 0);
            client_read.read_exact(&mut frame[4..]).await?;
            // S7 job (0x32 0x01), function "read var" (0x04), one item: DB number at 25..27.
            if frame.len() >= 31 && frame[7] == 0x32 && frame[8] == 0x01 && frame[17] == 0x04 {
                let db = if frame[27] == 0x84 {
                    u16::from_be_bytes([frame[25], frame[26]])
                } else {
                    0
                };
                reads.lock().unwrap().push(db);
            }
            plc_write.write_all(&frame).await?;
        }
        #[allow(unreachable_code)]
        std::io::Result::Ok(())
    });

    let mut header = [0u8; 4];
    plc_read.read_exact(&mut header).await?;
    let length = u16::from_be_bytes([header[2], header[3]]) as usize;
    let mut body = vec![0u8; length.saturating_sub(4)];
    plc_read.read_exact(&mut body).await?;
    let first = if body.len() >= 6 && body[1] == 0xD0 {
        // TPKT + COTP CC: LI, CC, dst-ref, src-ref, class, TPDU size 1024, calling/called TSAP.
        let mut cc = vec![
            0x03, 0x00, 0x00, 22, 17, 0xD0, body[2], body[3], body[4], body[5], 0x00,
        ];
        cc.extend_from_slice(&[
            0xC0, 0x01, 0x0A, 0xC1, 0x02, 0x01, 0x00, 0xC2, 0x02, 0x03, 0x00,
        ]);
        cc
    } else {
        let mut frame = header.to_vec();
        frame.extend(body);
        frame
    };
    client_write.write_all(&first).await?;
    tokio::io::copy(&mut plc_read, &mut client_write).await?;
    upstream.abort();
    Ok(())
}
