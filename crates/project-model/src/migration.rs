//! Schema migration interfaces.
//!
//! Migrations operate on the raw JSON document (`serde_json::Value`) rather
//! than on typed structs, so a migration from version *N* never needs the
//! Rust types of version *N* to still exist. Each migration upgrades exactly
//! one version step; the registry chains them.
//!
//! Version 1 is the first schema, so [`MigrationRegistry::builtin`] is
//! currently empty. The machinery is exercised by tests with synthetic
//! migrations.

use std::collections::BTreeMap;

use serde_json::Value;

/// A single-step schema migration `source_version -> source_version + 1`.
pub trait Migration: Send + Sync {
    /// Version this migration reads.
    fn source_version(&self) -> u32;

    /// Human-readable summary, recorded in [`MigrationStep`].
    fn description(&self) -> &str;

    /// Transforms the document. Implementations must set `schemaVersion`
    /// to `source_version() + 1` and must not drop information silently:
    /// data with no equivalent in the target version must be moved to an
    /// `extensions` entry or the migration must fail.
    fn migrate(&self, document: Value) -> Result<Value, MigrationError>;
}

/// Record of one applied migration step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MigrationStep {
    /// Source version.
    pub from: u32,
    /// Target version.
    pub to: u32,
    /// Migration description.
    pub description: String,
}

/// Migration failure.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum MigrationError {
    /// No migration is registered for a required step.
    #[error("no migration registered from schema version {from}")]
    MissingStep {
        /// Version with no outgoing migration.
        from: u32,
    },
    /// A migration with the same source version is already registered.
    #[error("a migration from schema version {0} is already registered")]
    DuplicateStep(u32),
    /// Downgrades are not supported.
    #[error("cannot migrate from schema version {from} down to {to}")]
    Downgrade {
        /// Source.
        from: u32,
        /// Target.
        to: u32,
    },
    /// A migration produced a document with an unexpected version.
    #[error("migration from {from} produced schemaVersion {produced:?}, expected {expected}")]
    WrongResultVersion {
        /// Source version.
        from: u32,
        /// Expected result version.
        expected: u32,
        /// Version found in the result.
        produced: Option<u64>,
    },
    /// The migration itself failed.
    #[error("migration from schema version {from} failed: {message}")]
    Failed {
        /// Source version.
        from: u32,
        /// Reason.
        message: String,
    },
}

/// Ordered collection of single-step migrations.
#[derive(Default)]
pub struct MigrationRegistry {
    steps: BTreeMap<u32, Box<dyn Migration>>,
}

impl std::fmt::Debug for MigrationRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MigrationRegistry")
            .field("sources", &self.steps.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl MigrationRegistry {
    /// Registry with every migration shipped by this crate version.
    pub fn builtin() -> Self {
        Self::default()
    }

    /// Adds a migration.
    pub fn register(&mut self, migration: Box<dyn Migration>) -> Result<(), MigrationError> {
        let from = migration.source_version();
        if self.steps.contains_key(&from) {
            return Err(MigrationError::DuplicateStep(from));
        }
        self.steps.insert(from, migration);
        Ok(())
    }

    /// Whether `from` can be upgraded to `to` with registered steps.
    pub fn can_migrate(&self, from: u32, to: u32) -> bool {
        from <= to && (from..to).all(|v| self.steps.contains_key(&v))
    }

    /// Lowest version that can be upgraded to `to`.
    pub fn oldest_supported(&self, to: u32) -> u32 {
        let mut v = to;
        while v > 0 && self.steps.contains_key(&(v - 1)) {
            v -= 1;
        }
        v
    }

    /// Upgrades `document` from `from` to `to`, step by step.
    pub fn migrate(
        &self,
        mut document: Value,
        from: u32,
        to: u32,
    ) -> Result<(Value, Vec<MigrationStep>), MigrationError> {
        if from > to {
            return Err(MigrationError::Downgrade { from, to });
        }
        let mut applied = Vec::new();
        for version in from..to {
            let step = self
                .steps
                .get(&version)
                .ok_or(MigrationError::MissingStep { from: version })?;
            document = step.migrate(document)?;
            let produced = document.get("schemaVersion").and_then(Value::as_u64);
            if produced != Some(u64::from(version) + 1) {
                return Err(MigrationError::WrongResultVersion {
                    from: version,
                    expected: version + 1,
                    produced,
                });
            }
            applied.push(MigrationStep {
                from: version,
                to: version + 1,
                description: step.description().to_owned(),
            });
        }
        Ok((document, applied))
    }
}
