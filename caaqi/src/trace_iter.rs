//! Walks over the trace: up through a node's ancestors, or down through a
//! node and all its descendants.
//!
//! [`TraceIterExt`] has the walks:
//!
//! - [`ancestors`](TraceIterExt::ancestors): from a node's parent up to its
//!   root.
//! - [`subtree_top_down`](TraceIterExt::subtree_top_down): a node and its
//!   descendants, each node before its children.
//! - [`subtree_bottom_up`](TraceIterExt::subtree_bottom_up): a node and its
//!   descendants, each node after its children.
//!
//! The `_rev` variants visit siblings last to first. For a single step, such
//! as a node's parent or children, use the node handles from
//! [`TraceExt::node`].
//!
//! ```
//! use caaqi::{prelude::*, trace_iter::TraceIterExt};
//!
//! let mut ctx = Context::new();
//! let root = ctx.create_node(|_: &mut Context, _| {}).id();
//! let child = ctx.create_node(|_: &mut Context, _| {}).id();
//! ctx.node_mut(root)?.add_child(child)?;
//!
//! let top_down: Vec<_> = ctx.subtree_top_down(root)?.collect();
//! assert_eq!(top_down, [root, child]);
//! let bottom_up: Vec<_> = ctx.subtree_bottom_up(root)?.collect();
//! assert_eq!(bottom_up, [child, root]);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! A walk borrows the context, so you can't change the trace during it. To
//! change the trace while walking top-down, turn the walk into a
//! [`TopDownCursor`] with [`into_cursor`](TopDownWalk::into_cursor).

use crate::{
    context::Context,
    trace::{NodeKey, TraceExt, UnknownNode},
};

/// A walk over a node and its descendants, each node before its children.
/// Returned by [`TraceIterExt::subtree_top_down`].
///
/// Call [`skip_children`](Self::skip_children) to skip the descendants of
/// the node the walk just returned.
///
/// The walk borrows the context. To change the context while walking, turn
/// it into a [`TopDownCursor`] with [`into_cursor`](Self::into_cursor).
pub struct TopDownWalk<'a> {
    ctx: &'a Context,
    cursor: TopDownCursor,
}

impl TopDownWalk<'_> {
    /// Skips the descendants of the node the walk returned last. Does
    /// nothing before the walk has returned a node.
    pub fn skip_children(&mut self) {
        self.cursor.skip_children();
    }

    /// Turns this walk into a cursor that continues where it left off, but
    /// doesn't borrow the context.
    pub fn into_cursor(self) -> TopDownCursor {
        self.cursor
    }
}

impl Iterator for TopDownWalk<'_> {
    type Item = NodeKey;

    fn next(&mut self) -> Option<NodeKey> {
        self.cursor.next(self.ctx)
    }
}

/// A top-down walk that doesn't borrow the context, so you can change the
/// trace between steps. Get one from [`TopDownWalk::into_cursor`], and pass
/// the context to each call of [`next`](Self::next).
///
/// Changes you make between steps are taken into account: if you add
/// children to the node the cursor just returned, they're visited, and nodes
/// you delete are skipped.
///
/// ```
/// use caaqi::{prelude::*, trace_iter::TraceIterExt};
///
/// let mut ctx = Context::new();
/// let root = ctx.create_node(|_: &mut Context, _| {}).id();
///
/// let mut cursor = ctx.subtree_top_down(root)?.into_cursor();
/// while let Some(node) = cursor.next(&ctx) {
///     ctx.node_mut(node)?.run()?;
/// }
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub struct TopDownCursor {
    pending: Vec<NodeKey>,
    /// The node returned last, whose children haven't been queued yet.
    /// They're queued on the next `next` call, so `skip_children` can drop
    /// them first.
    last: Option<NodeKey>,
    siblings_rev: bool,
}

impl TopDownCursor {
    /// Skips the descendants of the node the cursor returned last. Does
    /// nothing before the cursor has returned a node.
    pub fn skip_children(&mut self) {
        self.last = None;
    }

    /// Returns the next node of the walk, or `None` once it's done.
    pub fn next(&mut self, ctx: &Context) -> Option<NodeKey> {
        // A node removed since it was returned has no children left to walk.
        if let Some(Ok(node)) = self.last.take().map(|last| ctx.node(last)) {
            let children = node.children().iter().copied();
            if self.siblings_rev {
                self.pending.extend(children);
            } else {
                self.pending.extend(children.rev());
            }
        }
        // Nodes removed since they were queued are skipped, along with their
        // descendants, which were removed with them.
        let key = std::iter::from_fn(|| self.pending.pop()).find(|&key| ctx.contains_node(key))?;
        self.last = Some(key);
        Some(key)
    }
}

/// Iterates `key` and its descendants, each before its own descendants,
/// siblings last to first when `siblings_rev`.
fn parents_first(
    ctx: &Context,
    key: NodeKey,
    siblings_rev: bool,
) -> Result<TopDownWalk<'_>, UnknownNode> {
    ctx.node(key)?;
    Ok(TopDownWalk {
        ctx,
        cursor: TopDownCursor {
            pending: vec![key],
            last: None,
            siblings_rev,
        },
    })
}

/// Iterates `key` and its descendants, each after its own descendants,
/// siblings last to first when `siblings_rev`.
fn children_first(
    ctx: &Context,
    key: NodeKey,
    siblings_rev: bool,
) -> Result<impl Iterator<Item = NodeKey> + '_, UnknownNode> {
    ctx.node(key)?;
    // `true` once a node's children have been pushed above it.
    let mut pending = vec![(key, false)];
    Ok(std::iter::from_fn(move || {
        loop {
            let (key, expanded) = pending.pop()?;
            if expanded {
                return Some(key);
            }
            pending.push((key, true));
            // The walk borrows the context, so nothing is removed during it.
            let children = ctx
                .node(key)
                .expect("descendants of a node in the trace are in the trace")
                .children()
                .iter()
                .map(|&child| (child, false));
            if siblings_rev {
                pending.extend(children);
            } else {
                pending.extend(children.rev());
            }
        }
    }))
}

/// Walks over the trace. See the [module docs](self) for an example.
///
/// Every walk fails with [`UnknownNode`] if `key` isn't in the trace.
pub trait TraceIterExt {
    /// Walks up from the parent of `key` to its root. Returns nothing if
    /// `key` is a root.
    fn ancestors(&self, key: NodeKey) -> Result<impl Iterator<Item = NodeKey> + '_, UnknownNode>;

    /// Walks over `key` and its descendants, each node before its children,
    /// so `key` comes first. Siblings are visited first to last. This is
    /// [trace order](crate::trace#trace-order).
    ///
    /// Call [`skip_children`](TopDownWalk::skip_children) to skip the
    /// descendants of the node just returned.
    fn subtree_top_down(&self, key: NodeKey) -> Result<TopDownWalk<'_>, UnknownNode>;

    /// Like [`subtree_top_down`](Self::subtree_top_down), but visits siblings
    /// last to first.
    fn subtree_top_down_rev(&self, key: NodeKey) -> Result<TopDownWalk<'_>, UnknownNode>;

    /// Walks over `key` and its descendants, each node after its children,
    /// so `key` comes last. Siblings are visited first to last.
    fn subtree_bottom_up(
        &self,
        key: NodeKey,
    ) -> Result<impl Iterator<Item = NodeKey> + '_, UnknownNode>;

    /// Like [`subtree_bottom_up`](Self::subtree_bottom_up), but visits
    /// siblings last to first.
    fn subtree_bottom_up_rev(
        &self,
        key: NodeKey,
    ) -> Result<impl Iterator<Item = NodeKey> + '_, UnknownNode>;
}

impl TraceIterExt for Context {
    fn ancestors(&self, key: NodeKey) -> Result<impl Iterator<Item = NodeKey> + '_, UnknownNode> {
        let parent = |key| {
            self.node(key)
                .expect("parents of nodes in the trace are in the trace")
                .parent()
        };
        Ok(std::iter::successors(
            self.node(key)?.parent(),
            move |&key| parent(key),
        ))
    }

    fn subtree_top_down(&self, key: NodeKey) -> Result<TopDownWalk<'_>, UnknownNode> {
        parents_first(self, key, false)
    }

    fn subtree_top_down_rev(&self, key: NodeKey) -> Result<TopDownWalk<'_>, UnknownNode> {
        parents_first(self, key, true)
    }

    fn subtree_bottom_up(
        &self,
        key: NodeKey,
    ) -> Result<impl Iterator<Item = NodeKey> + '_, UnknownNode> {
        children_first(self, key, false)
    }

    fn subtree_bottom_up_rev(
        &self,
        key: NodeKey,
    ) -> Result<impl Iterator<Item = NodeKey> + '_, UnknownNode> {
        children_first(self, key, true)
    }
}

#[cfg(test)]
#[path = "trace_iter_tests.rs"]
mod tests;
