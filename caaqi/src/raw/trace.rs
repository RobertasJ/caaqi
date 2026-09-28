//! The trace: the tree of nodes that computation runs in, and each node's
//! runner.
//!
//! The trace is a forest. Each node has at most one parent, any number of
//! children, and a [`Runner`], the code that runs it. Nodes are also linked
//! in trace order, parents before their children (see [`Trace`]).
//!
//! [`TraceExt`] creates nodes and hands out handles to them, like bevy's
//! `EntityRef` / `EntityWorldMut`: [`NodeRef`] for reads and [`NodeMut`] for
//! reads and writes. Every write has its own error type, which says exactly
//! why it was refused.
//!
//! The trace knows only structure and runners. Nodes are created and deleted
//! without notifying anyone, and everything else about a node, rewinds
//! included, lives in other resources keyed by [`NodeKey`]. Other modules add
//! per-node methods through extension traits on the handles, reaching their
//! resources through [`NodeRef::context`], [`NodeMut::context`] and
//! [`NodeMut::context_mut`].
//!
//! Multi-step walks over the trace are in
//! [`trace_iter`](crate::raw::trace_iter).

use std::panic::{self, AssertUnwindSafe};

use slotmap::{SlotMap, new_key_type};
use smallvec::SmallVec;

use crate::context::Context;

new_key_type! {
    /// Only valid in the `Context` that created it. Using a key with another
    /// context is unsupported and may refer to an unrelated node.
    pub struct NodeKey;
}

/// The code that runs a node, given to [`TraceExt::create_node`] and called
/// by [`NodeMut::run`] with the node's key. Closures taking
/// `(&mut Context, NodeKey)` are runners.
pub trait Runner: 'static {
    fn run(&mut self, ctx: &mut Context, node: NodeKey);
}

impl<F: FnMut(&mut Context, NodeKey) + 'static> Runner for F {
    fn run(&mut self, ctx: &mut Context, node: NodeKey) {
        self(ctx, node)
    }
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
     delete or detach them first"
)]
pub struct HasChildren {
    pub key: NodeKey,
    pub children: usize,
}

/// The node's runner is taken out because the node is running.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("node {0:?} is running")]
pub struct RunnerInUse(pub NodeKey);

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DeleteError {
    #[error(transparent)]
    RunnerInUse(#[from] RunnerInUse),
    #[error(transparent)]
    HasChildren(#[from] HasChildren),
}

/// The trace resource. It holds the shape of the trace and each node's
/// runner; other per-node data, rewinds included, lives in other resources
/// keyed by [`NodeKey`].
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
///
/// # Running
///
/// Every node has a [`Runner`], the code that runs it, called by
/// [`run`](NodeMut::run). A running node can't be run again or deleted.
/// Undoing what a run did is left to layers built on top of the trace.
#[derive(Default)]
pub struct Trace {
    nodes: SlotMap<NodeKey, Node>,
}

struct Node {
    children: SmallVec<[NodeKey; 1]>,
    parent: Option<NodeKey>,
    prev: Option<NodeKey>,
    next: Option<NodeKey>,
    /// `None` only while the runner is taken out to run.
    runner: Option<Box<dyn Runner>>,
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
    fn insert_root(&mut self, runner: Box<dyn Runner>) -> NodeKey {
        self.nodes.insert(Node {
            children: SmallVec::new(),
            parent: None,
            prev: None,
            next: None,
            runner: Some(runner),
        })
    }

    /// Takes `key`'s runner out to run it. `key` must be in the trace.
    fn take_runner(&mut self, key: NodeKey) -> Result<Box<dyn Runner>, RunnerInUse> {
        self.nodes
            .get_mut(key)
            .expect("the handle's node is in the trace")
            .runner
            .take()
            .ok_or(RunnerInUse(key))
    }

    /// Puts back the runner [`take_runner`](Self::take_runner) took out.
    fn put_runner(&mut self, key: NodeKey, runner: Box<dyn Runner>) {
        self.nodes
            .get_mut(key)
            .expect("a running node can't be deleted")
            .runner = Some(runner);
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

    /// Removes `key` from the trace. It must be a leaf that isn't running.
    fn remove_leaf(&mut self, key: NodeKey) -> Result<(), DeleteError> {
        let node = self
            .nodes
            .get(key)
            .expect("the handle's node is in the trace");

        // running check
        if node.runner.is_none() {
            return Err(RunnerInUse(key).into());
        }

        // children check
        let children = node.children.len();
        if children > 0 {
            return Err(HasChildren { key, children }.into());
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

    /// Whether this node is running: [`NodeMut::run`] has its runner taken
    /// out.
    pub fn is_running(&self) -> bool {
        self.data().runner.is_none()
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
    /// Code using the context directly can change the trace behind the
    /// handle's back, and may also run or rewind nodes, this one included. It
    /// must not delete this node: the handle's methods may panic afterwards.
    /// Extension traits must leave the handle's node in place.
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

    /// Whether this node is running: [`run`](Self::run) has its runner taken
    /// out.
    pub fn is_running(&self) -> bool {
        self.as_ref().is_running()
    }

    /// Calls this node's runner with this node's key.
    ///
    /// Fails if the node is already running. If the runner panics, it's put
    /// back before the panic is passed on.
    pub fn run(&mut self) -> Result<(), RunnerInUse> {
        let key = self.key;
        let mut runner = self.trace_mut().take_runner(key)?;
        let result = panic::catch_unwind(AssertUnwindSafe(|| runner.run(self.ctx, key)));
        self.trace_mut().put_runner(key, runner);
        if let Err(payload) = result {
            panic::resume_unwind(payload);
        }
        Ok(())
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

    /// Deletes this node, which must be a leaf that isn't running. Detaches
    /// it from its parent first if it has one.
    ///
    /// Nobody is notified: per-node data in other resources, rewinds
    /// included, is left behind.
    pub fn delete(self) -> Result<(), DeleteError> {
        let Self { ctx, key } = self;
        trace_mut(ctx).remove_leaf(key)
    }
}

pub trait TraceExt {
    /// Whether `key` is in the trace.
    fn contains_node(&self, key: NodeKey) -> bool;

    /// Read access to `key`.
    fn node(&self, key: NodeKey) -> Result<NodeRef<'_>, UnknownNode>;

    /// Write access to `key`.
    fn node_mut(&mut self, key: NodeKey) -> Result<NodeMut<'_>, UnknownNode>;

    /// Creates a node run by `runner` that is not parented and has no
    /// children. It's a root until attached with
    /// [`set_parent`](NodeMut::set_parent) or
    /// [`add_child`](NodeMut::add_child), alone in its own trace order.
    fn create_node(&mut self, runner: impl Runner) -> NodeMut<'_>;
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

    fn create_node(&mut self, runner: impl Runner) -> NodeMut<'_> {
        let key = trace_mut(self).insert_root(Box::new(runner));
        NodeMut { ctx: self, key }
    }
}

#[cfg(test)]
#[path = "trace_tests.rs"]
mod tests;
