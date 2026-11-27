//! Application of single commands to a working copy of the project.
//!
//! Every function here mutates only the working copy owned by the running
//! transaction. Errors leave the working copy in an unspecified state; the
//! transaction engine discards it, so the session's project is never
//! partially modified.

use std::collections::{BTreeMap, BTreeSet};

use hmi_project_model::id::IdAllocator;
use hmi_project_model::model::*;

use crate::change::{ChangeKind as K, ChangeSet, EntityKind as E};
use crate::command::{AllocatedIds, Command};
use crate::error::CommandError;
use crate::ids;
use crate::locate::{self, NodeLocation};

/// Per-command execution context.
pub(crate) struct Ctx<'a> {
    pub alloc: IdAllocator,
    pub changes: &'a mut ChangeSet,
    pub allocated: &'a mut AllocatedIds,
    pub index: usize,
}

impl Ctx<'_> {
    fn allocate(&mut self, prefix: &str, key: &str) -> Result<String, CommandError> {
        let id = self.alloc.allocate(&ids::prefix_of(prefix)).map_err(|e| {
            CommandError::InvalidArgument {
                message: format!("cannot allocate identifier: {e}"),
            }
        })?;
        self.allocated
            .insert(format!("{}:{key}", self.index), id.clone());
        Ok(id)
    }

    fn reassign(&mut self, root: &mut Node) -> Result<BTreeMap<String, String>, CommandError> {
        let map = ids::reassign_node_ids(root, &mut self.alloc).map_err(|e| {
            CommandError::InvalidArgument {
                message: format!("cannot allocate identifier: {e}"),
            }
        })?;
        for (old, new) in &map {
            self.allocated
                .insert(format!("{}:node:{old}", self.index), new.clone());
            self.changes.push(K::Added, E::Node, Some(new.clone()));
        }
        Ok(map)
    }
}

fn not_found(entity: &'static str, id: impl ToString) -> CommandError {
    CommandError::NotFound {
        entity,
        id: id.to_string(),
    }
}

fn locate(p: &Project, id: &NodeId) -> Result<NodeLocation, CommandError> {
    locate::find(p, id).ok_or_else(|| not_found("node", id))
}

fn node_mut<'a>(p: &'a mut Project, id: &NodeId) -> Result<&'a mut Node, CommandError> {
    let loc = locate(p, id)?;
    locate::get_mut(p, &loc).ok_or_else(|| not_found("node", id))
}

fn parent_loc(loc: &NodeLocation) -> NodeLocation {
    NodeLocation {
        tree: loc.tree,
        path: loc.path[..loc.path.len().saturating_sub(1)].to_vec(),
    }
}

/// Removes the node at `loc` (not a root) and returns it.
fn detach(p: &mut Project, loc: &NodeLocation) -> Result<Node, CommandError> {
    let Some(&last) = loc.path.last() else {
        return Err(CommandError::InvalidArgument {
            message: "cannot detach a tree root".into(),
        });
    };
    let parent = locate::get_mut(p, &parent_loc(loc)).ok_or_else(|| not_found("node", "?"))?;
    if last >= parent.children.len() {
        return Err(not_found("node", "?"));
    }
    Ok(parent.children.remove(last))
}

fn insert_child(
    p: &mut Project,
    parent: &NodeId,
    index: Option<usize>,
    node: Node,
) -> Result<usize, CommandError> {
    let parent = node_mut(p, parent)?;
    let max = parent.children.len();
    let index = index.unwrap_or(max);
    if index > max {
        return Err(CommandError::IndexOutOfRange { index, max });
    }
    parent.children.insert(index, node);
    Ok(index)
}

/// Rejects subtree ids that are already used or repeated inside the subtree.
fn ensure_new_ids(p: &Project, extra: &[String]) -> Result<(), CommandError> {
    let existing: BTreeSet<String> = ids::collect_ids(p).into_iter().collect();
    let mut seen = BTreeSet::new();
    for id in extra {
        if existing.contains(id) || !seen.insert(id.as_str()) {
            return Err(CommandError::AlreadyExists { id: id.clone() });
        }
    }
    Ok(())
}

fn subtree_ids(n: &Node) -> Vec<String> {
    let mut v = Vec::new();
    ids::collect_node_ids(n, &mut v);
    v
}

fn check_index(index: Option<usize>, len: usize) -> Result<usize, CommandError> {
    let index = index.unwrap_or(len);
    if index > len {
        Err(CommandError::IndexOutOfRange { index, max: len })
    } else {
        Ok(index)
    }
}

fn screen_index(p: &Project, id: &ScreenId) -> Result<usize, CommandError> {
    p.screens
        .iter()
        .position(|s| &s.id == id)
        .ok_or_else(|| not_found("screen", id))
}

fn definition_index(p: &Project, id: &ComponentDefinitionId) -> Result<usize, CommandError> {
    p.components
        .iter()
        .position(|d| &d.id == id)
        .ok_or_else(|| not_found("component definition", id))
}

/// Applies one command to the working copy.
pub(crate) fn apply(p: &mut Project, cmd: &Command, cx: &mut Ctx<'_>) -> Result<(), CommandError> {
    match cmd {
        // ------------------------------------------------------- screens
        Command::CreateScreen {
            id,
            name,
            kind,
            root,
            index,
        } => {
            let index = check_index(*index, p.screens.len())?;
            let id = match id {
                Some(id) => {
                    ensure_new_ids(p, &[id.to_string()])?;
                    id.clone()
                }
                None => ScreenId::new_unchecked(cx.allocate("scr", "screen")?),
            };
            let root = match root {
                Some(r) => {
                    let mut all = vec![id.to_string()];
                    all.extend(subtree_ids(r));
                    ensure_new_ids(p, &all)?;
                    r.clone()
                }
                None => {
                    let rid = cx.allocate(&format!("{id}-root"), "root")?;
                    let mut n = Node::new(
                        NodeId::new_unchecked(rid),
                        Element::Column {
                            arrangement: Arrangement::Start,
                            alignment: CrossAlignment::Start,
                            spacing: 0.0,
                            wrap: false,
                        },
                    );
                    n.layout.width = Size::fill();
                    n.layout.height = Size::fill();
                    n
                }
            };
            cx.changes.push(K::Added, E::Screen, Some(id.to_string()));
            p.screens.insert(
                index,
                Screen {
                    id,
                    name: name.clone(),
                    kind: *kind,
                    title: None,
                    design_profile: None,
                    events: Vec::new(),
                    root,
                    extensions: Extensions::new(),
                },
            );
        }
        Command::RenameScreen { screen, name } => {
            let i = screen_index(p, screen)?;
            p.screens[i].name = name.clone();
            cx.changes
                .push(K::Modified, E::Screen, Some(screen.to_string()));
        }
        Command::SetScreenTitle { screen, title } => {
            let i = screen_index(p, screen)?;
            p.screens[i].title = title.clone();
            cx.changes
                .push(K::Modified, E::Screen, Some(screen.to_string()));
        }
        Command::DeleteScreen { screen } => {
            let i = screen_index(p, screen)?;
            p.screens.remove(i);
            cx.changes
                .push(K::Removed, E::Screen, Some(screen.to_string()));
        }
        Command::DuplicateScreen { screen, name } => {
            let i = screen_index(p, screen)?;
            let mut copy = p.screens[i].clone();
            let new_id = ScreenId::new_unchecked(cx.allocate(screen.as_str(), "screen")?);
            let map = cx.reassign(&mut copy.root)?;
            ids::duplicate_v2(p, &map, Some((screen, &new_id)), None);
            ids::remap_screen_refs(&mut copy.root, screen, &new_id);
            for h in &mut copy.events {
                ids::remap_actions(&mut h.actions, screen, &new_id);
            }
            copy.name = name
                .clone()
                .unwrap_or_else(|| format!("{} copy", copy.name));
            copy.id = new_id.clone();
            cx.changes
                .push(K::Added, E::Screen, Some(new_id.to_string()));
            p.screens.insert(i + 1, copy);
        }

        // --------------------------------------------------------- nodes
        Command::InsertNode {
            parent,
            index,
            node,
            assign_new_ids,
        } => {
            let mut node = node.clone();
            if *assign_new_ids {
                if p.has_v2_features()
                    && subtree_ids(&node)
                        .iter()
                        .any(|id| locate::find(p, &NodeId::new_unchecked(id.clone())).is_some())
                {
                    return Err(CommandError::InvalidArgument { message: "existing v2 subtree must be copied with duplicateNode to preserve declarations".into() });
                }
                cx.reassign(&mut node)?;
            } else {
                let all = subtree_ids(&node);
                ensure_new_ids(p, &all)?;
                for id in all {
                    cx.changes.push(K::Added, E::Node, Some(id));
                }
            }
            insert_child(p, parent, *index, node)?;
            cx.changes
                .push(K::Modified, E::Node, Some(parent.to_string()));
        }
        Command::InsertV2Component {
            parent,
            index,
            id,
            layout,
            contract,
        } => {
            let id = match id {
                Some(id) => {
                    ensure_new_ids(p, &[id.to_string()])?;
                    id.clone()
                }
                None => NodeId::new_unchecked(cx.allocate("v2-node", "node")?),
            };
            let mut node = Node::new(
                id.clone(),
                Element::Component {
                    kind: contract.kind(),
                },
            );
            node.layout = layout.clone();
            insert_child(p, parent, *index, node)?;
            p.v2.components.components.push(
                hmi_project_model::model::v2_components::ComponentCandidate {
                    id: id.clone(),
                    contract: contract.clone(),
                },
            );
            cx.changes.push(K::Added, E::Node, Some(id.to_string()));
            cx.changes
                .push(K::Modified, E::Node, Some(parent.to_string()));
            cx.changes.push(K::Modified, E::Project, None);
        }
        Command::SetV2ComponentContract { node, contract } => {
            let loc = locate(p, node)?;
            let existing = locate::get(p, &loc).ok_or_else(|| not_found("node", node))?;
            if !matches!(existing.element, Element::Component { kind } if kind == contract.kind()) {
                return Err(CommandError::InvalidArgument {
                    message: "contract kind must match the existing node".into(),
                });
            }
            let entry =
                p.v2.components
                    .components
                    .iter_mut()
                    .find(|c| &c.id == node)
                    .ok_or_else(|| not_found("component contract", node))?;
            entry.contract = contract.clone();
            cx.changes
                .push(K::Modified, E::Node, Some(node.to_string()));
            cx.changes.push(K::Modified, E::Project, None);
        }
        Command::RemoveNode { node } => {
            let loc = locate(p, node)?;
            if loc.is_root() {
                return Err(CommandError::RootNode {
                    id: node.to_string(),
                    action: "removed",
                });
            }
            let removed = detach(p, &loc)?;
            let removed_ids = subtree_ids(&removed);
            let removed_set: BTreeSet<_> = removed_ids.iter().map(String::as_str).collect();
            let count = p.v2.components.components.len();
            p.v2.components
                .components
                .retain(|c| !removed_set.contains(c.id.as_str()));
            if count != p.v2.components.components.len() {
                cx.changes.push(K::Modified, E::Project, None);
            }
            // Other declarations are independent engineering references:
            // strict validation refuses dangling permissions/bindings/actions.
            for id in removed_ids {
                cx.changes.push(K::Removed, E::Node, Some(id));
            }
            if let Some(parent) = locate::get(p, &parent_loc(&loc)) {
                cx.changes
                    .push(K::Modified, E::Node, Some(parent.id.to_string()));
            }
        }
        Command::DuplicateNode {
            node,
            parent,
            index,
        } => {
            let loc = locate(p, node)?;
            let mut copy = locate::get(p, &loc)
                .cloned()
                .ok_or_else(|| not_found("node", node))?;
            let (target, default_index) = match parent {
                Some(pid) => (pid.clone(), None),
                None => {
                    if loc.is_root() {
                        return Err(CommandError::RootNode {
                            id: node.to_string(),
                            action: "duplicated without a target parent",
                        });
                    }
                    let parent =
                        locate::get(p, &parent_loc(&loc)).ok_or_else(|| not_found("node", node))?;
                    let pos = loc.path.last().copied().unwrap_or(0) + 1;
                    (parent.id.clone(), Some(pos))
                }
            };
            let target_loc = locate(p, &target)?;
            check_reparent_v2(p, &loc, &target_loc)?;
            let map = cx.reassign(&mut copy)?;
            ids::duplicate_v2(p, &map, None, None);
            insert_child(p, &target, index.or(default_index), copy)?;
            cx.changes
                .push(K::Modified, E::Node, Some(target.to_string()));
        }
        Command::MoveNode {
            node,
            parent,
            index,
        } => {
            let loc = locate(p, node)?;
            if loc.is_root() {
                return Err(CommandError::RootNode {
                    id: node.to_string(),
                    action: "moved",
                });
            }
            let ploc = locate(p, parent)?;
            check_reparent_v2(p, &loc, &ploc)?;
            if ploc.is_within(&loc) {
                return Err(CommandError::CircularHierarchy {
                    node: node.to_string(),
                    parent: parent.to_string(),
                });
            }
            let old_parent = locate::get(p, &parent_loc(&loc)).map(|n| n.id.to_string());
            let moving = detach(p, &loc)?;
            insert_child(p, parent, *index, moving)?;
            cx.changes.push(K::Moved, E::Node, Some(node.to_string()));
            if let Some(op) = old_parent {
                cx.changes.push(K::Modified, E::Node, Some(op));
            }
            cx.changes
                .push(K::Modified, E::Node, Some(parent.to_string()));
        }
        Command::ReorderNode { node, index } => {
            let loc = locate(p, node)?;
            if loc.is_root() {
                return Err(CommandError::RootNode {
                    id: node.to_string(),
                    action: "reordered",
                });
            }
            let ploc = parent_loc(&loc);
            let len = locate::get(p, &ploc).map_or(0, |n| n.children.len());
            if *index >= len {
                return Err(CommandError::IndexOutOfRange {
                    index: *index,
                    max: len.saturating_sub(1),
                });
            }
            let moving = detach(p, &loc)?;
            let parent = locate::get_mut(p, &ploc).ok_or_else(|| not_found("node", node))?;
            parent.children.insert(*index, moving);
            let pid = parent.id.to_string();
            cx.changes.push(K::Moved, E::Node, Some(node.to_string()));
            cx.changes.push(K::Modified, E::Node, Some(pid));
        }
        Command::RenameNode { node, name } => {
            node_mut(p, node)?.name = name.clone();
            cx.changes
                .push(K::Modified, E::Node, Some(node.to_string()));
        }
        Command::SetElement { node, element } => {
            node_mut(p, node)?.element = element.clone();
            cx.changes
                .push(K::Modified, E::Node, Some(node.to_string()));
        }
        Command::SetProperty { node, name, value } => {
            let n = node_mut(p, node)?;
            match value {
                Some(v) => {
                    n.properties.insert(name.clone(), v.clone());
                }
                None => {
                    if n.properties.remove(name).is_none() {
                        return Err(not_found("property", name));
                    }
                }
            }
            cx.changes
                .push(K::Modified, E::Node, Some(node.to_string()));
        }
        Command::SetNodeStyle { node, style } => {
            node_mut(p, node)?.style = style.clone();
            cx.changes
                .push(K::Modified, E::Node, Some(node.to_string()));
        }
        Command::SetLayout { node, layout } => {
            node_mut(p, node)?.layout = layout.clone();
            cx.changes
                .push(K::Modified, E::Node, Some(node.to_string()));
        }
        Command::SetResponsive { node, rules } => {
            node_mut(p, node)?.responsive = rules.clone();
            cx.changes
                .push(K::Modified, E::Node, Some(node.to_string()));
        }
        Command::SetBinding { node, binding } => {
            let n = node_mut(p, node)?;
            match n
                .bindings
                .iter_mut()
                .find(|b| b.property == binding.property)
            {
                Some(b) => *b = binding.clone(),
                None => n.bindings.push(binding.clone()),
            }
            cx.changes
                .push(K::Modified, E::Node, Some(node.to_string()));
        }
        Command::RemoveBinding { node, property } => {
            let n = node_mut(p, node)?;
            let before = n.bindings.len();
            n.bindings.retain(|b| &b.property != property);
            if n.bindings.len() == before {
                return Err(not_found("binding", property));
            }
            cx.changes
                .push(K::Modified, E::Node, Some(node.to_string()));
        }
        Command::SetEventHandler { node, handler } => {
            let n = node_mut(p, node)?;
            match n.events.iter_mut().find(|h| h.event == handler.event) {
                Some(h) => *h = handler.clone(),
                None => n.events.push(handler.clone()),
            }
            cx.changes
                .push(K::Modified, E::Node, Some(node.to_string()));
        }
        Command::RemoveEventHandler { node, event } => {
            let n = node_mut(p, node)?;
            let before = n.events.len();
            n.events.retain(|h| &h.event != event);
            if n.events.len() == before {
                return Err(not_found("event handler", format!("{event:?}")));
            }
            cx.changes
                .push(K::Modified, E::Node, Some(node.to_string()));
        }

        // ---------------------------------------------------------- tags
        Command::CreateTag { tag, index } => {
            ensure_new_ids(p, &[tag.id.to_string()])?;
            let index = check_index(*index, p.tags.len())?;
            p.tags.insert(index, tag.clone());
            cx.changes.push(K::Added, E::Tag, Some(tag.id.to_string()));
        }
        Command::UpdateTag { tag } => {
            let t = p
                .tags
                .iter_mut()
                .find(|t| t.id == tag.id)
                .ok_or_else(|| not_found("tag", &tag.id))?;
            *t = tag.clone();
            cx.changes
                .push(K::Modified, E::Tag, Some(tag.id.to_string()));
        }
        Command::DeleteTag { tag } => {
            let i = p
                .tags
                .iter()
                .position(|t| &t.id == tag)
                .ok_or_else(|| not_found("tag", tag))?;
            p.tags.remove(i);
            cx.changes.push(K::Removed, E::Tag, Some(tag.to_string()));
        }

        // -------------------------------------------------------- styles
        Command::UpsertStyle { style } => match p.styles.iter_mut().find(|s| s.id == style.id) {
            Some(s) => {
                *s = style.clone();
                cx.changes
                    .push(K::Modified, E::Style, Some(style.id.to_string()));
            }
            None => {
                ensure_new_ids(p, &[style.id.to_string()])?;
                p.styles.push(style.clone());
                cx.changes
                    .push(K::Added, E::Style, Some(style.id.to_string()));
            }
        },
        Command::DeleteStyle { style } => {
            let i = p
                .styles
                .iter()
                .position(|s| &s.id == style)
                .ok_or_else(|| not_found("style", style))?;
            p.styles.remove(i);
            cx.changes
                .push(K::Removed, E::Style, Some(style.to_string()));
        }

        // --------------------------------------------------- definitions
        Command::CreateIndustrialSymbol { kind } => {
            let definition = hmi_project_model::symbols::build_symbol(*kind, &mut cx.alloc)
                .map_err(|e| CommandError::InvalidArgument {
                    message: format!("cannot allocate symbol: {e}"),
                })?;
            let id = definition.id.to_string();
            cx.allocated
                .insert(format!("{}:definition", cx.index), id.clone());
            cx.changes.push(K::Added, E::ComponentDefinition, Some(id));
            definition.root.walk(&mut |n| {
                let id = n.id.to_string();
                cx.allocated
                    .insert(format!("{}:node:{id}", cx.index), id.clone());
                cx.changes.push(K::Added, E::Node, Some(id));
            });
            p.v2.components.definitions.push(
                hmi_project_model::model::v2_components::DefinitionClassification {
                    definition: definition.id.clone(),
                    classification:
                        hmi_project_model::model::v2_components::DefinitionKind::IndustrialSymbol {
                            symbol: *kind,
                        },
                },
            );
            p.components.push(definition);
            cx.changes.push(K::Modified, E::Project, None);
        }
        Command::CreateComponentDefinition { definition } => {
            let mut all = vec![definition.id.to_string()];
            all.extend(subtree_ids(&definition.root));
            ensure_new_ids(p, &all)?;
            p.components.push(definition.clone());
            cx.changes.push(
                K::Added,
                E::ComponentDefinition,
                Some(definition.id.to_string()),
            );
        }
        Command::UpdateComponentDefinitionInfo {
            definition,
            name,
            description,
            category,
        } => {
            let i = definition_index(p, definition)?;
            let d = &mut p.components[i];
            d.name = name.clone();
            d.description = description.clone();
            d.category = category.clone();
            cx.changes.push(
                K::Modified,
                E::ComponentDefinition,
                Some(definition.to_string()),
            );
        }
        Command::SetComponentParameters {
            definition,
            parameters,
        } => {
            let i = definition_index(p, definition)?;
            p.components[i].parameters = parameters.clone();
            cx.changes.push(
                K::Modified,
                E::ComponentDefinition,
                Some(definition.to_string()),
            );
        }
        Command::DeleteComponentDefinition { definition } => {
            let i = definition_index(p, definition)?;
            p.components.remove(i);
            cx.changes.push(
                K::Removed,
                E::ComponentDefinition,
                Some(definition.to_string()),
            );
        }
        Command::DuplicateComponentDefinition { definition, name } => {
            let i = definition_index(p, definition)?;
            let mut copy = p.components[i].clone();
            let new_id = ComponentDefinitionId::new_unchecked(
                cx.allocate(definition.as_str(), "definition")?,
            );
            let map = cx.reassign(&mut copy.root)?;
            ids::duplicate_v2(p, &map, None, Some((definition, &new_id)));
            ids::remap_definition_refs(&mut copy.root, definition, &new_id);
            copy.name = name
                .clone()
                .unwrap_or_else(|| format!("{} copy", copy.name));
            copy.id = new_id.clone();
            cx.changes
                .push(K::Added, E::ComponentDefinition, Some(new_id.to_string()));
            p.components.insert(i + 1, copy);
        }

        // ------------------------------------------------------- project
        Command::SetIndustrialSecurity { declarations } => {
            p.v2.industrial = declarations.clone();
            cx.changes.push(K::Modified, E::Project, None);
        }
        Command::SetExpressionEnumerations { declarations } => {
            p.v2.bindings = declarations.clone();
            cx.changes.push(K::Modified, E::Project, None);
        }
        Command::SetComponentActions { declarations } => {
            p.v2.components = declarations.clone();
            cx.changes.push(K::Modified, E::Project, None);
        }
        Command::SetMetadata { metadata } => {
            p.metadata = metadata.clone();
            cx.changes.push(K::Modified, E::Metadata, None);
        }
        Command::SetSettings { settings } => {
            p.settings = settings.clone();
            cx.changes.push(K::Modified, E::Settings, None);
        }
        Command::SetNavigation { navigation } => {
            p.navigation = navigation.clone();
            cx.changes.push(K::Modified, E::Navigation, None);
        }
        Command::SetExtension { key, value } => {
            if crate::legacy_quarantine::reserved_extension(key) {
                return Err(crate::error::CommandError::InvalidArgument {
                    message: "import provenance cannot be edited".into(),
                });
            }
            match value {
                Some(v) => {
                    p.extensions.insert(key.clone(), v.clone());
                }
                None => {
                    if p.extensions.remove(key).is_none() {
                        return Err(not_found("extension", key));
                    }
                }
            }
            cx.changes
                .push(K::Modified, E::Extensions, Some(key.clone()));
        }
    }
    Ok(())
}

fn inherited_permissions(
    p: &Project,
    loc: &NodeLocation,
    include_node: bool,
) -> BTreeSet<PermissionId> {
    let mut result = BTreeSet::new();
    if let locate::Tree::Screen(i) = loc.tree {
        if let Some(s) = p.screens.get(i) {
            result.extend(
                p.v2.industrial
                    .screens
                    .iter()
                    .filter(|x| x.screen == s.id)
                    .map(|x| x.required_permission.clone()),
            );
        }
    }
    let count = loc.path.len() + usize::from(include_node);
    for depth in 0..count {
        let ancestor = NodeLocation {
            tree: loc.tree,
            path: loc.path[..depth].to_vec(),
        };
        if let Some(n) = locate::get(p, &ancestor) {
            result.extend(
                p.v2.industrial
                    .nodes
                    .iter()
                    .filter(|x| x.node == n.id)
                    .map(|x| x.required_permission.clone()),
            );
        }
    }
    result
}

fn check_reparent_v2(
    p: &Project,
    source: &NodeLocation,
    target: &NodeLocation,
) -> Result<(), CommandError> {
    if !p.has_v2_features() {
        return Ok(());
    }
    // Definition parameter and instance security contexts are not interchangeable.
    if source.tree != target.tree
        && (matches!(source.tree, locate::Tree::Definition(_))
            || matches!(target.tree, locate::Tree::Definition(_)))
    {
        return Err(CommandError::InvalidArgument {
            message: "cross-definition v2 reparenting requires explicit scope remapping".into(),
        });
    }
    if !inherited_permissions(p, source, false).is_subset(&inherited_permissions(p, target, true)) {
        return Err(CommandError::InvalidArgument {
            message: "reparenting would lose an inherited permission requirement".into(),
        });
    }
    Ok(())
}
