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
    out.extend(
        p.v2.industrial
            .security
            .permissions
            .iter()
            .map(|x| x.id.to_string()),
    );
    out.extend(
        p.v2.industrial
            .security
            .roles
            .iter()
            .map(|x| x.id.to_string()),
    );
    out.extend(p.v2.bindings.enumerations.iter().map(|x| x.id.to_string()));
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

/// Copy normalized v2 declarations for a freshly allocated subtree. Global
/// definitions (tags, enumerations, permissions and roles) stay shared.
/// Only self references to a copied screen/definition change.
pub(crate) fn duplicate_v2(
    p: &mut Project,
    nodes: &BTreeMap<String, String>,
    screen: Option<(&ScreenId, &ScreenId)>,
    definition: Option<(&ComponentDefinitionId, &ComponentDefinitionId)>,
) {
    use hmi_project_model::model::v2_components::{CandidateAction, EventOrigin, NewAction};
    let v = &mut p.v2;
    let mapped_node = |id: &NodeId| {
        nodes
            .get(id.as_str())
            .map(|s| NodeId::new_unchecked(s.clone()))
    };
    let copies: Vec<_> = v
        .industrial
        .nodes
        .iter()
        .filter_map(|old| {
            let mut new = old.clone();
            new.node = mapped_node(&old.node)?;
            Some(new)
        })
        .collect();
    v.industrial.nodes.extend(copies);
    let copies: Vec<_> = v
        .bindings
        .expressions
        .iter()
        .filter_map(|old| {
            let mut new = old.clone();
            new.node = mapped_node(&old.node)?;
            Some(new)
        })
        .collect();
    v.bindings.expressions.extend(copies);
    let copies: Vec<_> = v
        .components
        .components
        .iter()
        .filter_map(|old| {
            let mut new = old.clone();
            new.id = mapped_node(&old.id)?;
            Some(new)
        })
        .collect();
    v.components.components.extend(copies);
    if let Some((from, to)) = screen {
        let copies: Vec<_> = v
            .industrial
            .screens
            .iter()
            .filter(|x| &x.screen == from)
            .cloned()
            .map(|mut x| {
                x.screen = to.clone();
                x
            })
            .collect();
        v.industrial.screens.extend(copies);
    }
    if let Some((from, to)) = definition {
        let copies: Vec<_> = v
            .components
            .definitions
            .iter()
            .filter(|x| &x.definition == from)
            .cloned()
            .map(|mut x| {
                x.definition = to.clone();
                x
            })
            .collect();
        v.components.definitions.extend(copies);
    }
    let copies: Vec<_> = v
        .components
        .handlers
        .iter()
        .filter_map(|old| {
            let mut new = old.clone();
            new.origin = match &old.origin {
                EventOrigin::Node(id) => EventOrigin::Node(mapped_node(id)?),
                EventOrigin::Screen(id) => {
                    let (from, to) = screen?;
                    if id != from {
                        return None;
                    }
                    EventOrigin::Screen(to.clone())
                }
            };
            for action in &mut new.actions {
                match action {
                    CandidateAction::Existing(a) => {
                        if let Some((from, to)) = screen {
                            remap_actions(std::slice::from_mut(a), from, to);
                        }
                    }
                    CandidateAction::New(NewAction::OpenFaceplate {
                        definition: target, ..
                    }) => {
                        if let Some((from, to)) = definition {
                            if target == from {
                                *target = to.clone();
                            }
                        }
                    }
                    CandidateAction::New(NewAction::NavigateStartup {}) => {}
                }
            }
            Some(new)
        })
        .collect();
    v.components.handlers.extend(copies);
}
