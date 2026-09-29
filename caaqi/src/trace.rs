//! The trace: the tree of nodes your computation runs in.
//!
//! Every node has a runner, the code that runs it, and can have a parent and
//! any number of children. A node without a parent is a root, so the trace
//! can hold several separate trees.
//!
//! Use [`TraceExt`] to create nodes and to get handles to them: [`NodeRef`]
//! to read a node, [`NodeMut`] to also change or run it.
//!
//! ```
//! use caaqi::prelude::*;
//!
//! let mut ctx = Context::new();
//! let parent = ctx.create_node(|_: &mut Context, _| {}).id();
//! let child = ctx
//!     .create_node(|_: &mut Context, node| println!("running {node:?}"))
//!     .id();
//!
//! ctx.node_mut(parent)?.add_child(child)?;
//! assert_eq!(ctx.node(child)?.parent(), Some(parent));
//!
//! ctx.node_mut(child)?.run()?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! Every method that can fail returns an error type that says exactly why.
//!
//! The trace only knows about the tree and the runners. It doesn't tell
//! anyone when a node is created or deleted, and anything else you want to
//! keep per node belongs in your own state, keyed by [`NodeKey`].
//!
//! # Trace order
//!
//! Besides its parent and children, each node knows the node before and after
//! it in trace order ([`prev`](NodeRef::prev) and [`next`](NodeRef::next)).
//! Trace order is the order you'd read the tree in: each node comes before
//! its children, and siblings come first to last. For example:
//!
//! ```text
//! a           order: a, b, c, d
//! ├── b
//! │   └── c   c's next is d, and d's prev is c:
//! └── d       b's whole subtree comes before d.
//! ```
//!
//! Each tree has its own order: a root has no `prev`, the last node of its
//! tree has no `next`, and the orders of two separate trees never connect.
//!
//! # Adding your own node methods
//!
//! You can add methods to the node handles with your own extension traits,
//! the way bevy extends `EntityWorldMut`. Reach the rest of the state through
//! [`NodeRef::context`], [`NodeMut::context`] and [`NodeMut::context_mut`].
//!
//! For walks over many nodes, such as all of a node's descendants, see
//! [`trace_iter`](crate::trace_iter).

use std::panic::{self, AssertUnwindSafe};

use slotmap::{SlotMap, new_key_type};
use smallvec::SmallVec;

use crate::context::Context;

new_key_type! {
    /// Identifies a node in the trace. Get one from
    /// [`TraceExt::create_node`], through the handle's `id`.
    pub struct NodeKey;
}

/// The code that runs a node. Pass it to [`TraceExt::create_node`];
/// [`NodeMut::run`] calls it with the node's key.
///
/// Any `FnMut(&mut Context, NodeKey)` closure is a runner. Annotate the
/// closure's parameter types (`|ctx: &mut Context, node| ...`), because Rust
/// can't infer them here.
///
/// If your code doesn't need to be given the node's key, use a
/// [`current::Runner`](crate::current::Runner) instead.
pub trait RunnerWithNode: 'static {
    fn run(&mut self, ctx: &mut Context, node: NodeKey);
}

impl<F: FnMut(&mut Context, NodeKey) + 'static> RunnerWithNode for F {
    fn run(&mut self, ctx: &mut Context, node: NodeKey) {
        self(ctx, node)
    }
}

/// Returned when a [`NodeKey`] doesn't refer to a node in the trace, for
/// example because the node was deleted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("node {0:?} isn't in the trace")]
pub struct UnknownNode(pub NodeKey);

/// Returned by [`NodeMut::set_parent`] when the parent you passed isn't in
/// the trace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("parent {:?} isn't in the trace", .0.0)]
pub struct UnknownParent(#[source] pub UnknownNode);

/// Returned by [`NodeMut::add_child`] when the child you passed isn't in the
/// trace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("child {:?} isn't in the trace", .0.0)]
pub struct UnknownChild(#[source] pub UnknownNode);

/// Returned when attaching a node that already has a parent. To move a node
/// to another parent, [`detach`](NodeMut::detach) it first.
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

/// Returned when attaching a node to itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("node {0:?} can't be its own parent")]
pub struct SelfParent(pub NodeKey);

/// Returned when attaching a node under one of its own descendants, which
/// would make the tree loop. `depth` is how many levels below `child` the
/// requested parent is.
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

/// Returned by [`NodeMut::delete`] for a node that still has children. Only
/// nodes without children can be deleted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error(
    "node {key:?} has {children} child(ren); \
     delete or detach them first"
)]
pub struct HasChildren {
    pub key: NodeKey,
    pub children: usize,
}

/// Returned when running or deleting a node that is already running.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("node {0:?} is running")]
pub struct RunnerInUse(pub NodeKey);

/// Why [`NodeMut::set_parent`] failed.
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

/// Why [`NodeMut::add_child`] failed.
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

/// Why [`NodeMut::delete`] failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DeleteError {
    #[error(transparent)]
    RunnerInUse(#[from] RunnerInUse),
    #[error(transparent)]
    HasChildren(#[from] HasChildren),
}

/// The trace resource: every node's links and runner. Everything public goes
/// through [`TraceExt`] and the node handles.
///
/// Each node before its descendants, siblings first to last, is linked by
/// `prev` / `next`. So a node's `prev` is its previous sibling's last
/// descendant, or its parent when it's a first child.
#[derive(Default)]
struct Trace {
    nodes: SlotMap<NodeKey, Node>,
}

struct Node {
    children: SmallVec<[NodeKey; 1]>,
    parent: Option<NodeKey>,
    prev: Option<NodeKey>,
    next: Option<NodeKey>,
    /// `None` only while the runner is taken out to run.
    runner: Option<Box<dyn RunnerWithNode>>,
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
    fn insert_root(&mut self, runner: Box<dyn RunnerWithNode>) -> NodeKey {
        self.nodes.insert(Node {
            children: SmallVec::new(),
            parent: None,
            prev: None,
            next: None,
            runner: Some(runner),
        })
    }

    /// Takes `key`'s runner out to run it. `key` must be in the trace.
    fn take_runner(&mut self, key: NodeKey) -> Result<Box<dyn RunnerWithNode>, RunnerInUse> {
        self.nodes
            .get_mut(key)
            .expect("the handle's node is in the trace")
            .runner
            .take()
            .ok_or(RunnerInUse(key))
    }

    /// Puts back the runner [`take_runner`](Self::take_runner) took out.
    fn put_runner(&mut self, key: NodeKey, runner: Box<dyn RunnerWithNode>) {
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

/// A handle for reading a node, returned by [`TraceExt::node`].
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

    /// The node's key.
    pub fn id(&self) -> NodeKey {
        self.key
    }

    /// The context this handle reads from. Useful when writing your own
    /// extension traits for node handles.
    pub fn context(&self) -> &'a Context {
        self.ctx
    }

    /// The node's parent, or `None` if it's a root.
    pub fn parent(&self) -> Option<NodeKey> {
        self.data().parent
    }

    /// The node's children, first to last.
    pub fn children(&self) -> &'a [NodeKey] {
        &self.data().children
    }

    /// The node before this one in [trace order](self#trace-order), or
    /// `None` if this is a root.
    pub fn prev(&self) -> Option<NodeKey> {
        self.data().prev
    }

    /// The node after this one in [trace order](self#trace-order), or `None`
    /// if this is the last node of its tree.
    pub fn next(&self) -> Option<NodeKey> {
        self.data().next
    }

    /// Whether the node is running right now, which is the case while its
    /// runner, or code its runner called, is executing.
    pub fn is_running(&self) -> bool {
        self.data().runner.is_none()
    }
}

/// A handle for reading, changing and running a node, returned by
/// [`TraceExt::node_mut`] and [`TraceExt::create_node`].
///
/// When you attach or detach a node, its children and their descendants move
/// with it. To move a node that already has a parent, first
/// [`detach`](Self::detach) it, then attach it elsewhere.
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

    /// The node's key.
    pub fn id(&self) -> NodeKey {
        self.key
    }

    /// The context this handle belongs to. Useful when writing your own
    /// extension traits for node handles.
    pub fn context(&self) -> &Context {
        self.ctx
    }

    /// The context this handle belongs to, mutably. Useful when writing your
    /// own extension traits for node handles.
    ///
    /// # Don't delete this node
    ///
    /// You may use the context to change the trace, and to run or rewind
    /// nodes, this one included. But don't delete this handle's node: the
    /// handle's methods may panic afterwards.
    pub fn context_mut(&mut self) -> &mut Context {
        self.ctx
    }

    /// A read-only handle to the same node.
    pub fn as_ref(&self) -> NodeRef<'_> {
        NodeRef {
            ctx: self.ctx,
            key: self.key,
        }
    }

    /// The node's parent, or `None` if it's a root.
    pub fn parent(&self) -> Option<NodeKey> {
        self.as_ref().parent()
    }

    /// The node's children, first to last.
    pub fn children(&self) -> &[NodeKey] {
        self.as_ref().children()
    }

    /// The node before this one in [trace order](self#trace-order), or
    /// `None` if this is a root.
    pub fn prev(&self) -> Option<NodeKey> {
        self.as_ref().prev()
    }

    /// The node after this one in [trace order](self#trace-order), or `None`
    /// if this is the last node of its tree.
    pub fn next(&self) -> Option<NodeKey> {
        self.as_ref().next()
    }

    /// Whether the node is running right now, which is the case while its
    /// runner, or code its runner called, is executing.
    pub fn is_running(&self) -> bool {
        self.as_ref().is_running()
    }

    /// Runs the node: calls its runner with the node's key.
    ///
    /// Returns [`RunnerInUse`] if the node is already running, because a
    /// node can't run inside its own run. If the runner panics, the panic is
    /// passed on, and the node can be run again afterwards.
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

    /// Attaches this node, with all its descendants, as the last child of
    /// `parent`. Does the same as calling [`add_child`](Self::add_child) on
    /// `parent`.
    ///
    /// Fails if `parent` isn't in the trace, if this node already has a
    /// parent, or if `parent` is this node or one of its descendants.
    pub fn set_parent(&mut self, parent: NodeKey) -> Result<&mut Self, SetParentError> {
        let key = self.key;
        self.trace_mut().attach(parent, key)?;
        Ok(self)
    }

    /// Attaches `child`, with all its descendants, as this node's last child.
    /// Does the same as calling [`set_parent`](Self::set_parent) on `child`.
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

    /// Detaches this node, with all its descendants, from its parent, making
    /// it a root. Returns `false` if it had no parent to begin with.
    pub fn detach(&mut self) -> bool {
        let key = self.key;
        self.trace_mut().detach(key)
    }

    /// Deletes this node, detaching it from its parent first.
    ///
    /// Fails if the node has children (delete or detach them first) or is
    /// running.
    ///
    /// Nothing else is told about the deletion, so anything kept for this
    /// node elsewhere stays behind. In particular,
    /// [rewind](crate::rewind::RewindExt::rewind) the node before deleting
    /// it, or its rewinds never run.
    pub fn delete(self) -> Result<(), DeleteError> {
        let Self { ctx, key } = self;
        trace_mut(ctx).remove_leaf(key)
    }
}

/// Creating nodes and getting handles to them. See the [module docs](self)
/// for an example.
pub trait TraceExt {
    /// Whether `key` refers to a node in the trace.
    fn contains_node(&self, key: NodeKey) -> bool;

    /// Returns a handle for reading the node, or [`UnknownNode`] if it isn't
    /// in the trace.
    fn node(&self, key: NodeKey) -> Result<NodeRef<'_>, UnknownNode>;

    /// Returns a handle for changing and running the node, or
    /// [`UnknownNode`] if it isn't in the trace.
    fn node_mut(&mut self, key: NodeKey) -> Result<NodeMut<'_>, UnknownNode>;

    /// Creates a node that runs `runner`, and returns a handle to it. Call
    /// [`id`](NodeMut::id) on the handle to get its key.
    ///
    /// The new node has no parent and no children. Attach it with
    /// [`set_parent`](NodeMut::set_parent) or
    /// [`add_child`](NodeMut::add_child). It doesn't run until you call
    /// [`run`](NodeMut::run).
    fn create_node(&mut self, runner: impl RunnerWithNode) -> NodeMut<'_>;
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

    fn create_node(&mut self, runner: impl RunnerWithNode) -> NodeMut<'_> {
        let key = trace_mut(self).insert_root(Box::new(runner));
        NodeMut { ctx: self, key }
    }
}

#[cfg(test)]
#[path = "trace_tests.rs"]
mod tests;
