//! Walks over the trace, written only against the public [`TraceExt`] API.
//!
//! [`TraceIterExt`] has the multi-step walks: up to the root
//! ([`ancestors`](TraceIterExt::ancestors)), and down through a subtree,
//! parents first ([`subtree_top_down`](TraceIterExt::subtree_top_down)) or
//! children first ([`subtree_bottom_up`](TraceIterExt::subtree_bottom_up)),
//! with `_rev` variants that take siblings last to first. Single steps are on
//! the node handles.
//!
//! Walks borrow the context. To change the trace during a top-down walk,
//! turn it into a [`TopDownCursor`], which takes the context on each step
//! instead.

use crate::{
    context::Context,
    raw::trace::{NodeKey, TraceExt, UnknownNode},
};

/// A walk over a node and its descendants, each before its own descendants.
/// [`skip_children`](Self::skip_children) keeps the walk out of the children
/// of the node it just returned.
///
/// It borrows the context; use [`into_cursor`](Self::into_cursor) to walk
/// while changing it.
pub struct TopDownWalk<'a> {
    ctx: &'a Context,
    cursor: TopDownCursor,
}

impl TopDownWalk<'_> {
    /// Skips the descendants of the node `next` returned last. Does nothing
    /// before the first `next`, or when called twice in a row.
    pub fn skip_children(&mut self) {
        self.cursor.skip_children();
    }

    /// Continues this walk without borrowing the context.
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

/// A [`TopDownWalk`] that doesn't borrow the context: each
/// [`next`](Self::next) takes it instead, so the context can be changed
/// between calls.
///
/// A node's children are read on the `next` call after it's returned, so
/// children it gains or loses in between are taken into account, unless
/// [`skip_children`](Self::skip_children) is called. Nodes removed in between
/// are skipped.
pub struct TopDownCursor {
    pending: Vec<NodeKey>,
    /// The node returned last, whose children haven't been queued yet.
    /// They're queued on the next `next` call, so `skip_children` can drop
    /// them first.
    last: Option<NodeKey>,
    siblings_rev: bool,
}

impl TopDownCursor {
    /// Skips the descendants of the node `next` returned last. Does nothing
    /// before the first `next`, or when called twice in a row.
    pub fn skip_children(&mut self) {
        self.last = None;
    }

    /// The next node of the walk, or `None` once it's done.
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

/// Walks over the trace. The single steps (`parent`, `children`, `prev`,
/// `next`) are on [`NodeRef`](crate::raw::trace::NodeRef), from
/// [`TraceExt::node`].
pub trait TraceIterExt {
    /// Walks up from the parent of `key` to its root.
    fn ancestors(&self, key: NodeKey) -> Result<impl Iterator<Item = NodeKey> + '_, UnknownNode>;

    /// Iterates `key` and its descendants, each before its own descendants,
    /// so `key` comes first. Call
    /// [`skip_children`](TopDownWalk::skip_children) to skip the descendants
    /// of the node just returned.
    fn subtree_top_down(&self, key: NodeKey) -> Result<TopDownWalk<'_>, UnknownNode>;

    /// [`subtree_top_down`](Self::subtree_top_down) with siblings last to
    /// first.
    fn subtree_top_down_rev(&self, key: NodeKey) -> Result<TopDownWalk<'_>, UnknownNode>;

    /// Iterates `key` and its descendants, each after its own descendants, so
    /// `key` comes last.
    fn subtree_bottom_up(
        &self,
        key: NodeKey,
    ) -> Result<impl Iterator<Item = NodeKey> + '_, UnknownNode>;

    /// [`subtree_bottom_up`](Self::subtree_bottom_up) with siblings last to
    /// first.
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
