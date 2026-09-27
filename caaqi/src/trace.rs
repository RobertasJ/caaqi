use slotmap::{SlotMap, new_key_type};
use smallvec::SmallVec;

use crate::context::Context;

new_key_type! {
    /// Only valid in the `Context` that created it. Using a key with another
    /// context is unsupported and may refer to an unrelated node.
    pub struct NodeKey;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("node {0:?} isn't in the trace")]
pub struct UnknownNode(pub NodeKey);

/// The parent passed to [`NodeMut::set_parent`] isn't in the trace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("parent {:?} isn't in the trace", .0.0)]
pub struct UnknownParent(#[source] pub UnknownNode);

/// The child passed to [`NodeMut::add_child`] isn't in the trace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("child {:?} isn't in the trace", .0.0)]
pub struct UnknownChild(#[source] pub UnknownNode);

/// The child already has a parent. Reparenting is explicit: call
/// [`detach`](NodeMut::detach) first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error(
    "node {child:?} already has parent {current_parent:?}; \
     call `detach` before attaching it to {requested_parent:?}"
)]
pub struct AlreadyParented {
    pub child: NodeKey,
    pub current_parent: NodeKey,
    pub requested_parent: NodeKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("node {0:?} can't be its own parent")]
pub struct SelfParent(pub NodeKey);

/// The requested parent is a descendant of the child, `depth` levels below
/// it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error(
    "parenting {child:?} under {parent:?} would create a cycle; \
     {parent:?} is {depth} level(s) below {child:?}"
)]
pub struct WouldCycle {
    pub child: NodeKey,
    pub parent: NodeKey,
    pub depth: usize,
}

/// [`NodeMut::delete`] only deletes leaves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error(
    "node {key:?} has {children} child(ren); \
     use `delete_branch`, or detach them first"
)]
pub struct HasChildren {
    pub key: NodeKey,
    pub children: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SetParentError {
    #[error(transparent)]
    UnknownParent(#[from] UnknownParent),
    #[error(transparent)]
    AlreadyParented(#[from] AlreadyParented),
    #[error(transparent)]
    SelfParent(#[from] SelfParent),
    #[error(transparent)]
    WouldCycle(#[from] WouldCycle),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AddChildError {
    #[error(transparent)]
    UnknownChild(#[from] UnknownChild),
    #[error(transparent)]
    AlreadyParented(#[from] AlreadyParented),
    #[error(transparent)]
    SelfParent(#[from] SelfParent),
    #[error(transparent)]
    WouldCycle(#[from] WouldCycle),
}

/// The trace resource. It holds only the shape of the trace; per-node
/// data lives in other resources keyed by [`NodeKey`].
///
/// Everything public goes through [`TraceExt`] and the node handles it
/// returns, [`NodeRef`] and [`NodeMut`].
///
/// Besides parent and children links, every node is linked to its `prev` and
/// `next` node in trace order: each node before its descendants, siblings
/// first to last. So a node's `prev` is its previous sibling's last
/// descendant (or that sibling itself, when it's a leaf), or its parent when
/// it's a first child. Each root's tree has its own order: a root has no
/// `prev`, the last node of its tree has no `next`, and the orders of two
/// roots never link.
#[derive(Default)]
pub struct Trace {
    nodes: SlotMap<NodeKey, Node>,
}

struct Node {
    children: SmallVec<[NodeKey; 1]>,
    parent: Option<NodeKey>,
    prev: Option<NodeKey>,
    next: Option<NodeKey>,
}

impl Trace {
    fn contains(&self, key: NodeKey) -> bool {
        self.nodes.contains_key(key)
    }

    /// The last node of `key`'s subtree in trace order: its last child's last
    /// child, and so on, or `key` itself when it's a leaf. `key` must be in
    /// the trace.
    fn subtree_last(&self, key: NodeKey) -> NodeKey {
        std::iter::successors(Some(key), |&key| {
            self.nodes
                .get(key)
                .expect("`key` and its descendants are in the trace")
                .children
                .last()
                .copied()
        })
        .last()
        .expect("the walk starts at `key`")
    }

    /// Links `first` and `second` to each other. A `None` side ends the order
    /// there.
    fn link(&mut self, first: Option<NodeKey>, second: Option<NodeKey>) {
        if let Some(prev) = first {
            self.nodes
                .get_mut(prev)
                .expect("linked nodes are in the trace")
                .next = second;
        }
        if let Some(next) = second {
            self.nodes
                .get_mut(next)
                .expect("linked nodes are in the trace")
                .prev = first;
        }
    }

    /// Takes the span from `span_start` to `span_end` out of its tree's order, linking
    /// the nodes on either side of it to each other. The span is left as an
    /// order of its own.
    fn cut(&mut self, span_start: NodeKey, span_end: NodeKey) {
        let start = self
            .nodes
            .get(span_start)
            .expect("the span is in the trace");
        let end = self.nodes.get(span_end).expect("the span is in the trace");
        let (prev, next) = (start.prev, end.next);
        self.link(prev, next);
        self.link(None, Some(span_start));
        self.link(Some(span_end), None);
    }

    /// Puts the span from `span_start` to `span_end`, which must be an order of its
    /// own, right after `put_after`.
    fn splice(&mut self, put_after: NodeKey, span_start: NodeKey, span_end: NodeKey) {
        let next = self
            .nodes
            .get(put_after)
            .expect("`put_after` is in the trace")
            .next;
        self.link(Some(put_after), Some(span_start));
        self.link(Some(span_end), next);
    }

    /// Adds an unparented node without children, alone in its own order.
    fn insert_root(&mut self) -> NodeKey {
        self.nodes.insert(Node {
            children: SmallVec::new(),
            parent: None,
            prev: None,
            next: None,
        })
    }

    /// Makes `child` the last child of `parent`, moving `child`'s subtree
    /// right after `parent`'s.
    /// `child` must be in the trace.
    fn attach(&mut self, parent: NodeKey, child: NodeKey) -> Result<(), SetParentError> {
        // check for an unknown parent
        if !self.contains(parent) {
            return Err(UnknownParent(UnknownNode(parent)).into());
        }

        // check if parent exists already
        if let Some(current_parent) = self
            .nodes
            .get(child)
            .expect("the handle's node is in the trace")
            .parent
        {
            return Err(AlreadyParented {
                child,
                current_parent,
                requested_parent: parent,
            }
            .into());
        }

        // check for cycles
        if parent == child {
            return Err(SelfParent(child).into());
        }
        if let Some(depth) = std::iter::successors(Some(parent), |&key| {
            self.nodes
                .get(key)
                .expect("`parent` and its ancestors are in the trace")
                .parent
        })
        .position(|key| key == child)
        {
            return Err(WouldCycle {
                child,
                parent,
                depth,
            }
            .into());
        }

        // new last node of the parent after adding the child
        let last = self.subtree_last(child);
        // previous last node of the parent
        let prev = self.subtree_last(parent);

        // attach the span
        self.splice(prev, child, last);

        // update the tree
        self.nodes
            .get_mut(child)
            .expect("the handle's node is in the trace")
            .parent = Some(parent);
        self.nodes
            .get_mut(parent)
            .expect("checked above")
            .children
            .push(child);
        Ok(())
    }

    /// Detaches `key` from its parent, cutting its subtree out into an order
    /// of its own, like a new root. Returns whether it had a parent.
    fn detach(&mut self, key: NodeKey) -> bool {
        // parent check
        let Some(parent) = self
            .nodes
            .get_mut(key)
            .expect("the handle's node is in the trace")
            .parent
            .take()
        else {
            return false;
        };

        // update the tree
        self.nodes
            .get_mut(parent)
            .expect("a node's parent is in the trace")
            .children
            .retain(|&mut child| child != key);

        // cut the subtree out of its tree's order
        let last = self.subtree_last(key);
        self.cut(key, last);
        true
    }

    /// Removes `key`, which must be a leaf, from the trace.
    fn remove_leaf(&mut self, key: NodeKey) -> Result<(), HasChildren> {
        // children check
        let children = self
            .nodes
            .get(key)
            .expect("the handle's node is in the trace")
            .children
            .len();
        if children > 0 {
            return Err(HasChildren { key, children });
        }

        let node = self.nodes.remove(key).expect("checked above");
        if let Some(parent) = node.parent {
            self.nodes
                .get_mut(parent)
                .expect("a node's parent is in the trace")
                .children
                .retain(|&mut child| child != key);
        }
        self.link(node.prev, node.next);
        Ok(())
    }
}

fn trace_mut(ctx: &mut Context) -> &mut Trace {
    ctx.get_or_insert_with(Trace::default)
}

/// Read access to a node in the trace, returned by [`TraceExt::node`].
#[derive(Clone, Copy)]
pub struct NodeRef<'a> {
    ctx: &'a Context,
    key: NodeKey,
}

impl<'a> NodeRef<'a> {
    fn data(&self) -> &'a Node {
        self.ctx
            .get::<Trace>()
            .expect("a node exists, so the trace does")
            .nodes
            .get(self.key)
            .expect("the handle's node is in the trace")
    }

    pub fn id(&self) -> NodeKey {
        self.key
    }

    /// The context this handle reads from, for extension traits that add
    /// per-node methods.
    pub fn context(&self) -> &'a Context {
        self.ctx
    }

    /// The parent of this node, or `None` for a root.
    pub fn parent(&self) -> Option<NodeKey> {
        self.data().parent
    }

    /// The children of this node, first to last.
    pub fn children(&self) -> &'a [NodeKey] {
        &self.data().children
    }

    /// The node before this one in trace order, or `None` for a root.
    /// See [`Trace`] for the order.
    pub fn prev(&self) -> Option<NodeKey> {
        self.data().prev
    }

    /// The node after this one in trace order, or `None` for the last node of
    /// its tree.
    /// See [`Trace`] for the order.
    pub fn next(&self) -> Option<NodeKey> {
        self.data().next
    }
}

/// Write access to a node in the trace, returned by [`TraceExt::node_mut`]
/// and [`TraceExt::create_node`].
///
/// Reparenting is explicit: a node that has a parent must be detached with
/// [`detach`](Self::detach) before it's attached elsewhere. A
/// node's whole subtree moves with it.
pub struct NodeMut<'a> {
    ctx: &'a mut Context,
    key: NodeKey,
}

impl NodeMut<'_> {
    fn trace_mut(&mut self) -> &mut Trace {
        self.ctx
            .get_mut::<Trace>()
            .expect("a node exists, so the trace does")
    }

    pub fn id(&self) -> NodeKey {
        self.key
    }

    /// The context this handle writes to, for extension traits that add
    /// per-node methods.
    pub fn context(&self) -> &Context {
        self.ctx
    }

    /// Mutable access to the context this handle writes to, for extension
    /// traits that add per-node methods.
    ///
    /// # Danger
    ///
    /// Code using the context directly can delete this node or change the
    /// trace behind the handle's back. After that, the handle's methods may
    /// panic. Extension traits must leave the handle's node in place.
    pub fn context_mut(&mut self) -> &mut Context {
        self.ctx
    }

    pub fn as_ref(&self) -> NodeRef<'_> {
        NodeRef {
            ctx: self.ctx,
            key: self.key,
        }
    }

    /// The parent of this node, or `None` for a root.
    pub fn parent(&self) -> Option<NodeKey> {
        self.as_ref().parent()
    }

    /// The children of this node, first to last.
    pub fn children(&self) -> &[NodeKey] {
        self.as_ref().children()
    }

    /// The node before this one in trace order, or `None` for a root.
    pub fn prev(&self) -> Option<NodeKey> {
        self.as_ref().prev()
    }

    /// The node after this one in trace order, or `None` for the last node of
    /// its tree.
    pub fn next(&self) -> Option<NodeKey> {
        self.as_ref().next()
    }

    /// Makes this node the last child of `parent`.
    ///
    /// Fails if `parent` isn't in the trace, if this node already has a
    /// parent, or if `parent` is this node or one of its descendants.
    pub fn set_parent(&mut self, parent: NodeKey) -> Result<&mut Self, SetParentError> {
        let key = self.key;
        self.trace_mut().attach(parent, key)?;
        Ok(self)
    }

    /// Makes `child` this node's last child.
    ///
    /// Fails if `child` isn't in the trace, if it already has a parent, or if
    /// it's this node or one of its ancestors.
    pub fn add_child(&mut self, child: NodeKey) -> Result<&mut Self, AddChildError> {
        let parent = self.key;
        self.ctx
            .node_mut(child)
            .map_err(UnknownChild)?
            .set_parent(parent)
            .map_err(|error| match error {
                SetParentError::UnknownParent(_) => unreachable!("this node is in the trace"),
                SetParentError::AlreadyParented(error) => AddChildError::from(error),
                SetParentError::SelfParent(error) => AddChildError::from(error),
                SetParentError::WouldCycle(error) => AddChildError::from(error),
            })?;
        Ok(self)
    }

    /// Detaches this node from its parent, making it a root whose subtree is
    /// an order of its own. Returns whether it had one.
    pub fn detach(&mut self) -> bool {
        let key = self.key;
        self.trace_mut().detach(key)
    }

    /// Deletes this node, which must be a leaf. Detaches it from its parent
    /// first if it has one.
    pub fn delete(self) -> Result<(), HasChildren> {
        let Self { ctx, key } = self;
        trace_mut(ctx).remove_leaf(key)
    }

    /// Deletes this node and its descendants, bottom up, one
    /// [`delete`](Self::delete) at a time. Returns the deleted keys bottom
    /// up, so this node's key is last.
    pub fn delete_branch(self) -> Vec<NodeKey> {
        let Self { ctx, key } = self;
        let trace = ctx
            .get::<Trace>()
            .expect("a node exists, so the trace does");
        // Reverse trace order puts every node after its descendants.
        let last = trace.subtree_last(key);
        let deleted: Vec<_> = std::iter::successors(Some(last), |&node| {
            (node != key).then(|| trace.nodes[node].prev.expect("`key` comes earlier"))
        })
        .collect();
        for &node in &deleted {
            ctx.node_mut(node)
                .expect("the subtree is in the trace")
                .delete()
                .expect("descendants are deleted first");
        }
        deleted
    }
}

pub trait TraceExt {
    /// Whether `key` is in the trace.
    fn contains_node(&self, key: NodeKey) -> bool;

    /// Read access to `key`.
    fn node(&self, key: NodeKey) -> Result<NodeRef<'_>, UnknownNode>;

    /// Write access to `key`.
    fn node_mut(&mut self, key: NodeKey) -> Result<NodeMut<'_>, UnknownNode>;

    /// Creates a node that is not parented and has no children. It's a root
    /// until attached with [`set_parent`](NodeMut::set_parent) or
    /// [`add_child`](NodeMut::add_child), alone in its own trace order.
    fn create_node(&mut self) -> NodeMut<'_>;
}

impl TraceExt for Context {
    fn contains_node(&self, key: NodeKey) -> bool {
        self.get::<Trace>().is_some_and(|trace| trace.contains(key))
    }

    fn node(&self, key: NodeKey) -> Result<NodeRef<'_>, UnknownNode> {
        if !self.contains_node(key) {
            return Err(UnknownNode(key));
        }
        Ok(NodeRef { ctx: self, key })
    }

    fn node_mut(&mut self, key: NodeKey) -> Result<NodeMut<'_>, UnknownNode> {
        if !self.contains_node(key) {
            return Err(UnknownNode(key));
        }
        Ok(NodeMut { ctx: self, key })
    }

    fn create_node(&mut self) -> NodeMut<'_> {
        let key = trace_mut(self).insert_root();
        NodeMut { ctx: self, key }
    }
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    fn child(ctx: &mut Context, parent: NodeKey) -> NodeKey {
        let mut node = ctx.create_node();
        node.set_parent(parent).unwrap();
        node.id()
    }

    /// ```text
    /// root
    /// ├── a
    /// │   ├── a1
    /// │   └── a2
    /// └── b
    ///     └── b1
    /// ```
    fn tree() -> (Context, NodeKey, [NodeKey; 5]) {
        let mut ctx = Context::new();
        let root = ctx.create_node().id();
        let a = child(&mut ctx, root);
        let a1 = child(&mut ctx, a);
        let a2 = child(&mut ctx, a);
        let b = child(&mut ctx, root);
        let b1 = child(&mut ctx, b);
        (ctx, root, [a, a1, a2, b, b1])
    }

    /// `root`'s tree following `next` from `root`, after checking that the
    /// order starts at `root`, stays inside its tree, and that `prev` links it
    /// backwards.
    fn tree_order(ctx: &Context, root: NodeKey) -> Vec<NodeKey> {
        assert_eq!(ctx.node(root).unwrap().parent(), None, "not a root");
        assert_eq!(ctx.node(root).unwrap().prev(), None, "a root has no prev");
        let forward: Vec<_> =
            std::iter::successors(Some(root), |&key| ctx.node(key).unwrap().next()).collect();
        for &key in &forward {
            let key_root = std::iter::successors(Some(key), |&key| ctx.node(key).unwrap().parent())
                .last()
                .unwrap();
            assert_eq!(key_root, root, "{key:?} is in another tree");
        }
        let last = *forward.last().unwrap();
        let mut backward: Vec<_> =
            std::iter::successors(Some(last), |&key| ctx.node(key).unwrap().prev()).collect();
        backward.reverse();
        assert_eq!(forward, backward, "prev and next disagree");
        forward
    }

    #[gtest]
    fn trace_order_is_parents_first() {
        let (ctx, root, [a, a1, a2, b, b1]) = tree();
        expect_eq!(tree_order(&ctx, root), [root, a, a1, a2, b, b1]);
        // A first child's prev is its parent; any other node's prev is its
        // previous sibling's last descendant.
        expect_that!(ctx.node(a1).unwrap().prev(), some(eq(a)));
        expect_that!(ctx.node(b).unwrap().prev(), some(eq(a2)));
    }

    #[gtest]
    fn roots_have_separate_orders() {
        let (mut ctx, root, [a, a1, a2, b, b1]) = tree();
        let root2 = ctx.create_node().id();
        let c = child(&mut ctx, root2);
        let a3 = child(&mut ctx, a);
        expect_eq!(tree_order(&ctx, root), [root, a, a1, a2, a3, b, b1]);
        expect_eq!(tree_order(&ctx, root2), [root2, c]);
        expect_that!(ctx.node(b1).unwrap().next(), none());
    }

    #[gtest]
    fn deletion_keeps_the_trace_order_linked() {
        let (mut ctx, root, [a, a1, a2, b, b1]) = tree();
        let root2 = ctx.create_node().id();

        ctx.node_mut(a1).unwrap().delete().unwrap();
        expect_eq!(tree_order(&ctx, root), [root, a, a2, b, b1]);
        expect_eq!(ctx.node(a).unwrap().children(), &[a2]);

        ctx.node_mut(b1).unwrap().delete_branch();
        expect_eq!(tree_order(&ctx, root), [root, a, a2, b]);

        ctx.node_mut(a).unwrap().delete_branch();
        expect_eq!(tree_order(&ctx, root), [root, b]);

        ctx.node_mut(root).unwrap().delete_branch();
        expect_false!(ctx.contains_node(root));
        expect_eq!(tree_order(&ctx, root2), [root2]);
    }

    #[gtest]
    fn set_parent_and_add_child_attach_as_last_child() {
        let (mut ctx, root, [a, a1, a2, b, b1]) = tree();
        let c = ctx.create_node().id();
        let d = ctx.create_node().id();

        ctx.node_mut(c).unwrap().set_parent(a).unwrap();
        expect_eq!(ctx.node(a).unwrap().children(), &[a1, a2, c]);
        expect_that!(ctx.node(c).unwrap().parent(), some(eq(a)));

        ctx.node_mut(a).unwrap().add_child(d).unwrap();
        expect_eq!(ctx.node(a).unwrap().children(), &[a1, a2, c, d]);
        expect_that!(ctx.node(d).unwrap().parent(), some(eq(a)));

        expect_eq!(tree_order(&ctx, root), [root, a, a1, a2, c, d, b, b1]);
    }

    #[gtest]
    fn attaching_a_parented_node_is_refused() {
        let (mut ctx, root, [a, a1, _, b, _]) = tree();
        let a1_under_b = AlreadyParented {
            child: a1,
            current_parent: a,
            requested_parent: b,
        };

        expect_that!(
            ctx.node_mut(a1).unwrap().set_parent(b).err(),
            some(eq(SetParentError::AlreadyParented(a1_under_b)))
        );
        expect_that!(
            ctx.node_mut(b).unwrap().add_child(a1).err(),
            some(eq(AddChildError::AlreadyParented(a1_under_b)))
        );
        // Even under its own parent.
        expect_that!(
            ctx.node_mut(root).unwrap().add_child(b).err(),
            some(eq(AddChildError::AlreadyParented(AlreadyParented {
                child: b,
                current_parent: root,
                requested_parent: root,
            })))
        );
        expect_that!(ctx.node(a1).unwrap().parent(), some(eq(a)));
    }

    #[gtest]
    fn self_parenting_is_refused() {
        let (mut ctx, root, _) = tree();

        expect_that!(
            ctx.node_mut(root).unwrap().set_parent(root).err(),
            some(eq(SetParentError::SelfParent(SelfParent(root))))
        );
        expect_that!(
            ctx.node_mut(root).unwrap().add_child(root).err(),
            some(eq(AddChildError::SelfParent(SelfParent(root))))
        );
        expect_that!(ctx.node(root).unwrap().parent(), none());
    }

    #[gtest]
    fn cycles_are_refused_with_their_depth() {
        let (mut ctx, root, [a, a1, ..]) = tree();

        expect_that!(
            ctx.node_mut(root).unwrap().set_parent(a).err(),
            some(eq(SetParentError::WouldCycle(WouldCycle {
                child: root,
                parent: a,
                depth: 1,
            })))
        );
        expect_that!(
            ctx.node_mut(root).unwrap().set_parent(a1).err(),
            some(eq(SetParentError::WouldCycle(WouldCycle {
                child: root,
                parent: a1,
                depth: 2,
            })))
        );
        expect_that!(
            ctx.node_mut(a1).unwrap().add_child(root).err(),
            some(eq(AddChildError::WouldCycle(WouldCycle {
                child: root,
                parent: a1,
                depth: 2,
            })))
        );
        expect_that!(ctx.node(root).unwrap().parent(), none());
        expect_eq!(tree_order(&ctx, root)[..3], [root, a, a1]);
    }

    #[gtest]
    fn unknown_other_keys_are_refused_by_role() {
        let (mut ctx, root, [a, ..]) = tree();
        let deleted = ctx.create_node().id();
        ctx.node_mut(deleted).unwrap().delete().unwrap();

        expect_that!(
            ctx.node_mut(a).unwrap().add_child(deleted).err(),
            some(eq(AddChildError::UnknownChild(UnknownChild(UnknownNode(
                deleted
            )))))
        );
        ctx.node_mut(a).unwrap().detach();
        expect_that!(
            ctx.node_mut(a).unwrap().set_parent(deleted).err(),
            some(eq(SetParentError::UnknownParent(UnknownParent(
                UnknownNode(deleted)
            ))))
        );
        expect_that!(ctx.node(a).unwrap().parent(), none());
        expect_that!(ctx.node(root).unwrap().children().len(), eq(1));
    }

    #[gtest]
    fn deleted_nodes_are_unknown() {
        let (mut ctx, _, [a, a1, ..]) = tree();
        ctx.node_mut(a).unwrap().delete_branch();

        expect_false!(ctx.contains_node(a1));
        expect_that!(ctx.node(a1).err(), some(eq(UnknownNode(a1))));
        expect_that!(ctx.node_mut(a).err(), some(eq(UnknownNode(a))));
    }

    #[gtest]
    fn detach_reports_whether_there_was_a_parent() {
        let (mut ctx, root, [a, _, _, b, _]) = tree();
        let mut node = ctx.node_mut(a).unwrap();

        expect_true!(node.detach());
        expect_false!(node.detach());
        expect_that!(node.parent(), none());
        expect_eq!(ctx.node(root).unwrap().children(), &[b]);
    }

    #[gtest]
    fn delete_refuses_nodes_with_children() {
        let (mut ctx, _, [a, a1, a2, ..]) = tree();

        expect_that!(
            ctx.node_mut(a).unwrap().delete().err(),
            some(eq(HasChildren {
                key: a,
                children: 2
            }))
        );
        expect_eq!(ctx.node(a).unwrap().children(), &[a1, a2]);
    }

    #[gtest]
    fn delete_branch_returns_keys_bottom_up() {
        let (mut ctx, root, [a, a1, a2, b, b1]) = tree();

        expect_eq!(ctx.node_mut(a).unwrap().delete_branch(), [a2, a1, a]);
        expect_eq!(ctx.node_mut(root).unwrap().delete_branch(), [b1, b, root]);
        expect_false!(ctx.contains_node(root));
    }

    #[gtest]
    fn moving_subtrees_keeps_the_trace_order_linked() {
        let (mut ctx, root, [a, a1, a2, b, b1]) = tree();
        let root2 = ctx.create_node().id();

        // Detaching cuts the whole subtree out into an order of its own.
        ctx.node_mut(a).unwrap().detach();
        expect_eq!(tree_order(&ctx, root), [root, b, b1]);
        expect_eq!(tree_order(&ctx, a), [a, a1, a2]);

        // Attaching puts it right after the new parent's last descendant.
        ctx.node_mut(a).unwrap().set_parent(b).unwrap();
        expect_eq!(tree_order(&ctx, root), [root, b, b1, a, a1, a2]);

        ctx.node_mut(a1).unwrap().add_child(root2).unwrap();
        expect_eq!(tree_order(&ctx, root), [root, b, b1, a, a1, root2, a2]);

        ctx.node_mut(b).unwrap().detach();
        expect_eq!(tree_order(&ctx, root), [root]);
        expect_eq!(tree_order(&ctx, b), [b, b1, a, a1, root2, a2]);

        ctx.node_mut(b).unwrap().set_parent(root).unwrap();
        ctx.node_mut(b1).unwrap().detach();
        expect_eq!(tree_order(&ctx, root), [root, b, a, a1, root2, a2]);
        expect_eq!(tree_order(&ctx, b1), [b1]);
    }
}
