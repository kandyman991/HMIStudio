#![allow(dead_code, clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use hmi_engineering_core::fs::{FileSystem, StdFileSystem};
use hmi_engineering_core::{Command, ProjectSession, SessionConfig, Transaction};
use hmi_project_model::migration::MigrationRegistry;
use hmi_project_model::model::*;
use hmi_project_model::serialization::parse_project;
use serde_json::json;

/// Unique temporary directory, removed on drop.
pub struct TempDir(pub PathBuf);

impl TempDir {
    pub fn new(tag: &str) -> Self {
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("hmi-core-{tag}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    pub fn join(&self, p: &str) -> PathBuf {
        self.0.join(p)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../project-model/fixtures/valid")
        .join(name)
}

pub fn bottling() -> Project {
    parse_project(&std::fs::read_to_string(fixture_path("bottling-line.hmi.json")).unwrap())
        .unwrap()
        .project
}

pub fn minimal() -> Project {
    parse_project(&std::fs::read_to_string(fixture_path("minimal.hmi.json")).unwrap())
        .unwrap()
        .project
}

pub fn session(p: Project) -> ProjectSession {
    ProjectSession::new(p, SessionConfig::default())
}

pub fn std_fs() -> Arc<dyn FileSystem> {
    Arc::new(StdFileSystem)
}

pub fn registry() -> MigrationRegistry {
    MigrationRegistry::builtin()
}

pub fn nid(s: &str) -> NodeId {
    NodeId::new(s).unwrap()
}

pub fn sid(s: &str) -> ScreenId {
    ScreenId::new(s).unwrap()
}

pub fn text_node(id: &str, text: &str) -> Node {
    serde_json::from_value(json!({
        "id": id, "element": { "type": "component", "kind": "text" },
        "properties": { "text": text }
    }))
    .unwrap()
}

pub fn run(s: &mut ProjectSession, cmds: Vec<Command>) -> hmi_engineering_core::TransactionOutcome {
    s.execute(Transaction::new(cmds))
        .unwrap_or_else(|e| panic!("transaction failed: {e:?}"))
}

pub fn canonical(p: &Project) -> String {
    hmi_project_model::to_json_string(p).unwrap()
}

pub fn write(path: &Path, text: &str) {
    std::fs::write(path, text).unwrap();
}

/// Builds a column chain `depth` deep under a fresh root, with `width`
/// leaves per level; ids are prefixed.
pub fn deep_tree(prefix: &str, depth: usize, width: usize) -> Node {
    let mut node = Node::new(
        nid(&format!("{prefix}-d{depth}")),
        Element::Box {
            content_alignment: Alignment::TopStart,
        },
    );
    for d in (1..depth).rev() {
        let mut parent = Node::new(
            nid(&format!("{prefix}-d{d}")),
            if d % 2 == 0 {
                Element::Row {
                    arrangement: Arrangement::Start,
                    alignment: CrossAlignment::Start,
                    spacing: 4.0,
                    wrap: false,
                }
            } else {
                Element::Column {
                    arrangement: Arrangement::Start,
                    alignment: CrossAlignment::Start,
                    spacing: 4.0,
                    wrap: false,
                }
            },
        );
        for w in 0..width {
            parent
                .children
                .push(text_node(&format!("{prefix}-d{d}-t{w}"), "x"));
        }
        parent.children.push(node);
        node = parent;
    }
    node
}
