//! Change sets describing what a transaction, undo or redo changed.

use serde::{Deserialize, Serialize};

/// Kind of object affected by a change.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EntityKind {
    /// Project metadata.
    Metadata,
    /// Project settings.
    Settings,
    /// Navigation model.
    Navigation,
    /// Project extensions.
    Extensions,
    /// Screen or popup.
    Screen,
    /// Layout node (component, container or instance).
    Node,
    /// Tag.
    Tag,
    /// Named style.
    Style,
    /// Reusable component definition.
    ComponentDefinition,
    /// Whole project (load, undo/redo across many changes).
    Project,
}

/// What happened to the entity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChangeKind {
    /// Created.
    Added,
    /// Modified in place.
    Modified,
    /// Moved to a new parent or position.
    Moved,
    /// Deleted.
    Removed,
}

/// One change.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    /// What happened.
    pub kind: ChangeKind,
    /// Affected entity kind.
    pub entity: EntityKind,
    /// Entity identifier (absent for singletons such as settings).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

/// Ordered, de-duplicated list of changes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeSet {
    /// Changes in the order they first occurred.
    pub changes: Vec<Change>,
}

impl ChangeSet {
    /// Records a change, ignoring exact repeats.
    pub fn push(&mut self, kind: ChangeKind, entity: EntityKind, id: Option<String>) {
        let c = Change { kind, entity, id };
        if !self.changes.contains(&c) {
            self.changes.push(c);
        }
    }

    /// Appends another change set.
    pub fn extend(&mut self, other: ChangeSet) {
        for c in other.changes {
            self.push(c.kind, c.entity, c.id);
        }
    }

    /// Whether nothing changed.
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    /// Whether a change with this kind/entity/id exists.
    pub fn contains(&self, kind: ChangeKind, entity: EntityKind, id: &str) -> bool {
        self.changes
            .iter()
            .any(|c| c.kind == kind && c.entity == entity && c.id.as_deref() == Some(id))
    }
}
