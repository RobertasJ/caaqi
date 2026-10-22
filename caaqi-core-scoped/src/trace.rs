//! The trace, like [`caaqi_core::trace`]. Nodes are [`TraceNode`]s, a newtype over
//! caaqi-core's [`NodeKey`] with the node's operations as methods, in place of
//! caaqi-core's node handles.

use std::cmp::Ordering;

pub use caaqi_core::trace::{
    AddChildError, AlreadyParented, CmpNodesError, DeleteError, DifferentTrees, HasChildren,
    NodeKey, RunnerInUse, SelfParent, SetParentError, UnknownChild, UnknownNode, UnknownParent,
    WouldCycle,
};
use caaqi_core::{context::Context, trace::TraceExt};

use crate::context::{scope, with_ctx};

/// A node of the trace: a newtype over caaqi-core's [`NodeKey`] that adds the
/// node's operations as methods, in place of caaqi-core's
/// [`NodeRef`](caaqi_core::trace::NodeRef) / [`NodeMut`](caaqi_core::trace::NodeMut).
/// Convert with `From` either way.
///
/// A `Node` doesn't borrow the context, so nothing keeps its node alive: the
/// methods panic if the node was deleted. Check with
/// [`exists`](Self::exists) when that may have happened.
///
/// It's `PartialOrd` by trace order, unlike `NodeKey`, whose order compares
/// slots: `a < b` if `a` comes first in the trace, and nodes in different
/// trees are unordered (`None`). It isn't `Ord`. The comparison panics if
/// either node was deleted; [`trace_cmp`](Self::trace_cmp) returns the error
/// for an unknown `other` instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TraceNode(NodeKey);

impl From<NodeKey> for TraceNode {
    fn from(key: NodeKey) -> Self {
        TraceNode(key)
    }
}

impl From<TraceNode> for NodeKey {
    fn from(node: TraceNode) -> Self {
        node.0
    }
}

impl PartialOrd for TraceNode {
    fn partial_cmp(&self, other: &TraceNode) -> Option<Ordering> {
        match self.trace_cmp(*other) {
            Ok(ordering) => Some(ordering),
            Err(CmpNodesError::DifferentTrees(_)) => None,
            Err(CmpNodesError::UnknownNode(UnknownNode(key))) => deleted(key),
        }
    }
}

/// The code that runs a node, given the node. Pass it to [`create_node`]. If
/// it doesn't need the node, write a [`Runner`](crate::current::Runner) and
/// wrap it in [`WithCurrent`](crate::current::WithCurrent).
///
/// Any `FnMut(TraceNode)` closure is a runner. Annotate its parameter type
/// (`|node: TraceNode| ...`).
pub trait RunnerWithNode: Sized + 'static {
    /// caaqi-core's runner for this one, which runs it with the context in scope.
    fn into_runner(self) -> impl caaqi_core::trace::RunnerWithNode;
}

impl<F: FnMut(TraceNode) + 'static> RunnerWithNode for F {
    fn into_runner(mut self) -> impl caaqi_core::trace::RunnerWithNode {
        move |ctx: &mut Context, node| scope(ctx, || self(TraceNode(node)))
    }
}

fn deleted(key: NodeKey) -> ! {
    panic!("node {key:?} was deleted")
}

/// Calls `f` with caaqi-core's handle for reading the node. Panics if it was
/// deleted.
fn read<R>(key: NodeKey, f: impl FnOnce(caaqi_core::trace::NodeRef<'_>) -> R) -> R {
    with_ctx(|ctx| f(ctx.node(key).unwrap_or_else(|_| deleted(key))))
}

/// Like [`read`], with caaqi-core's handle for changing the node.
fn write<R>(key: NodeKey, f: impl FnOnce(caaqi_core::trace::NodeMut<'_>) -> R) -> R {
    with_ctx(|ctx| f(ctx.node_mut(key).unwrap_or_else(|_| deleted(key))))
}

impl TraceNode {
    /// Whether the node is in the trace: [`TraceExt::contains_node`]. When
    /// it isn't, because it was deleted, the other methods panic.
    pub fn exists(self) -> bool {
        with_ctx(|ctx| ctx.contains_node(self.0))
    }

    /// The node's parent, or `None` if it's a root.
    pub fn parent(self) -> Option<TraceNode> {
        read(self.0, |node| node.parent()).map(TraceNode)
    }

    /// The node's children, first to last.
    pub fn children(self) -> Vec<TraceNode> {
        read(self.0, |node| {
            node.children().iter().copied().map(TraceNode).collect()
        })
    }

    /// The node before this one in trace order, or `None` if this is a root.
    pub fn prev(self) -> Option<TraceNode> {
        read(self.0, |node| node.prev()).map(TraceNode)
    }

    /// The node after this one in trace order, or `None` if this is the last
    /// node of its tree.
    pub fn next(self) -> Option<TraceNode> {
        read(self.0, |node| node.next()).map(TraceNode)
    }

    /// Whether the node is running right now.
    pub fn is_running(self) -> bool {
        read(self.0, |node| node.is_running())
    }

    /// Compares this node with `other` by trace order: [`TraceExt::cmp_nodes`].
    /// `Less` when this node comes before `other`. A node comes before its
    /// descendants, and an earlier child's subtree before a later one's.
    ///
    /// Nodes in different trees have no order between them
    /// ([`DifferentTrees`]).
    ///
    /// Fails if `other` isn't in the trace. Panics if this node isn't.
    pub fn trace_cmp(self, other: TraceNode) -> Result<Ordering, CmpNodesError> {
        match with_ctx(|ctx| ctx.cmp_nodes(self.0, other.0)) {
            Err(CmpNodesError::UnknownNode(UnknownNode(key))) if key == self.0 => deleted(key),
            result => result,
        }
    }

    /// Runs the node. See [`caaqi_core::trace::NodeMut::run`].
    pub fn run(self) -> Result<(), RunnerInUse> {
        write(self.0, |mut node| node.run())
    }

    /// Attaches this node as the last child of `parent`, and returns it. See
    /// [`caaqi_core::trace::NodeMut::set_parent`].
    pub fn set_parent(self, parent: TraceNode) -> Result<TraceNode, SetParentError> {
        write(self.0, |mut node| node.set_parent(parent.0).map(|_| ()))?;
        Ok(self)
    }

    /// Attaches `child` as this node's last child, and returns this node. See
    /// [`caaqi_core::trace::NodeMut::add_child`].
    pub fn add_child(self, child: TraceNode) -> Result<TraceNode, AddChildError> {
        write(self.0, |mut node| node.add_child(child.0).map(|_| ()))?;
        Ok(self)
    }

    /// Detaches this node from its parent, making it a root. Returns `false`
    /// if it had no parent. See [`caaqi_core::trace::NodeMut::detach`].
    pub fn detach(self) -> bool {
        write(self.0, |mut node| node.detach())
    }

    /// Deletes this node. See [`caaqi_core::trace::NodeMut::delete`].
    pub fn delete(self) -> Result<(), DeleteError> {
        write(self.0, |node| node.delete())
    }
}

/// [`TraceExt::create_node`]: creates a node that runs `runner`, and returns
/// it. When it runs, the context it runs in is in scope.
pub fn create_node(runner: impl RunnerWithNode) -> TraceNode {
    with_ctx(|ctx| TraceNode(ctx.create_node(runner.into_runner()).id()))
}

#[cfg(test)]
#[path = "trace_tests.rs"]
mod tests;
