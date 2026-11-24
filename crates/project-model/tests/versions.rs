//! Schema version detection, unsupported versions and migrations.
#![allow(clippy::unwrap_used)]

mod common;

use common::*;
use hmi_project_model::migration::{Migration, MigrationError, MigrationRegistry};
use hmi_project_model::serialization::{
    detect_version, load_project, load_project_with, parse_project, LoadError,
};
use hmi_project_model::{validate, DiagnosticCode as C, CURRENT_SCHEMA_VERSION};
use serde_json::{json, Value};

#[test]
fn detects_current_version() {
    assert_eq!(
        detect_version(&base().to_string()).unwrap(),
        CURRENT_SCHEMA_VERSION
    );
    assert_eq!(
        detect_version(&fixture("valid/bottling-line.hmi.json")).unwrap(),
        1
    );
}

#[test]
fn rejects_documents_that_are_not_projects() {
    for (doc, expect_not_project) in [
        (json!([1, 2]), true),
        (json!({ "schemaVersion": 1 }), true),
        (json!({ "format": "figma", "schemaVersion": 1 }), true),
        (json!({ "format": "hmi-studio.project" }), false),
        (
            json!({ "format": "hmi-studio.project", "schemaVersion": "1" }),
            false,
        ),
        (
            json!({ "format": "hmi-studio.project", "schemaVersion": -1 }),
            false,
        ),
        (
            json!({ "format": "hmi-studio.project", "schemaVersion": 1.5 }),
            false,
        ),
    ] {
        match detect_version(&doc.to_string()) {
            Err(LoadError::NotAProject(_)) => assert!(expect_not_project, "{doc}"),
            Err(LoadError::MissingVersion) => assert!(!expect_not_project, "{doc}"),
            other => panic!("{doc}: {other:?}"),
        }
    }
}

#[test]
fn newer_versions_are_rejected_explicitly() {
    let mut v = base();
    v["schemaVersion"] = json!(CURRENT_SCHEMA_VERSION + 1);
    v["someFutureField"] = json!({ "x": 1 });
    match parse_project(&v.to_string()).unwrap_err() {
        e @ LoadError::UnsupportedVersion {
            found,
            newest_supported,
            ..
        } => {
            assert_eq!(found, u64::from(CURRENT_SCHEMA_VERSION + 1));
            assert_eq!(newest_supported, CURRENT_SCHEMA_VERSION);
            assert!(e.to_string().contains("newer HMI Studio"), "{e}");
        }
        other => panic!("{other}"),
    }
    v["schemaVersion"] = json!(u64::MAX);
    assert!(matches!(
        parse_project(&v.to_string()),
        Err(LoadError::UnsupportedVersion { .. })
    ));
}

#[test]
fn older_versions_without_migrations_are_rejected() {
    let mut v = base();
    v["schemaVersion"] = json!(0);
    match parse_project(&v.to_string()).unwrap_err() {
        LoadError::UnsupportedVersion {
            found,
            oldest_supported,
            ..
        } => {
            assert_eq!(found, 0);
            assert_eq!(oldest_supported, CURRENT_SCHEMA_VERSION);
        }
        other => panic!("{other}"),
    }
}

/// Synthetic "version 0" format used only to exercise the migration
/// machinery: it stored the project name at the root as `title`.
struct V0ToV1;

impl Migration for V0ToV1 {
    fn source_version(&self) -> u32 {
        0
    }
    fn description(&self) -> &str {
        "move root title into metadata.name"
    }
    fn migrate(&self, mut doc: Value) -> Result<Value, MigrationError> {
        let obj = doc.as_object_mut().ok_or(MigrationError::Failed {
            from: 0,
            message: "root is not an object".into(),
        })?;
        let title = obj.remove("title").ok_or(MigrationError::Failed {
            from: 0,
            message: "missing title".into(),
        })?;
        obj.insert("metadata".into(), json!({ "name": title }));
        obj.insert("schemaVersion".into(), json!(1));
        Ok(doc)
    }
}

struct Broken;

impl Migration for Broken {
    fn source_version(&self) -> u32 {
        0
    }
    fn description(&self) -> &str {
        "forgets to bump schemaVersion"
    }
    fn migrate(&self, doc: Value) -> Result<Value, MigrationError> {
        Ok(doc)
    }
}

fn v0_document() -> Value {
    let mut v = base();
    let name = v["metadata"]["name"].take();
    v.as_object_mut().unwrap().remove("metadata");
    v["title"] = name;
    v["schemaVersion"] = json!(0);
    v
}

#[test]
fn migrations_upgrade_older_documents() {
    let mut registry = MigrationRegistry::builtin();
    registry.register(Box::new(V0ToV1)).unwrap();
    assert!(registry.can_migrate(0, 1));
    assert_eq!(registry.oldest_supported(1), 0);

    let loaded = load_project_with(&v0_document().to_string(), &registry).unwrap();
    assert_eq!(loaded.source_version, 0);
    assert_eq!(loaded.migrations.len(), 1);
    assert_eq!(loaded.migrations[0].from, 0);
    assert_eq!(loaded.migrations[0].to, 1);
    assert_eq!(loaded.project.schema_version, 1);
    assert_eq!(loaded.project.metadata.name, "Test");
    assert_eq!(loaded.project, project(base()));
}

#[test]
fn migration_registry_guards() {
    let mut registry = MigrationRegistry::builtin();
    registry.register(Box::new(V0ToV1)).unwrap();
    assert_eq!(
        registry.register(Box::new(V0ToV1)).unwrap_err(),
        MigrationError::DuplicateStep(0)
    );
    assert_eq!(
        registry.migrate(json!({}), 2, 1).unwrap_err(),
        MigrationError::Downgrade { from: 2, to: 1 }
    );
    assert_eq!(
        registry.migrate(json!({}), 1, 3).unwrap_err(),
        MigrationError::MissingStep { from: 1 }
    );

    let mut broken = MigrationRegistry::builtin();
    broken.register(Box::new(Broken)).unwrap();
    match load_project_with(&v0_document().to_string(), &broken).unwrap_err() {
        LoadError::Migration(MigrationError::WrongResultVersion {
            from,
            expected,
            produced,
        }) => {
            assert_eq!((from, expected, produced), (0, 1, Some(0)));
        }
        other => panic!("{other}"),
    }
}

#[test]
fn validator_reports_unsupported_schema_version_and_format() {
    let mut p = project(base());
    p.schema_version = 7;
    p.format = "other".into();
    let r = validate(&p);
    assert_only_errors(
        &r,
        &[
            (C::UnsupportedFormat, "/format"),
            (C::UnsupportedSchemaVersion, "/schemaVersion"),
        ],
    );
}

#[test]
fn load_project_fails_with_full_report() {
    let mut v = base();
    v["navigation"]["startScreen"] = json!("nope");
    match load_project(&v.to_string()).unwrap_err() {
        LoadError::Invalid(report) => {
            assert_eq!(report.error_count(), 1);
            assert_eq!(report.diagnostics[0].path, "/navigation/startScreen");
        }
        other => panic!("{other}"),
    }
}
