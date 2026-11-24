//! Validates project files and optionally rewrites them in canonical form.
//!
//! ```text
//! cargo run --example validate -- fixtures/valid/*.hmi.json
//! cargo run --example validate -- --json path/to/project.hmi.json
//! cargo run --example validate -- --canonicalize path/to/project.hmi.json
//! ```
//!
//! Exit status: 0 if every file loads and has no validation errors, 1
//! otherwise, 2 on usage errors.

use std::process::ExitCode;

use hmi_project_model::serialization::{parse_project, to_json_string};
use hmi_project_model::validate;

fn main() -> ExitCode {
    let mut json = false;
    let mut canonicalize = false;
    let mut files = Vec::new();
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--json" => json = true,
            "--canonicalize" => canonicalize = true,
            "-h" | "--help" => {
                eprintln!("usage: validate [--json] [--canonicalize] FILE...");
                return ExitCode::from(2);
            }
            _ => files.push(arg),
        }
    }
    if files.is_empty() {
        eprintln!("usage: validate [--json] [--canonicalize] FILE...");
        return ExitCode::from(2);
    }
    let mut ok = true;
    for file in &files {
        let text = match std::fs::read_to_string(file) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("{file}: {e}");
                ok = false;
                continue;
            }
        };
        let parsed = match parse_project(&text) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("{file}: {e}");
                ok = false;
                continue;
            }
        };
        let report = validate(&parsed.project);
        if json {
            match serde_json::to_string_pretty(&report) {
                Ok(s) => println!("{s}"),
                Err(e) => eprintln!("{file}: {e}"),
            }
        } else if report.diagnostics.is_empty() {
            println!("{file}: ok");
        } else {
            println!(
                "{file}: {} error(s), {} warning(s)",
                report.error_count(),
                report.warnings().count()
            );
            print!("{report}");
        }
        if report.has_errors() {
            ok = false;
        }
        if canonicalize {
            match to_json_string(&parsed.project) {
                Ok(canonical) if canonical != text => {
                    if let Err(e) = std::fs::write(file, canonical) {
                        eprintln!("{file}: {e}");
                        ok = false;
                    } else {
                        println!("{file}: rewritten in canonical form");
                    }
                }
                Ok(_) => {}
                Err(e) => {
                    eprintln!("{file}: {e}");
                    ok = false;
                }
            }
        }
    }
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
