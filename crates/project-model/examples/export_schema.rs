//! Writes the JSON Schema to `schema/hmi-project.v2.schema.json`
//! (or to the path given as the first argument).
//!
//! ```text
//! cargo run --example export_schema
//! ```

use std::path::PathBuf;

fn main() -> std::io::Result<()> {
    let target = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(hmi_project_model::schema::SCHEMA_FILE)
        });
    std::fs::write(
        &target,
        hmi_project_model::schema::project_json_schema_string(),
    )?;
    println!("wrote {}", target.display());
    Ok(())
}
