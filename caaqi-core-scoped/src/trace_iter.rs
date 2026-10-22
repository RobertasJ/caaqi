//! Walks over the trace, like [`caaqi_core::trace_iter`].
//!
//! [`subtree_top_down`] stays lazy, and the trace can change between its
//! steps; it doesn't borrow the context, so it also does what
//! [`caaqi_core::trace_iter::TopDownCursor`] is for. The other walks collect
//! their nodes before returning.

use caaqi_core::trace_iter::{TopDownCursor, TraceIterExt};

use crate::{
    context::with_ctx,
    trace::{TraceNode, UnknownNode},
};

/// A walk over a node and its descendants, each node before its children.
/// Returned by [`subtree_top_down`].
///
/// It doesn't borrow the context, so the trace can change between steps:
/// children added to the node just returned are visited, and deleted nodes
/// are skipped. Call [`skip_children`](Self::skip_children) to skip the
/// descendants of the node just returned.
pub struct TopDownWalk {
    cursor: TopDownCursor,
}

impl TopDownWalk {
    /// Skips the descendants of the node the walk returned last. Does
    /// nothing before the walk has returned a node.
    pub fn skip_children(&mut self) {
        self.cursor.skip_children();
    }
}

impl Iterator for TopDownWalk {
    type Item = TraceNode;

    fn next(&mut self) -> Option<TraceNode> {
        with_ctx(|ctx| self.cursor.next(ctx)).map(TraceNode::from)
    }
}

/// [`TraceIterExt::ancestors`].
pub fn ancestors(node: TraceNode) -> Result<impl Iterator<Item = TraceNode>, UnknownNode> {
    with_ctx(|ctx| {
        Ok(ctx
            .ancestors(node.into())?
            .map(TraceNode::from)
            .collect::<Vec<_>>()
            .into_iter())
    })
}

/// [`TraceIterExt::subtree_top_down`].
pub fn subtree_top_down(node: TraceNode) -> Result<TopDownWalk, UnknownNode> {
    with_ctx(|ctx| {
        Ok(TopDownWalk {
            cursor: ctx.subtree_top_down(node.into())?.into_cursor(),
        })
    })
}

/// [`TraceIterExt::subtree_top_down_rev`].
pub fn subtree_top_down_rev(node: TraceNode) -> Result<TopDownWalk, UnknownNode> {
    with_ctx(|ctx| {
        Ok(TopDownWalk {
            cursor: ctx.subtree_top_down_rev(node.into())?.into_cursor(),
        })
    })
}

/// [`TraceIterExt::subtree_bottom_up`].
pub fn subtree_bottom_up(node: TraceNode) -> Result<impl Iterator<Item = TraceNode>, UnknownNode> {
    with_ctx(|ctx| {
        Ok(ctx
            .subtree_bottom_up(node.into())?
            .map(TraceNode::from)
            .collect::<Vec<_>>()
            .into_iter())
    })
}

/// [`TraceIterExt::subtree_bottom_up_rev`].
pub fn subtree_bottom_up_rev(
    node: TraceNode,
) -> Result<impl Iterator<Item = TraceNode>, UnknownNode> {
    with_ctx(|ctx| {
        Ok(ctx
            .subtree_bottom_up_rev(node.into())?
            .map(TraceNode::from)
            .collect::<Vec<_>>()
            .into_iter())
    })
}

#[cfg(test)]
#[path = "trace_iter_tests.rs"]
mod tests;
