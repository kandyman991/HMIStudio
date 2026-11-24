//! Identifier collection, deterministic allocation and remapping.

use std::collections::{BTreeMap, BTreeSet};

use hmi_project_model::id::{IdAllocator, IdError};
use hmi_project_model::model::*;

/// Maximum prefix length accepted by [`IdAllocator::allocate`].
const MAX_PREFIX: usize = 40;

/// Every identifier declared anywhere in the project, in document order
/// (duplicates included).
pub fn collect_ids(p: &Project) -> Vec<String> {
    let mut out = vec![p.id.to_string()];
    out.extend(p.navigation.items.iter().map(|x| x.id.to_string()));
    out.extend(p.device_profiles.iter().map(|x| x.id.to_string()));
    out.extend(p.breakpoints.iter().map(|x| x.id.to_string()));
    out.extend(p.themes.iter().map(|x| x.id.to_string()));
    out.extend(p.styles.iter().map(|x| x.id.to_string()));
    out.extend(p.resources.iter().map(|x| x.id.to_string()));
    out.extend(p.units.iter().map(|x| x.id.to_string()));
    out.extend(p.data_types.iter().map(|x| x.id.to_string()));
    out.extend(p.connections.iter().map(|x| x.id.to_string()));
    out.extend(p.tags.iter().map(|x| x.id.to_string()));
    out.extend(p.alarms.iter().map(|x| x.id.to_string()));
    for r in &p.recipes {
        out.push(r.id.to_string());
        out.extend(r.fields.iter().map(|x| x.id.to_string()));
        out.extend(r.sets.iter().map(|x| x.id.to_string()));
    }
    for d in &p.components {
        out.push(d.id.to_string());
        collect_node_ids(&d.root, &mut out);
    }
    for s in &p.screens {
        out.push(s.id.to_string());
        collect_node_ids(&s.root, &mut out);
    }
    out
}

/// Node identifiers of a subtree (iterative, pre-order).
pub fn collect_node_ids(root: &Node, out: &mut Vec<String>) {
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        out.push(n.id.to_string());
        stack.extend(n.children.iter().rev());
    }
}

/// Derives a valid allocator prefix from an existing identifier.
pub fn prefix_of(id: &str) -> String {
    let mut p: String = id.chars().take(MAX_PREFIX).collect();
    while p.ends_with('-') || p.ends_with('_') {
        p.pop();
    }
    if p.is_empty() {
        "id".into()
    } else {
        p
    }
}

/// Allocator that avoids every id in `current` and every id in `retired`
/// (ids that existed earlier in the session, so undo/redo can never make
/// two different objects share an id).
pub fn allocator(current: &Project, retired: &BTreeSet<String>) -> IdAllocator {
    let mut a = IdAllocator::new(collect_ids(current));
    for id in retired {
        a.reserve(id);
    }
    a
}

/// Gives every node of `root` a fresh id, returning `old -> new`.
pub fn reassign_node_ids(
    root: &mut Node,
    alloc: &mut IdAllocator,
) -> Result<BTreeMap<String, String>, IdError> {
    let mut map = BTreeMap::new();
    let mut stack: Vec<&mut Node> = vec![root];
    while let Some(n) = stack.pop() {
        let new = alloc.allocate(&prefix_of(n.id.as_str()))?;
        map.insert(n.id.to_string(), new.clone());
        n.id = NodeId::new_unchecked(new);
        for c in n.children.iter_mut().rev() {
            stack.push(c);
        }
    }
    Ok(map)
}

/// Rewrites screen references inside a subtree (navigate / openPopup).
pub fn remap_screen_refs(root: &mut Node, from: &ScreenId, to: &ScreenId) -> usize {
    let mut count = 0;
    let mut stack: Vec<&mut Node> = vec![root];
    while let Some(n) = stack.pop() {
        for h in &mut n.events {
            count += remap_actions(&mut h.actions, from, to);
        }
        for c in n.children.iter_mut() {
            stack.push(c);
        }
    }
    count
}

/// Rewrites screen references in an action list.
pub fn remap_actions(actions: &mut [Action], from: &ScreenId, to: &ScreenId) -> usize {
    let mut count = 0;
    for a in actions {
        if let Action::Navigate { screen } | Action::OpenPopup { screen } = a {
            if screen == from {
                *screen = to.clone();
                count += 1;
            }
        }
    }
    count
}

/// Rewrites component-definition references inside a subtree.
pub fn remap_definition_refs(
    root: &mut Node,
    from: &ComponentDefinitionId,
    to: &ComponentDefinitionId,
) -> usize {
    let mut count = 0;
    let mut stack: Vec<&mut Node> = vec![root];
    while let Some(n) = stack.pop() {
        if let Element::Instance { definition, .. } = &mut n.element {
            if definition == from {
                *definition = to.clone();
                count += 1;
            }
        }
        for c in n.children.iter_mut() {
            stack.push(c);
        }
    }
    count
}
