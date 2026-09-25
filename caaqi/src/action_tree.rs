use slotmap::{SlotMap, new_key_type};
use smallvec::SmallVec;

use crate::{
    context::Context,
    current::{CurrentActionExt, NotExecuting},
    lifecycle::{notify_node_added, notify_nodes_removed},
};

new_key_type! {
    pub struct ActionNodeKey;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("action node {0:?} isn't in the action tree")]
pub struct UnknownNode(pub ActionNodeKey);

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
/// Everything public goes through [`ActionTreeExt`], so mutation notifies
/// [node observers](crate::lifecycle::NodeObserver).
#[derive(Default)]
pub struct ActionTree {
    nodes: SlotMap<ActionNodeKey, ActionNode>,
}

pub(crate) struct ActionNode {
    sub_actions: SmallVec<[ActionNodeKey; 1]>,
    parent: Option<ActionNodeKey>,
}

/// A walk over a node and its descendants, each before its own descendants.
/// [`skip_children`](Self::skip_children) keeps the walk out of the children
/// of the node it just returned.
///
/// It borrows the tree; use [`into_cursor`](Self::into_cursor) to walk while
/// changing the context.
pub struct TopDownWalk<'a> {
    tree: &'a ActionTree,
    cursor: TopDownCursor,
}

impl TopDownWalk<'_> {
    /// Skips the descendants of the node `next` returned last. Does nothing
    /// before the first `next`, or when called twice in a row.
    pub fn skip_children(&mut self) {
        self.cursor.skip_children();
    }

    /// Continues this walk without borrowing the tree.
    pub fn into_cursor(self) -> TopDownCursor {
        self.cursor
    }
}

impl Iterator for TopDownWalk<'_> {
    type Item = ActionNodeKey;

    fn next(&mut self) -> Option<ActionNodeKey> {
        self.cursor.next_in(self.tree)
    }
}

/// A [`TopDownWalk`] that doesn't borrow the tree: each
/// [`next`](Self::next) takes the context instead, so the context can be
/// changed between calls.
///
/// A node's children are read on the `next` call after it's returned, so a
/// node rerun in between has its new children walked, unless
/// [`skip_children`](Self::skip_children) is called. Nodes removed in between
/// are skipped.
pub struct TopDownCursor {
    pending: Vec<ActionNodeKey>,
    /// The node returned last, whose children haven't been queued yet.
    /// They're queued on the next `next` call, so `skip_children` can drop
    /// them first.
    last: Option<ActionNodeKey>,
    siblings_rev: bool,
}

impl TopDownCursor {
    /// Skips the descendants of the node `next` returned last. Does nothing
    /// before the first `next`, or when called twice in a row.
    pub fn skip_children(&mut self) {
        self.last = None;
    }

    /// The next node of the walk, or `None` once it's done.
    pub fn next(&mut self, ctx: &Context) -> Option<ActionNodeKey> {
        // Without a tree, every node has been removed.
        let Some(tree) = ctx.get::<ActionTree>() else {
            self.pending.clear();
            self.last = None;
            return None;
        };
        self.next_in(tree)
    }

    fn next_in(&mut self, tree: &ActionTree) -> Option<ActionNodeKey> {
        if let Some(node) = self.last.take().and_then(|last| tree.nodes.get(last)) {
            let children = node.sub_actions.iter().copied();
            if self.siblings_rev {
                self.pending.extend(children);
            } else {
                self.pending.extend(children.rev());
            }
        }
        // Nodes removed since they were queued are skipped, along with their
        // descendants, which were removed with them.
        let key = std::iter::from_fn(|| self.pending.pop()).find(|&key| tree.contains(key))?;
        self.last = Some(key);
        Some(key)
    }
}

impl ActionTree {
    fn contains(&self, key: ActionNodeKey) -> bool {
        self.nodes.contains_key(key)
    }

    pub(crate) fn node(&self, key: ActionNodeKey) -> Result<&ActionNode, UnknownNode> {
        self.nodes.get(key).ok_or(UnknownNode(key))
    }

    fn parent(&self, key: ActionNodeKey) -> Result<Option<ActionNodeKey>, UnknownNode> {
        Ok(self.node(key)?.parent)
    }

    fn children(&self, key: ActionNodeKey) -> Result<&[ActionNodeKey], UnknownNode> {
        Ok(&self.node(key)?.sub_actions)
    }

    fn ancestors(
        &self,
        key: ActionNodeKey,
    ) -> Result<impl Iterator<Item = ActionNodeKey> + '_, UnknownNode> {
        let parent = self.parent(key)?;
        // Parents of nodes in the tree are in the tree too.
        Ok(std::iter::successors(parent, |&key| self.nodes[key].parent))
    }

    /// Iterates `key` and its descendants, each before its own descendants,
    /// siblings last to first when `siblings_rev`. `key` must be in the tree.
    fn parents_first(&self, key: ActionNodeKey, siblings_rev: bool) -> TopDownWalk<'_> {
        TopDownWalk {
            tree: self,
            cursor: TopDownCursor {
                pending: vec![key],
                last: None,
                siblings_rev,
            },
        }
    }

    /// Iterates `key` and its descendants, each after its own descendants,
    /// siblings last to first when `siblings_rev`. `key` must be in the tree.
    fn children_first(
        &self,
        key: ActionNodeKey,
        siblings_rev: bool,
    ) -> impl Iterator<Item = ActionNodeKey> + '_ {
        // `true` once a node's children have been pushed above it.
        let mut pending = vec![(key, false)];
        std::iter::from_fn(move || {
            loop {
                let (key, expanded) = pending.pop()?;
                if expanded {
                    return Some(key);
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

    fn subtree_top_down(&self, key: ActionNodeKey) -> Result<TopDownWalk<'_>, UnknownNode> {
        self.node(key)?;
        Ok(self.parents_first(key, false))
    }

    fn subtree_top_down_rev(&self, key: ActionNodeKey) -> Result<TopDownWalk<'_>, UnknownNode> {
        self.node(key)?;
        Ok(self.parents_first(key, true))
    }

    fn subtree_bottom_up(
        &self,
        key: ActionNodeKey,
    ) -> Result<impl Iterator<Item = ActionNodeKey> + '_, UnknownNode> {
        self.node(key)?;
        Ok(self.children_first(key, false))
    }

    fn subtree_bottom_up_rev(
        &self,
        key: ActionNodeKey,
    ) -> Result<impl Iterator<Item = ActionNodeKey> + '_, UnknownNode> {
        self.node(key)?;
        Ok(self.children_first(key, true))
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
        let mut removed: Vec<_> = self.subtree_bottom_up(key)?.collect();
        // `key` comes last, and stays.
        removed.pop();
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

/// The tree, for a read about `key`. Without a tree no node exists yet.
pub(crate) fn tree_ref(ctx: &Context, key: ActionNodeKey) -> Result<&ActionTree, UnknownNode> {
    ctx.get::<ActionTree>().ok_or(UnknownNode(key))
}

pub trait ActionTreeExt {
    /// Whether `key` is in the action tree.
    fn contains_node(&self, key: ActionNodeKey) -> bool;

    /// The parent of `key`, or `None` for a root.
    fn parent(&self, key: ActionNodeKey) -> Result<Option<ActionNodeKey>, UnknownNode>;

    fn children(&self, key: ActionNodeKey) -> Result<&[ActionNodeKey], UnknownNode>;

    /// Walks up from the parent of `key` to its root.
    fn ancestors(
        &self,
        key: ActionNodeKey,
    ) -> Result<impl Iterator<Item = ActionNodeKey> + '_, UnknownNode>;

    /// Iterates `key` and its descendants, each before its own descendants,
    /// so `key` comes first. Call
    /// [`skip_children`](TopDownWalk::skip_children) to skip the descendants
    /// of the node just returned.
    fn subtree_top_down(&self, key: ActionNodeKey) -> Result<TopDownWalk<'_>, UnknownNode>;

    /// [`subtree_top_down`](Self::subtree_top_down) with siblings last to
    /// first.
    fn subtree_top_down_rev(&self, key: ActionNodeKey) -> Result<TopDownWalk<'_>, UnknownNode>;

    /// Iterates `key` and its descendants, each after its own descendants, so
    /// `key` comes last.
    fn subtree_bottom_up(
        &self,
        key: ActionNodeKey,
    ) -> Result<impl Iterator<Item = ActionNodeKey> + '_, UnknownNode>;

    /// [`subtree_bottom_up`](Self::subtree_bottom_up) with siblings last to
    /// first.
    fn subtree_bottom_up_rev(
        &self,
        key: ActionNodeKey,
    ) -> Result<impl Iterator<Item = ActionNodeKey> + '_, UnknownNode>;

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
    fn contains_node(&self, key: ActionNodeKey) -> bool {
        self.get::<ActionTree>()
            .is_some_and(|tree| tree.contains(key))
    }

    fn parent(&self, key: ActionNodeKey) -> Result<Option<ActionNodeKey>, UnknownNode> {
        tree_ref(self, key)?.parent(key)
    }

    fn children(&self, key: ActionNodeKey) -> Result<&[ActionNodeKey], UnknownNode> {
        tree_ref(self, key)?.children(key)
    }

    fn ancestors(
        &self,
        key: ActionNodeKey,
    ) -> Result<impl Iterator<Item = ActionNodeKey> + '_, UnknownNode> {
        tree_ref(self, key)?.ancestors(key)
    }

    fn subtree_top_down(&self, key: ActionNodeKey) -> Result<TopDownWalk<'_>, UnknownNode> {
        tree_ref(self, key)?.subtree_top_down(key)
    }

    fn subtree_top_down_rev(&self, key: ActionNodeKey) -> Result<TopDownWalk<'_>, UnknownNode> {
        tree_ref(self, key)?.subtree_top_down_rev(key)
    }

    fn subtree_bottom_up(
        &self,
        key: ActionNodeKey,
    ) -> Result<impl Iterator<Item = ActionNodeKey> + '_, UnknownNode> {
        tree_ref(self, key)?.subtree_bottom_up(key)
    }

    fn subtree_bottom_up_rev(
        &self,
        key: ActionNodeKey,
    ) -> Result<impl Iterator<Item = ActionNodeKey> + '_, UnknownNode> {
        tree_ref(self, key)?.subtree_bottom_up_rev(key)
    }

    fn create_root(&mut self) -> ActionNodeKey {
        let key = tree_mut(self).insert(None);
        notify_node_added(self, key);
        key
    }

    fn create_child(&mut self) -> Result<ActionNodeKey, NotExecuting> {
        let parent = self.current_action()?;
        let key = tree_mut(self).insert(Some(parent));
        notify_node_added(self, key);
        Ok(key)
    }

    fn has_executing(&self, key: ActionNodeKey) -> bool {
        self.current_action() == Ok(key) || self.has_executing_descendant(key)
    }

    fn has_executing_descendant(&self, key: ActionNodeKey) -> bool {
        // Actions only run nested inside the current one, so the only
        // executing actions are the current one and its ancestors.
        let Ok(current) = self.current_action() else {
            return false;
        };
        self.ancestors(current)
            .expect("the current action is in the tree")
            .any(|ancestor| ancestor == key)
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
    fn subtree_top_down_visits_parents_first() {
        let (tree, root, [a, a1, a2, b, b1]) = tree();
        let order: Vec<_> = tree.subtree_top_down(root).unwrap().collect();
        expect_eq!(order, [root, a, a1, a2, b, b1]);
    }

    #[gtest]
    fn skip_children_skips_the_last_returned_subtree() {
        let (tree, root, [a, _, _, b, b1]) = tree();
        let mut walk = tree.subtree_top_down(root).unwrap();
        let mut order = Vec::new();
        while let Some(key) = walk.next() {
            order.push(key);
            if key == a {
                walk.skip_children();
            }
        }
        expect_eq!(order, [root, a, b, b1]);
    }

    #[gtest]
    fn skip_children_before_next_does_nothing() {
        let (tree, root, [a, a1, a2, b, b1]) = tree();
        let mut walk = tree.subtree_top_down(root).unwrap();
        walk.skip_children();
        expect_eq!(walk.collect::<Vec<_>>(), [root, a, a1, a2, b, b1]);
    }

    #[gtest]
    fn skip_children_at_the_start_node_ends_the_walk() {
        let (tree, root, _) = tree();
        let mut walk = tree.subtree_top_down(root).unwrap();
        expect_that!(walk.next(), some(eq(root)));
        walk.skip_children();
        expect_that!(walk.next(), none());
    }

    #[gtest]
    fn subtree_bottom_up_visits_children_first() {
        let (tree, root, [a, a1, a2, b, b1]) = tree();
        let order: Vec<_> = tree.subtree_bottom_up(root).unwrap().collect();
        expect_eq!(order, [a1, a2, a, b1, b, root]);
    }

    #[gtest]
    fn rev_variants_reverse_sibling_order() {
        let (tree, root, [a, a1, a2, b, b1]) = tree();
        expect_eq!(
            tree.subtree_top_down_rev(root).unwrap().collect::<Vec<_>>(),
            [root, b, b1, a, a2, a1]
        );
        expect_eq!(
            tree.subtree_bottom_up_rev(root)
                .unwrap()
                .collect::<Vec<_>>(),
            [b1, b, a2, a1, a, root]
        );
    }

    /// A context with a root that has children `a` and `b`, and `a` has a
    /// child.
    fn ctx_tree() -> (Context, [ActionNodeKey; 4]) {
        let mut ctx = Context::new();
        let root = ctx.create_root();
        let (a, a_child, b) = ctx
            .with_current_action(root, |ctx| {
                let a = ctx.create_child().unwrap();
                let a_child = ctx
                    .with_current_action(a, |ctx| ctx.create_child().unwrap())
                    .unwrap();
                (a, a_child, ctx.create_child().unwrap())
            })
            .unwrap();
        (ctx, [root, a, a_child, b])
    }

    #[gtest]
    fn cursor_walks_while_the_tree_changes() {
        let (mut ctx, [root, a, _, b]) = ctx_tree();
        let mut cursor = ctx.subtree_top_down(root).unwrap().into_cursor();
        let mut order = Vec::new();
        while let Some(key) = cursor.next(&ctx) {
            order.push(key);
            if key == a {
                ctx.clear_children(a).unwrap();
            }
        }
        expect_eq!(order, [root, a, b]);
    }

    #[gtest]
    fn cursor_skips_nodes_removed_after_being_queued() {
        let (mut ctx, [root, a, a_child, b]) = ctx_tree();
        let mut cursor = ctx.subtree_top_down(root).unwrap().into_cursor();
        let mut order = Vec::new();
        while let Some(key) = cursor.next(&ctx) {
            order.push(key);
            if key == a {
                ctx.remove_node(b).unwrap();
            }
        }
        expect_eq!(order, [root, a, a_child]);
    }

    #[gtest]
    fn top_down_rev_can_skip_children() {
        let (tree, root, [a, a1, a2, b, _]) = tree();
        let mut walk = tree.subtree_top_down_rev(root).unwrap();
        let mut order = Vec::new();
        while let Some(key) = walk.next() {
            order.push(key);
            if key == b {
                walk.skip_children();
            }
        }
        expect_eq!(order, [root, b, a, a2, a1]);
    }

    #[gtest]
    fn reads_reject_unknown_nodes() {
        let (mut tree, _, [a, a1, ..]) = tree();
        tree.remove_branch(a).unwrap();

        expect_that!(tree.parent(a1), err(eq(UnknownNode(a1))));
        expect_that!(tree.children(a), err(eq(UnknownNode(a))));
        expect_true!(tree.ancestors(a1).is_err());
        expect_true!(tree.subtree_top_down(a).is_err());
        expect_that!(tree.remove_branch(a), err(eq(&UnknownNode(a))));
        expect_that!(tree.remove_descendants(a), err(eq(&UnknownNode(a))));
    }

    #[gtest]
    fn leaf_subtree_is_just_the_leaf() {
        let (tree, _, [_, a1, ..]) = tree();
        expect_that!(
            tree.subtree_top_down(a1).unwrap().collect::<Vec<_>>(),
            elements_are![eq(&a1)]
        );
        expect_that!(
            tree.subtree_bottom_up_rev(a1).unwrap().collect::<Vec<_>>(),
            elements_are![eq(&a1)]
        );
    }
}
