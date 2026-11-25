//! Bounded protocol-3 transport. A failed transport ends the session.
use hmi_engine::protocol::{Message, Request};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc::channel;

const MAX_FRAME: usize = 1024 * 1024;
const QUEUE: usize = 32;

#[tokio::main]
async fn main() {
    let (request_tx, request_rx) = channel::<Request>(QUEUE);
    let (out_tx, mut out_rx) = channel::<Message>(QUEUE);
    let mut writer = tokio::spawn(async move {
        let mut stdout = tokio::io::stdout();
        while let Some(message) = out_rx.recv().await {
            let Ok(mut line) = serde_json::to_vec(&message) else {
                break;
            };
            if line.len() > MAX_FRAME {
                break;
            }
            line.push(b'\n');
            if !matches!(
                tokio::time::timeout(Duration::from_secs(2), async {
                    stdout.write_all(&line).await?;
                    stdout.flush().await
                })
                .await,
                Ok(Ok(()))
            ) {
                break;
            }
        }
    });
    let reader_out = out_tx.clone();
    let reader = tokio::spawn(async move {
        let mut input = BufReader::new(tokio::io::stdin());
        let mut frame = Vec::new();
        // A read error, EOF or an oversized frame ends the session.
        while let Ok(bytes) = input.fill_buf().await {
            if bytes.is_empty() {
                break;
            }
            let newline = bytes.iter().position(|&b| b == b'\n');
            let count = newline.unwrap_or(bytes.len());
            if count > MAX_FRAME - frame.len() {
                break;
            }
            frame.extend_from_slice(&bytes[..count]);
            input.consume(count + usize::from(newline.is_some()));
            if newline.is_none() {
                continue;
            }
            if !frame.iter().all(u8::is_ascii_whitespace) {
                match serde_json::from_slice::<Request>(&frame) {
                    Ok(request) => {
                        if request_tx.try_send(request).is_err() {
                            break;
                        }
                    }
                    Err(_) => {
                        if reader_out
                            .try_send(Message::Error {
                                id: 0,
                                code: "BAD_REQUEST".into(),
                                message: "not a valid request".into(),
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                }
            }
            frame.clear();
        }
    });
    hmi_engine::engine::run(request_rx, out_tx).await;
    reader.abort();
    let _ = tokio::time::timeout(Duration::from_secs(2), &mut writer).await;
    std::process::exit(0);
}
