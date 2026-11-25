//! # HMI Studio project model
//!
//! The authoritative, framework-independent representation of an HMI
//! Studio project. It is the interchange contract between the Rust
//! engineering environment (Tauri desktop editor) and the Kotlin
//! Multiplatform / Compose runtimes on Windows, Linux and Android.
//!
//! * [`model`] — plain data types: project, screens, semantic layout tree,
//!   components, styles, resources, tags, bindings, events, alarms,
//!   recipes, localization and units.
//! * [`catalog`] — typed property/event catalog for built-in components.
//! * [`serialization`] — canonical JSON load/save, version detection.
//! * [`migration`] — schema migration interfaces.
//! * [`validation`] — deterministic structural and semantic validation.
//! * [`schema`] — machine-readable JSON Schema export.
//!
//! The crate has no dependency on Tauri, any web technology or Kotlin.
//! See `docs/PROJECT_MODEL.md`, `docs/PROJECT_SCHEMA.md` and
//! `docs/PROJECT_MODEL_VALIDATION.md`.

pub mod catalog;
pub mod expression;
pub mod id;
pub mod migration;
pub mod model;
pub mod schema;
pub mod serialization;
pub mod symbols;
pub mod validation;

pub use migration::{Migration, MigrationError, MigrationRegistry};
pub use model::Project;
pub use serialization::{
    detect_version, load_project, parse_project, save_project, to_json_string, LoadError,
    LoadedProject, SaveError,
};
pub use validation::{validate, Diagnostic, DiagnosticCode, Severity, ValidationReport};

/// Value of the `format` field of every project document.
pub const FORMAT_ID: &str = "hmi-studio.project";

/// Schema version written by this crate.
pub const CURRENT_SCHEMA_VERSION: u32 = 1;

/// Maximum nesting depth of a layout tree (root = depth 1). Bounded so that
/// recursive processing on every platform (including Android) stays far
/// from stack limits and below `serde_json`'s recursion limit.
pub const MAX_NODE_DEPTH: usize = 32;
