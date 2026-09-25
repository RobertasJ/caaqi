use slotmap::{SlotMap, new_key_type};
use smallvec::SmallVec;

use crate::{
    context::Context,
    current::CurrentActionExt,
    lifecycle::{notify_node_added, notify_nodes_removed},
};

new_key_type! {
    pub struct ActionNodeKey;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("action node {0:?} isn't in the action tree")]
pub struct UnknownNode(pub ActionNodeKey);

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("no action is executing")]
pub struct NotExecuting;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("action node {0:?} is executing")]
pub struct NodeExecuting(pub ActionNodeKey);

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("action node {0:?} has an executing descendant")]
pub struct ExecutingDescendant(pub ActionNodeKey);

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ClearChildrenError {
    #[error(transparent)]
    UnknownNode(#[from] UnknownNode),
    #[error(transparent)]
    ExecutingDescendant(#[from] ExecutingDescendant),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RemoveNodeError {
    #[error(transparent)]
    UnknownNode(#[from] UnknownNode),
    #[error(transparent)]
    NodeExecuting(#[from] NodeExecuting),
}

/// The action tree resource. It holds only the shape of the tree; per-node
/// data lives in other resources keyed by [`ActionNodeKey`].
///
/// Its read methods are public so other modules can build on it; mutation goes
/// through [`ActionTreeExt`] so [node observers](crate::lifecycle::NodeObserver)
/// are notified.
#[derive(Default)]
pub struct ActionTree {
    nodes: SlotMap<ActionNodeKey, ActionNode>,
}

pub(crate) struct ActionNode {
    sub_actions: SmallVec<[ActionNodeKey; 1]>,
    parent: Option<ActionNodeKey>,
}

impl ActionTree {
    pub fn contains(&self, key: ActionNodeKey) -> bool {
        self.nodes.contains_key(key)
    }

    pub(crate) fn node(&self, key: ActionNodeKey) -> Result<&ActionNode, UnknownNode> {
        self.nodes.get(key).ok_or(UnknownNode(key))
    }

    /// The parent of `key`, or `None` for a root.
    pub fn parent(&self, key: ActionNodeKey) -> Result<Option<ActionNodeKey>, UnknownNode> {
        Ok(self.node(key)?.parent)
    }

    pub fn children(&self, key: ActionNodeKey) -> Result<&[ActionNodeKey], UnknownNode> {
        Ok(&self.node(key)?.sub_actions)
    }

    /// Walks up from the parent of `key` to its root.
    pub fn ancestors(
        &self,
        key: ActionNodeKey,
    ) -> Result<impl Iterator<Item = ActionNodeKey> + '_, UnknownNode> {
        let parent = self.parent(key)?;
        // Parents of nodes in the tree are in the tree too.
        Ok(std::iter::successors(parent, |&key| self.nodes[key].parent))
    }

    /// Iterates the descendants of `key`, each before its own descendants,
    /// siblings last to first when `siblings_rev`. `key` must be in the tree.
    fn parents_first(
        &self,
        key: ActionNodeKey,
        siblings_rev: bool,
    ) -> impl Iterator<Item = ActionNodeKey> + '_ {
        let mut pending = vec![key];
        std::iter::from_fn(move || {
            let key = pending.pop()?;
            let children = self.nodes[key].sub_actions.iter().copied();
            if siblings_rev {
                pending.extend(children);
            } else {
                pending.extend(children.rev());
            }
            Some(key)
        })
        // `key` itself comes first.
        .skip(1)
    }

    /// Iterates the descendants of `key`, each after its own descendants,
    /// siblings last to first when `siblings_rev`. `key` must be in the tree.
    fn children_first(
        &self,
        key: ActionNodeKey,
        siblings_rev: bool,
    ) -> impl Iterator<Item = ActionNodeKey> + '_ {
        let root = key;
        // `true` once a node's children have been pushed above it.
        let mut pending = vec![(key, false)];
        std::iter::from_fn(move || {
            loop {
                let (key, expanded) = pending.pop()?;
                if expanded {
                    // `root` comes last, so stop there.
                    return (key != root).then_some(key);
                }
                pending.push((key, true));
                let children = self.nodes[key]
                    .sub_actions
                    .iter()
                    .map(|&child| (child, false));
                if siblings_rev {
                    pending.extend(children);
                } else {
                    pending.extend(children.rev());
                }
            }
        })
    }

    /// Iterates the descendants of `key`, each before its own descendants.
    pub fn descendants_top_down(
        &self,
        key: ActionNodeKey,
    ) -> Result<impl Iterator<Item = ActionNodeKey> + '_, UnknownNode> {
        self.node(key)?;
        Ok(self.parents_first(key, false))
    }

    /// Iterates [`descendants_top_down`](Self::descendants_top_down) in
    /// reverse: each after its own descendants, siblings last to first.
    pub fn descendants_top_down_rev(
        &self,
        key: ActionNodeKey,
    ) -> Result<impl Iterator<Item = ActionNodeKey> + '_, UnknownNode> {
        self.node(key)?;
        Ok(self.children_first(key, true))
    }

    /// Iterates the descendants of `key`, each after its own descendants.
    pub fn descendants_bottom_up(
        &self,
        key: ActionNodeKey,
    ) -> Result<impl Iterator<Item = ActionNodeKey> + '_, UnknownNode> {
        self.node(key)?;
        Ok(self.children_first(key, false))
    }

    /// Iterates [`descendants_bottom_up`](Self::descendants_bottom_up) in
    /// reverse: each before its own descendants, siblings last to first.
    pub fn descendants_bottom_up_rev(
        &self,
        key: ActionNodeKey,
    ) -> Result<impl Iterator<Item = ActionNodeKey> + '_, UnknownNode> {
        self.node(key)?;
        Ok(self.parents_first(key, true))
    }

    fn insert(&mut self, parent: Option<ActionNodeKey>) -> ActionNodeKey {
        // Checked before inserting so a bad parent can't leave an orphan behind.
        if let Some(parent) = parent {
            assert!(
                self.contains(parent),
                "parent {parent:?} isn't in the action tree"
            );
        }
        // Only fresh nodes can be attached, so parenting cannot create cycles.
        let key = self.nodes.insert(ActionNode {
            sub_actions: SmallVec::new(),
            parent,
        });
        if let Some(parent) = parent {
            self.nodes[parent].sub_actions.push(key);
        }
        key
    }

    /// Removes every descendant of `key`, returning the removed keys bottom up.
    fn remove_descendants(
        &mut self,
        key: ActionNodeKey,
    ) -> Result<Vec<ActionNodeKey>, UnknownNode> {
        let removed: Vec<_> = self.descendants_bottom_up(key)?.collect();
        for &key in &removed {
            self.nodes.remove(key);
        }
        self.nodes[key].sub_actions.clear();
        Ok(removed)
    }

    /// Removes `key` and its descendants, returning the removed keys bottom
    /// up, so `key` comes last.
    fn remove_branch(&mut self, key: ActionNodeKey) -> Result<Vec<ActionNodeKey>, UnknownNode> {
        let mut removed = self.remove_descendants(key)?;
        let node = self
            .nodes
            .remove(key)
            .expect("checked by remove_descendants");
        if let Some(parent) = node.parent {
            self.nodes[parent]
                .sub_actions
                .retain(|&mut child| child != key);
        }
        removed.push(key);
        Ok(removed)
    }
}

fn tree_mut(ctx: &mut Context) -> &mut ActionTree {
    ctx.get_or_insert_with(ActionTree::default)
}

pub trait ActionTreeExt {
    /// Creates a root, even when another action is executing.
    fn create_root(&mut self) -> ActionNodeKey;

    /// Creates a child of the executing action.
    fn create_child(&mut self) -> Result<ActionNodeKey, NotExecuting>;

    /// Whether `key` is the current action or one of its ancestors.
    fn has_executing(&self, key: ActionNodeKey) -> bool;

    /// Whether the current action is a strict descendant of `key`.
    fn has_executing_descendant(&self, key: ActionNodeKey) -> bool;

    /// Removes every descendant of `key`, returning the removed keys bottom
    /// up.
    fn clear_children(
        &mut self,
        key: ActionNodeKey,
    ) -> Result<Vec<ActionNodeKey>, ClearChildrenError>;

    /// Removes `key` and its descendants, returning the removed keys bottom
    /// up.
    fn remove_node(&mut self, key: ActionNodeKey) -> Result<Vec<ActionNodeKey>, RemoveNodeError>;
}

impl ActionTreeExt for Context {
    fn create_root(&mut self) -> ActionNodeKey {
        let key = tree_mut(self).insert(None);
        notify_node_added(self, key);
        key
    }

    fn create_child(&mut self) -> Result<ActionNodeKey, NotExecuting> {
        let parent = self.current_action().ok_or(NotExecuting)?;
        let key = tree_mut(self).insert(Some(parent));
        notify_node_added(self, key);
        Ok(key)
    }

    fn has_executing(&self, key: ActionNodeKey) -> bool {
        self.current_action() == Some(key) || self.has_executing_descendant(key)
    }

    fn has_executing_descendant(&self, key: ActionNodeKey) -> bool {
        // Actions aren't stored, so the only executing actions are the
        // current one and its ancestors.
        let Some(current) = self.current_action() else {
            return false;
        };
        self.get::<ActionTree>().is_some_and(|tree| {
            tree.ancestors(current)
                .expect("the current action is in the tree")
                .any(|ancestor| ancestor == key)
        })
    }

    fn clear_children(
        &mut self,
        key: ActionNodeKey,
    ) -> Result<Vec<ActionNodeKey>, ClearChildrenError> {
        // An unknown node can't be executing, so it reaches the tree's check.
        if self.has_executing_descendant(key) {
            return Err(ExecutingDescendant(key).into());
        }
        let removed = tree_mut(self).remove_descendants(key)?;
        notify_nodes_removed(self, &removed);
        Ok(removed)
    }

    fn remove_node(&mut self, key: ActionNodeKey) -> Result<Vec<ActionNodeKey>, RemoveNodeError> {
        // An unknown node can't be executing, so it reaches the tree's check.
        if self.has_executing(key) {
            return Err(NodeExecuting(key).into());
        }
        let removed = tree_mut(self).remove_branch(key)?;
        notify_nodes_removed(self, &removed);
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    /// ```text
    /// root
    /// ├── a
    /// │   ├── a1
    /// │   └── a2
    /// └── b
    ///     └── b1
    /// ```
    fn tree() -> (ActionTree, ActionNodeKey, [ActionNodeKey; 5]) {
        let mut tree = ActionTree::default();
        let root = tree.insert(None);
        let a = tree.insert(Some(root));
        let a1 = tree.insert(Some(a));
        let a2 = tree.insert(Some(a));
        let b = tree.insert(Some(root));
        let b1 = tree.insert(Some(b));
        (tree, root, [a, a1, a2, b, b1])
    }

    #[gtest]
    fn descendants_top_down_visits_parents_first() {
        let (tree, root, [a, a1, a2, b, b1]) = tree();
        let order: Vec<_> = tree.descendants_top_down(root).unwrap().collect();
        expect_eq!(order, [a, a1, a2, b, b1]);
    }

    #[gtest]
    fn descendants_bottom_up_visits_children_first() {
        let (tree, root, [a, a1, a2, b, b1]) = tree();
        let order: Vec<_> = tree.descendants_bottom_up(root).unwrap().collect();
        expect_eq!(order, [a1, a2, a, b1, b]);
    }

    #[gtest]
    fn rev_variants_reverse_their_forward_order() {
        let (tree, root, _) = tree();
        let mut top_down: Vec<_> = tree.descendants_top_down(root).unwrap().collect();
        top_down.reverse();
        let mut bottom_up: Vec<_> = tree.descendants_bottom_up(root).unwrap().collect();
        bottom_up.reverse();

        expect_eq!(
            tree.descendants_top_down_rev(root)
                .unwrap()
                .collect::<Vec<_>>(),
            top_down
        );
        expect_eq!(
            tree.descendants_bottom_up_rev(root)
                .unwrap()
                .collect::<Vec<_>>(),
            bottom_up
        );
    }

    #[gtest]
    fn reads_reject_unknown_nodes() {
        let (mut tree, _, [a, a1, ..]) = tree();
        tree.remove_branch(a).unwrap();

        expect_that!(tree.parent(a1), err(eq(UnknownNode(a1))));
        expect_that!(tree.children(a), err(eq(UnknownNode(a))));
        expect_true!(tree.ancestors(a1).is_err());
        expect_true!(tree.descendants_top_down(a).is_err());
        expect_that!(tree.remove_branch(a), err(eq(&UnknownNode(a))));
        expect_that!(tree.remove_descendants(a), err(eq(&UnknownNode(a))));
    }

    #[gtest]
    fn leaf_has_no_descendants() {
        let (tree, _, [_, a1, ..]) = tree();
        expect_that!(
            tree.descendants_top_down(a1).unwrap().collect::<Vec<_>>(),
            is_empty()
        );
        expect_that!(
            tree.descendants_bottom_up_rev(a1)
                .unwrap()
                .collect::<Vec<_>>(),
            is_empty()
        );
    }
}
