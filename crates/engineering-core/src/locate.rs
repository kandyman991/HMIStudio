//! Locating nodes inside screen and component-definition trees.

use hmi_project_model::model::{Node, NodeId, Project};

/// Which tree a node lives in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tree {
    /// `project.screens[i].root`.
    Screen(usize),
    /// `project.components[i].root`.
    Definition(usize),
}

/// Position of a node: tree plus child indices from the root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeLocation {
    /// Containing tree.
    pub tree: Tree,
    /// Child-index path from the tree root (empty = the root itself).
    pub path: Vec<usize>,
}

impl NodeLocation {
    /// Whether this is a tree root.
    pub fn is_root(&self) -> bool {
        self.path.is_empty()
    }

    /// Whether `self` is `other` or one of its descendants.
    pub fn is_within(&self, other: &NodeLocation) -> bool {
        self.tree == other.tree && self.path.starts_with(&other.path)
    }
}

fn trees(p: &Project) -> impl Iterator<Item = (Tree, &Node)> {
    p.screens
        .iter()
        .enumerate()
        .map(|(i, s)| (Tree::Screen(i), &s.root))
        .chain(
            p.components
                .iter()
                .enumerate()
                .map(|(i, d)| (Tree::Definition(i), &d.root)),
        )
}

/// Finds the first node with `id` (screens first, then definitions; pre-order).
pub fn find(p: &Project, id: &NodeId) -> Option<NodeLocation> {
    for (tree, root) in trees(p) {
        let mut stack: Vec<(&Node, Vec<usize>)> = vec![(root, Vec::new())];
        while let Some((n, path)) = stack.pop() {
            if &n.id == id {
                return Some(NodeLocation { tree, path });
            }
            for (i, c) in n.children.iter().enumerate().rev() {
                let mut cp = path.clone();
                cp.push(i);
                stack.push((c, cp));
            }
        }
    }
    None
}

fn root_mut(p: &mut Project, tree: Tree) -> Option<&mut Node> {
    match tree {
        Tree::Screen(i) => p.screens.get_mut(i).map(|s| &mut s.root),
        Tree::Definition(i) => p.components.get_mut(i).map(|d| &mut d.root),
    }
}

fn root(p: &Project, tree: Tree) -> Option<&Node> {
    match tree {
        Tree::Screen(i) => p.screens.get(i).map(|s| &s.root),
        Tree::Definition(i) => p.components.get(i).map(|d| &d.root),
    }
}

/// Node at a location.
pub fn get<'a>(p: &'a Project, loc: &NodeLocation) -> Option<&'a Node> {
    let mut n = root(p, loc.tree)?;
    for &i in &loc.path {
        n = n.children.get(i)?;
    }
    Some(n)
}

/// Mutable node at a location.
pub fn get_mut<'a>(p: &'a mut Project, loc: &NodeLocation) -> Option<&'a mut Node> {
    let mut n = root_mut(p, loc.tree)?;
    for &i in &loc.path {
        n = n.children.get_mut(i)?;
    }
    Some(n)
}
