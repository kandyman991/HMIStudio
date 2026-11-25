//! Test-only OPC UA server process. Prints `READY port=<port> ns=<namespace index>` once it is
//! listening. Line commands on stdin: `writes` prints `WRITES <count>`; end of input stops it.

use std::io::Write;

use tokio::io::{AsyncBufReadExt, BufReader};

#[tokio::main]
async fn main() {
    let port: u16 = std::env::args()
        .nth(1)
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let pki = std::env::temp_dir().join(format!("hmi-opcua-test-server-{}", std::process::id()));
    let server = match hmi_engine::testserver::start(port, pki.clone()).await {
        Ok(server) => server,
        Err(error) => {
            eprintln!("test server failed to start: {error}");
            std::process::exit(1);
        }
    };
    println!("READY port={} ns={}", server.port, server.ns);
    let _ = std::io::stdout().flush();

    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        if line.trim() == "writes" {
            println!("WRITES {}", server.writes().len());
            let _ = std::io::stdout().flush();
        }
    }
    server.stop();
    let _ = std::fs::remove_dir_all(pki);
}
