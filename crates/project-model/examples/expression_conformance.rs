//! Replay synthetic vectors with the Rust contract-1 engine.
#[path = "../tests/conformance/rust_observe.rs"]
mod runner;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("provide a JSON vector file")?;
    let vectors: Vec<String> = serde_json::from_str(&std::fs::read_to_string(path)?)?;
    for line in runner::observe(&vectors) {
        println!("{line}");
    }
    Ok(())
}
