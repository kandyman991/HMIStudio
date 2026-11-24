//! `hmi-engine`: the runtime engine process. Speaks newline-delimited JSON on stdin/stdout.
//! Exits (after disconnecting every source) when stdin closes, so it cannot outlive its client.

use hmi_engine::protocol::{Message, Request};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc::unbounded_channel;

#[tokio::main]
async fn main() {
    let (request_tx, request_rx) = unbounded_channel::<Request>();
    let (out_tx, mut out_rx) = unbounded_channel::<Message>();

    let writer = tokio::spawn(async move {
        let mut stdout = tokio::io::stdout();
        while let Some(message) = out_rx.recv().await {
            let Ok(mut line) = serde_json::to_string(&message) else {
                continue;
            };
            line.push('\n');
            if stdout.write_all(line.as_bytes()).await.is_err() || stdout.flush().await.is_err() {
                break; // the client is gone
            }
        }
    });

    let reader_out = out_tx.clone();
    let reader = tokio::spawn(async move {
        let mut lines = BufReader::new(tokio::io::stdin()).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<Request>(&line) {
                Ok(request) => {
                    if request_tx.send(request).is_err() {
                        break;
                    }
                }
                // Never guess at a malformed or unknown request; id 0 marks "no request matched".
                Err(error) => {
                    let _ = reader_out.send(Message::Error {
                        id: 0,
                        code: "BAD_REQUEST".into(),
                        message: format!("not a valid request: {error}"),
                    });
                }
            }
        }
        // Dropping request_tx ends the engine loop.
    });

    hmi_engine::engine::run(request_rx, out_tx).await;
    // Every source is disconnected at this point. Let the last replies reach the client, then
    // exit explicitly: tokio's stdin reads on a blocking thread that would otherwise keep the
    // process alive for as long as the client keeps the pipe open.
    reader.abort();
    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), writer).await;
    std::process::exit(0);
}
