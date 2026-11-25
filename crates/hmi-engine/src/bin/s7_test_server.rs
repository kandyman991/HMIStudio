//! Test-only S7 PLC process. Prints `READY port=<port>` once listening. Line commands on stdin:
//! `writes` → `WRITES <count>`; `db10 <start> <count>` → `BYTES <hex>`. End of input stops it.

use std::io::Write;

use tokio::io::{AsyncBufReadExt, BufReader};

#[tokio::main]
async fn main() {
    let port: u16 = std::env::args()
        .nth(1)
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let server = match hmi_engine::testserver_s7::start(port).await {
        Ok(server) => server,
        Err(error) => {
            eprintln!("S7 test server failed to start: {error}");
            std::process::exit(1);
        }
    };
    println!("READY port={}", server.port);
    let _ = std::io::stdout().flush();
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let words: Vec<&str> = line.split_whitespace().collect();
        match words.as_slice() {
            ["writes"] => println!("WRITES {}", server.writes()),
            ["db10", start, count] => {
                let (Ok(start), Ok(count)) = (start.parse(), count.parse()) else {
                    continue;
                };
                let hex: String = server
                    .db10(start, count)
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect();
                println!("BYTES {hex}");
            }
            _ => println!("ERROR unknown command"),
        }
        let _ = std::io::stdout().flush();
    }
    server.stop();
    std::process::exit(0);
}
