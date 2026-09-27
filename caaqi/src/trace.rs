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

struct TraceNode {
    children: SmallVec<[TraceKey; 1]>,
    parent: Option<TraceKey>,
    prev: Option<TraceKey>,
    next: Option<TraceKey>,
}

impl Trace {
    fn contains(&self, key: TraceKey) -> bool {
        self.nodes.contains_key(key)
    }

    fn node(&self, key: TraceKey) -> Result<&TraceNode, UnknownNode> {
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

    /// Removes every descendant of `key`, returning the removed keys bottom up
    /// (reverse trace order).
    fn remove_descendants(&mut self, key: TraceKey) -> Result<Vec<TraceKey>, UnknownNode> {
        self.node(key)?;
        // The descendants are the nodes right after `key` in trace order, up
        // to its subtree's last node.
        let last = self.subtree_last(key);
        let mut removed: Vec<_> = std::iter::successors(Some(key), |&node| {
            (node != last).then(|| self.nodes[node].next.expect("`last` comes later"))
        })
        .skip(1)
        .collect();
        removed.reverse();
        let after = self.nodes[last].next;
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
fn trace_ref(ctx: &Context, key: TraceKey) -> Result<&Trace, UnknownNode> {
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
        let parent = |key| {
            self.parent(key)
                .expect("the current action and its ancestors are in the trace")
        };
        std::iter::successors(parent(current), |&key| parent(key)).any(|ancestor| ancestor == key)
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
    fn reads_reject_unknown_nodes() {
        let (mut tree, _, [a, a1, ..]) = tree();
        tree.remove_branch(a).unwrap();

        expect_that!(tree.parent(a1), err(eq(UnknownNode(a1))));
        expect_that!(tree.children(a), err(eq(UnknownNode(a))));
        expect_that!(tree.prev(a1), err(eq(UnknownNode(a1))));
        expect_that!(tree.next(a1), err(eq(UnknownNode(a1))));
        expect_that!(tree.remove_branch(a), err(eq(&UnknownNode(a))));
        expect_that!(tree.remove_descendants(a), err(eq(&UnknownNode(a))));
    }
}
