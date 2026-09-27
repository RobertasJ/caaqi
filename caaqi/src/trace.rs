use slotmap::{SlotMap, new_key_type};
use smallvec::SmallVec;

use crate::{
    context::Context,
    current::CurrentActionExt,
    lifecycle::{notify_node_added, notify_nodes_removed},
};

new_key_type! {
    /// Only valid in the `Context` that created it. Using a key with another
    /// context is unsupported and may refer to an unrelated node.
    pub struct TraceKey;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("action node {0:?} isn't in the trace")]
pub struct UnknownNode(pub TraceKey);

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("action node {0:?} is executing")]
pub struct NodeExecuting(pub TraceKey);

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("action node {0:?} has an executing descendant")]
pub struct ExecutingDescendant(pub TraceKey);

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

/// The trace resource. It holds only the shape of the trace; per-node
/// data lives in other resources keyed by [`TraceKey`].
///
/// Everything public goes through [`TraceExt`], so mutation notifies
/// [node observers](crate::lifecycle::NodeObserver).
///
/// Besides parent and children links, every node is linked to its `prev` and
/// `next` node in trace order: each node before its descendants, siblings
/// first to last, and roots in creation order. So a node's `prev` is its
/// previous sibling's last descendant (or that sibling itself, when it's a
/// leaf), or its parent when it's a first child.
#[derive(Default)]
pub struct Trace {
    nodes: SlotMap<TraceKey, TraceNode>,
    /// The first and last node in trace order.
    first: Option<TraceKey>,
    last: Option<TraceKey>,
}

pub(crate) struct TraceNode {
    children: SmallVec<[TraceKey; 1]>,
    parent: Option<TraceKey>,
    prev: Option<TraceKey>,
    next: Option<TraceKey>,
}

/// A walk over a node and its descendants, each before its own descendants.
/// [`skip_children`](Self::skip_children) keeps the walk out of the children
/// of the node it just returned.
///
/// It borrows the trace; use [`into_cursor`](Self::into_cursor) to walk while
/// changing the context.
pub struct TopDownWalk<'a> {
    tree: &'a Trace,
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
    type Item = TraceKey;

    fn next(&mut self) -> Option<TraceKey> {
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
    pending: Vec<TraceKey>,
    /// The node returned last, whose children haven't been queued yet.
    /// They're queued on the next `next` call, so `skip_children` can drop
    /// them first.
    last: Option<TraceKey>,
    siblings_rev: bool,
}

impl TopDownCursor {
    /// Skips the descendants of the node `next` returned last. Does nothing
    /// before the first `next`, or when called twice in a row.
    pub fn skip_children(&mut self) {
        self.last = None;
    }

    /// The next node of the walk, or `None` once it's done.
    pub fn next(&mut self, ctx: &Context) -> Option<TraceKey> {
        // Without a tree, every node has been removed.
        let Some(tree) = ctx.get::<Trace>() else {
            self.pending.clear();
            self.last = None;
            return None;
        };
        self.next_in(tree)
    }

    fn next_in(&mut self, tree: &Trace) -> Option<TraceKey> {
        if let Some(node) = self.last.take().and_then(|last| tree.nodes.get(last)) {
            let children = node.children.iter().copied();
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

impl Trace {
    fn contains(&self, key: TraceKey) -> bool {
        self.nodes.contains_key(key)
    }

    pub(crate) fn node(&self, key: TraceKey) -> Result<&TraceNode, UnknownNode> {
        self.nodes.get(key).ok_or(UnknownNode(key))
    }

    fn parent(&self, key: TraceKey) -> Result<Option<TraceKey>, UnknownNode> {
        Ok(self.node(key)?.parent)
    }

    fn children(&self, key: TraceKey) -> Result<&[TraceKey], UnknownNode> {
        Ok(&self.node(key)?.children)
    }

    fn prev(&self, key: TraceKey) -> Result<Option<TraceKey>, UnknownNode> {
        Ok(self.node(key)?.prev)
    }

    fn next(&self, key: TraceKey) -> Result<Option<TraceKey>, UnknownNode> {
        Ok(self.node(key)?.next)
    }

    /// The last node of `key`'s subtree in trace order: its last child's last
    /// child, and so on, or `key` itself when it's a leaf. `key` must be in
    /// the trace.
    fn subtree_last(&self, key: TraceKey) -> TraceKey {
        std::iter::successors(Some(key), |&key| self.nodes[key].children.last().copied())
            .last()
            .expect("the walk starts at `key`")
    }

    /// Links `prev` and `next` to each other, or makes them the trace's
    /// last and first node when the other side is `None`.
    fn link(&mut self, prev: Option<TraceKey>, next: Option<TraceKey>) {
        match prev {
            Some(prev) => self.nodes[prev].next = next,
            None => self.first = next,
        }
        match next {
            Some(next) => self.nodes[next].prev = prev,
            None => self.last = prev,
        }
    }

    fn ancestors(&self, key: TraceKey) -> Result<impl Iterator<Item = TraceKey> + '_, UnknownNode> {
        let parent = self.parent(key)?;
        // Parents of nodes in the trace are in the trace too.
        Ok(std::iter::successors(parent, |&key| self.nodes[key].parent))
    }

    /// Iterates `key` and its descendants, each before its own descendants,
    /// siblings last to first when `siblings_rev`. `key` must be in the trace.
    fn parents_first(&self, key: TraceKey, siblings_rev: bool) -> TopDownWalk<'_> {
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
    /// siblings last to first when `siblings_rev`. `key` must be in the trace.
    fn children_first(
        &self,
        key: TraceKey,
        siblings_rev: bool,
    ) -> impl Iterator<Item = TraceKey> + '_ {
        // `true` once a node's children have been pushed above it.
        let mut pending = vec![(key, false)];
        std::iter::from_fn(move || {
            loop {
                let (key, expanded) = pending.pop()?;
                if expanded {
                    return Some(key);
                }
                pending.push((key, true));
                let children = self.nodes[key].children.iter().map(|&child| (child, false));
                if siblings_rev {
                    pending.extend(children);
                } else {
                    pending.extend(children.rev());
                }
            }
        })
    }

    fn subtree_top_down(&self, key: TraceKey) -> Result<TopDownWalk<'_>, UnknownNode> {
        self.node(key)?;
        Ok(self.parents_first(key, false))
    }

    fn subtree_top_down_rev(&self, key: TraceKey) -> Result<TopDownWalk<'_>, UnknownNode> {
        self.node(key)?;
        Ok(self.parents_first(key, true))
    }

    fn subtree_bottom_up(
        &self,
        key: TraceKey,
    ) -> Result<impl Iterator<Item = TraceKey> + '_, UnknownNode> {
        self.node(key)?;
        Ok(self.children_first(key, false))
    }

    fn subtree_bottom_up_rev(
        &self,
        key: TraceKey,
    ) -> Result<impl Iterator<Item = TraceKey> + '_, UnknownNode> {
        self.node(key)?;
        Ok(self.children_first(key, true))
    }

    fn insert(&mut self, parent: Option<TraceKey>) -> TraceKey {
        // Checked before inserting so a bad parent can't leave an orphan behind.
        if let Some(parent) = parent {
            assert!(
                self.contains(parent),
                "parent {parent:?} isn't in the trace"
            );
        }
        // A new last child comes right after its parent's subtree, and a new
        // root at the end of the trace.
        let prev = match parent {
            Some(parent) => Some(self.subtree_last(parent)),
            None => self.last,
        };
        let next = prev.and_then(|prev| self.nodes[prev].next);
        // Only fresh nodes can be attached, so parenting cannot create cycles.
        let key = self.nodes.insert(TraceNode {
            children: SmallVec::new(),
            parent,
            prev: None,
            next: None,
        });
        if let Some(parent) = parent {
            self.nodes[parent].children.push(key);
        }
        self.link(prev, Some(key));
        self.link(Some(key), next);
        key
    }

    /// Removes every descendant of `key`, returning the removed keys bottom up.
    fn remove_descendants(&mut self, key: TraceKey) -> Result<Vec<TraceKey>, UnknownNode> {
        let mut removed: Vec<_> = self.subtree_bottom_up(key)?.collect();
        // `key` comes last, and stays.
        removed.pop();
        let after = self.nodes[self.subtree_last(key)].next;
        self.link(Some(key), after);
        for &key in &removed {
            self.nodes.remove(key);
        }
        self.nodes[key].children.clear();
        Ok(removed)
    }

    /// Removes `key` and its descendants, returning the removed keys bottom
    /// up, so `key` comes last.
    fn remove_branch(&mut self, key: TraceKey) -> Result<Vec<TraceKey>, UnknownNode> {
        let mut removed = self.remove_descendants(key)?;
        let node = self
            .nodes
            .remove(key)
            .expect("checked by remove_descendants");
        // With its descendants gone, `key` is linked to the node after its
        // old subtree.
        self.link(node.prev, node.next);
        if let Some(parent) = node.parent {
            self.nodes[parent]
                .children
                .retain(|&mut child| child != key);
        }
        removed.push(key);
        Ok(removed)
    }
}

fn trace_mut(ctx: &mut Context) -> &mut Trace {
    ctx.get_or_insert_with(Trace::default)
}

/// The tree, for a read about `key`. Without a tree no node exists yet.
pub(crate) fn trace_ref(ctx: &Context, key: TraceKey) -> Result<&Trace, UnknownNode> {
    ctx.get::<Trace>().ok_or(UnknownNode(key))
}

pub trait TraceExt {
    /// Whether `key` is in the trace.
    fn contains_node(&self, key: TraceKey) -> bool;

    /// The parent of `key`, or `None` for a root.
    fn parent(&self, key: TraceKey) -> Result<Option<TraceKey>, UnknownNode>;

    fn children(&self, key: TraceKey) -> Result<&[TraceKey], UnknownNode>;

    /// The node before `key` in trace order, or `None` for the first node.
    /// See [`Trace`] for the order.
    fn prev(&self, key: TraceKey) -> Result<Option<TraceKey>, UnknownNode>;

    /// The node after `key` in trace order, or `None` for the last node.
    /// See [`Trace`] for the order.
    fn next(&self, key: TraceKey) -> Result<Option<TraceKey>, UnknownNode>;

    /// Walks up from the parent of `key` to its root.
    fn ancestors(&self, key: TraceKey) -> Result<impl Iterator<Item = TraceKey> + '_, UnknownNode>;

    /// Iterates `key` and its descendants, each before its own descendants,
    /// so `key` comes first. Call
    /// [`skip_children`](TopDownWalk::skip_children) to skip the descendants
    /// of the node just returned.
    fn subtree_top_down(&self, key: TraceKey) -> Result<TopDownWalk<'_>, UnknownNode>;

    /// [`subtree_top_down`](Self::subtree_top_down) with siblings last to
    /// first.
    fn subtree_top_down_rev(&self, key: TraceKey) -> Result<TopDownWalk<'_>, UnknownNode>;

    /// Iterates `key` and its descendants, each after its own descendants, so
    /// `key` comes last.
    fn subtree_bottom_up(
        &self,
        key: TraceKey,
    ) -> Result<impl Iterator<Item = TraceKey> + '_, UnknownNode>;

    /// [`subtree_bottom_up`](Self::subtree_bottom_up) with siblings last to
    /// first.
    fn subtree_bottom_up_rev(
        &self,
        key: TraceKey,
    ) -> Result<impl Iterator<Item = TraceKey> + '_, UnknownNode>;

    /// Creates a root, even when another action is executing.
    fn create_root(&mut self) -> TraceKey;

    /// Creates a child of the executing action, or a root outside execution.
    fn create_branch(&mut self) -> TraceKey;

    /// Whether `key` is the current action or one of its ancestors.
    fn has_executing(&self, key: TraceKey) -> bool;

    /// Whether the current action is a strict descendant of `key`.
    fn has_executing_descendant(&self, key: TraceKey) -> bool;

    /// Removes every descendant of `key`, returning the removed keys bottom
    /// up.
    fn clear_children(&mut self, key: TraceKey) -> Result<Vec<TraceKey>, ClearChildrenError>;

    /// Removes `key` and its descendants, returning the removed keys bottom
    /// up.
    fn remove_node(&mut self, key: TraceKey) -> Result<Vec<TraceKey>, RemoveNodeError>;
}

impl TraceExt for Context {
    fn contains_node(&self, key: TraceKey) -> bool {
        self.get::<Trace>().is_some_and(|tree| tree.contains(key))
    }

    fn parent(&self, key: TraceKey) -> Result<Option<TraceKey>, UnknownNode> {
        trace_ref(self, key)?.parent(key)
    }

    fn children(&self, key: TraceKey) -> Result<&[TraceKey], UnknownNode> {
        trace_ref(self, key)?.children(key)
    }

    fn prev(&self, key: TraceKey) -> Result<Option<TraceKey>, UnknownNode> {
        trace_ref(self, key)?.prev(key)
    }

    fn next(&self, key: TraceKey) -> Result<Option<TraceKey>, UnknownNode> {
        trace_ref(self, key)?.next(key)
    }

    fn ancestors(&self, key: TraceKey) -> Result<impl Iterator<Item = TraceKey> + '_, UnknownNode> {
        trace_ref(self, key)?.ancestors(key)
    }

    fn subtree_top_down(&self, key: TraceKey) -> Result<TopDownWalk<'_>, UnknownNode> {
        trace_ref(self, key)?.subtree_top_down(key)
    }

    fn subtree_top_down_rev(&self, key: TraceKey) -> Result<TopDownWalk<'_>, UnknownNode> {
        trace_ref(self, key)?.subtree_top_down_rev(key)
    }

    fn subtree_bottom_up(
        &self,
        key: TraceKey,
    ) -> Result<impl Iterator<Item = TraceKey> + '_, UnknownNode> {
        trace_ref(self, key)?.subtree_bottom_up(key)
    }

    fn subtree_bottom_up_rev(
        &self,
        key: TraceKey,
    ) -> Result<impl Iterator<Item = TraceKey> + '_, UnknownNode> {
        trace_ref(self, key)?.subtree_bottom_up_rev(key)
    }

    fn create_root(&mut self) -> TraceKey {
        let key = trace_mut(self).insert(None);
        notify_node_added(self, key);
        key
    }

    fn create_branch(&mut self) -> TraceKey {
        let parent = self.current_action().ok();
        let key = trace_mut(self).insert(parent);
        notify_node_added(self, key);
        key
    }

    fn has_executing(&self, key: TraceKey) -> bool {
        self.current_action() == Ok(key) || self.has_executing_descendant(key)
    }

    fn has_executing_descendant(&self, key: TraceKey) -> bool {
        // Actions only run nested inside the current one, so the only
        // executing actions are the current one and its ancestors.
        let Ok(current) = self.current_action() else {
            return false;
        };
        self.ancestors(current)
            .expect("the current action is in the trace")
            .any(|ancestor| ancestor == key)
    }

    fn clear_children(&mut self, key: TraceKey) -> Result<Vec<TraceKey>, ClearChildrenError> {
        // An unknown node can't be executing, so it reaches the trace's check.
        if self.has_executing_descendant(key) {
            return Err(ExecutingDescendant(key).into());
        }
        let removed = trace_mut(self).remove_descendants(key)?;
        notify_nodes_removed(self, &removed);
        Ok(removed)
    }

    fn remove_node(&mut self, key: TraceKey) -> Result<Vec<TraceKey>, RemoveNodeError> {
        // An unknown node can't be executing, so it reaches the trace's check.
        if self.has_executing(key) {
            return Err(NodeExecuting(key).into());
        }
        let removed = trace_mut(self).remove_branch(key)?;
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
    fn tree() -> (Trace, TraceKey, [TraceKey; 5]) {
        let mut tree = Trace::default();
        let root = tree.insert(None);
        let a = tree.insert(Some(root));
        let a1 = tree.insert(Some(a));
        let a2 = tree.insert(Some(a));
        let b = tree.insert(Some(root));
        let b1 = tree.insert(Some(b));
        (tree, root, [a, a1, a2, b, b1])
    }

    /// The whole trace following `next` from the first node, after checking
    /// that `prev` links the same order backwards.
    fn trace_order(tree: &Trace) -> Vec<TraceKey> {
        let forward: Vec<_> =
            std::iter::successors(tree.first, |&key| tree.next(key).unwrap()).collect();
        let mut backward: Vec<_> =
            std::iter::successors(tree.last, |&key| tree.prev(key).unwrap()).collect();
        backward.reverse();
        assert_eq!(forward, backward, "prev and next disagree");
        forward
    }

    #[gtest]
    fn trace_order_is_parents_first() {
        let (tree, root, [a, a1, a2, b, b1]) = tree();
        expect_eq!(trace_order(&tree), [root, a, a1, a2, b, b1]);
        // A first child's prev is its parent; any other node's prev is its
        // previous sibling's last descendant.
        expect_that!(tree.prev(a1).unwrap(), some(eq(a)));
        expect_that!(tree.prev(b).unwrap(), some(eq(a2)));
    }

    #[gtest]
    fn trace_order_puts_roots_after_earlier_roots_subtrees() {
        let (mut tree, root, [a, a1, a2, b, b1]) = tree();
        let root2 = tree.insert(None);
        let c = tree.insert(Some(root2));
        // A child added to an earlier root goes before the later root.
        let a3 = tree.insert(Some(a));
        expect_eq!(trace_order(&tree), [root, a, a1, a2, a3, b, b1, root2, c]);
    }

    #[gtest]
    fn removal_keeps_the_trace_order_linked() {
        let (mut tree, root, [a, a1, a2, b, b1]) = tree();
        let root2 = tree.insert(None);

        tree.remove_branch(a1).unwrap();
        expect_eq!(trace_order(&tree), [root, a, a2, b, b1, root2]);

        tree.remove_descendants(b).unwrap();
        expect_eq!(trace_order(&tree), [root, a, a2, b, root2]);

        tree.remove_branch(root).unwrap();
        expect_eq!(trace_order(&tree), [root2]);

        tree.remove_branch(root2).unwrap();
        expect_eq!(trace_order(&tree), []);
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
    fn ctx_tree() -> (Context, [TraceKey; 4]) {
        let mut ctx = Context::new();
        let root = ctx.create_root();
        let (a, a_child, b) = ctx
            .with_current_action(root, |ctx| {
                let a = ctx.create_branch();
                let a_child = ctx
                    .with_current_action(a, |ctx| ctx.create_branch())
                    .unwrap();
                (a, a_child, ctx.create_branch())
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
        expect_that!(tree.prev(a1), err(eq(UnknownNode(a1))));
        expect_that!(tree.next(a1), err(eq(UnknownNode(a1))));
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
