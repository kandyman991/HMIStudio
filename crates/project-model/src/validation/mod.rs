//! Deterministic project validation.
//!
//! [`validate`] never mutates or discards anything: it returns a
//! [`ValidationReport`] listing every problem found, in a stable order
//! (document order, then check order). The same project always yields the
//! same report, so reports can be diffed, snapshot-tested and shown in the
//! editor next to the offending JSON path.
//!
//! Severity:
//! * **error** — the project must not be published or saved with
//!   [`crate::save_project`]; a runtime may refuse to load it.
//! * **warning** — the project is usable, but something is likely wrong
//!   (missing translation, possible secret in a connection).

mod checks;
mod layout;
mod values;

use serde::Serialize;

use crate::model::Project;

/// Diagnostic severity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    /// Must be fixed.
    Error,
    /// Should be reviewed.
    Warning,
}

/// Stable, machine-readable diagnostic code. The serialized (kebab-case)
/// names are part of the public contract and are listed in
/// `docs/PROJECT_MODEL_VALIDATION.md`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DiagnosticCode {
    /// `format` is not [`crate::FORMAT_ID`].
    UnsupportedFormat,
    /// `schemaVersion` is not the version this build validates.
    UnsupportedSchemaVersion,
    /// Identifier does not match [`crate::id::ID_PATTERN`].
    InvalidIdentifier,
    /// Identifier used more than once in the project.
    DuplicateIdentifier,
    /// Reference to an object that does not exist.
    UnknownReference,
    /// Reference to an object of the wrong kind (e.g. navigating to a popup).
    InvalidReference,
    /// Element has children it cannot have, or lacks required children.
    InvalidHierarchy,
    /// Component definitions instantiate each other in a cycle.
    CircularComponentReference,
    /// Structured data types contain each other in a cycle.
    CircularDataType,
    /// Layout tree deeper than [`crate::MAX_NODE_DEPTH`].
    NestingTooDeep,
    /// Size, constraint or placement parameter is invalid for its context.
    InvalidLayoutConstraint,
    /// Responsive rule is empty or inapplicable.
    InvalidResponsiveRule,
    /// Property not defined for this element.
    UnknownProperty,
    /// Property value has the wrong JSON type.
    InvalidPropertyType,
    /// Property value has the right type but is out of range / not allowed.
    InvalidPropertyValue,
    /// Component definition parameter or instance argument is invalid.
    InvalidParameter,
    /// Binding cannot work (type, direction, access, transform, scope).
    InvalidBinding,
    /// Same property bound twice on one node.
    DuplicateBinding,
    /// Event not raised by this element.
    InvalidEvent,
    /// Action cannot work (target kind, type, access, scope).
    InvalidAction,
    /// Tag declaration is inconsistent.
    InvalidTag,
    /// Data type or member path is invalid.
    InvalidDataType,
    /// Alarm definition is inconsistent.
    InvalidAlarm,
    /// Recipe definition is inconsistent.
    InvalidRecipe,
    /// Localization table is inconsistent.
    InvalidLocalization,
    /// Text missing for a non-default locale (warning).
    MissingTranslation,
    /// Resource declaration is invalid.
    InvalidResource,
    /// Theme or style is invalid.
    InvalidStyle,
    /// Other invalid value (metadata, device profiles, breakpoints, ...).
    InvalidValue,
    /// A connection parameter looks like an embedded secret (warning).
    PossibleSecret,
}

/// One validation finding.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    /// Severity.
    pub severity: Severity,
    /// Stable code.
    pub code: DiagnosticCode,
    /// RFC 6901 JSON Pointer to the offending value.
    pub path: String,
    /// Human-readable explanation (English; not part of the stable
    /// contract).
    pub message: String,
}

/// Result of [`validate`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ValidationReport {
    /// Diagnostics in deterministic order.
    pub diagnostics: Vec<Diagnostic>,
}

impl ValidationReport {
    /// Whether any error-severity diagnostic exists.
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }

    /// Whether there are no errors (warnings allowed).
    pub fn is_valid(&self) -> bool {
        !self.has_errors()
    }

    /// Number of errors.
    pub fn error_count(&self) -> usize {
        self.errors().count()
    }

    /// Error diagnostics.
    pub fn errors(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
    }

    /// Warning diagnostics.
    pub fn warnings(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Warning)
    }

    /// Whether a diagnostic with `code` exists.
    pub fn has_code(&self, code: DiagnosticCode) -> bool {
        self.diagnostics.iter().any(|d| d.code == code)
    }

    /// Diagnostics with `code`.
    pub fn with_code(&self, code: DiagnosticCode) -> Vec<&Diagnostic> {
        self.diagnostics.iter().filter(|d| d.code == code).collect()
    }
}

impl std::fmt::Display for ValidationReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for d in &self.diagnostics {
            let sev = match d.severity {
                Severity::Error => "error",
                Severity::Warning => "warning",
            };
            let code = serde_json::to_value(d.code)
                .ok()
                .and_then(|v| v.as_str().map(str::to_owned))
                .unwrap_or_default();
            writeln!(f, "{sev}[{code}] {}: {}", d.path, d.message)?;
        }
        Ok(())
    }
}

/// Validates a project. See the module documentation.
pub fn validate(project: &Project) -> ValidationReport {
    let mut v = checks::Validator::new(project);
    v.run();
    ValidationReport {
        diagnostics: v.into_diagnostics(),
    }
}
