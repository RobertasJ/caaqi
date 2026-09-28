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
    trace::{NodeKey, TraceExt, UnknownNode},
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
/// `next`) are on [`NodeRef`](crate::trace::NodeRef), from
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
mod tests {
    use googletest::prelude::*;

    use super::*;

    /// A runner for nodes whose runs don't matter.
    fn noop(_: &mut Context, _: NodeKey) {}

    fn child(ctx: &mut Context, parent: NodeKey) -> NodeKey {
        let mut node = ctx.create_node(noop);
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
        let root = ctx.create_node(noop).id();
        let a = child(&mut ctx, root);
        let a1 = child(&mut ctx, a);
        let a2 = child(&mut ctx, a);
        let b = child(&mut ctx, root);
        let b1 = child(&mut ctx, b);
        (ctx, root, [a, a1, a2, b, b1])
    }

    /// A context with a root that has children `a` and `b`, and `a` has a
    /// child.
    fn ctx_tree() -> (Context, [NodeKey; 4]) {
        let mut ctx = Context::new();
        let root = ctx.create_node(noop).id();
        let a = child(&mut ctx, root);
        let a_child = child(&mut ctx, a);
        let b = child(&mut ctx, root);
        (ctx, [root, a, a_child, b])
    }

    #[gtest]
    fn subtree_top_down_visits_parents_first() {
        let (ctx, root, [a, a1, a2, b, b1]) = tree();
        let order: Vec<_> = ctx.subtree_top_down(root).unwrap().collect();
        expect_eq!(order, [root, a, a1, a2, b, b1]);
    }

    #[gtest]
    fn skip_children_skips_the_last_returned_subtree() {
        let (ctx, root, [a, _, _, b, b1]) = tree();
        let mut walk = ctx.subtree_top_down(root).unwrap();
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
        let (ctx, root, [a, a1, a2, b, b1]) = tree();
        let mut walk = ctx.subtree_top_down(root).unwrap();
        walk.skip_children();
        expect_eq!(walk.collect::<Vec<_>>(), [root, a, a1, a2, b, b1]);
    }

    #[gtest]
    fn skip_children_at_the_start_node_ends_the_walk() {
        let (ctx, root, _) = tree();
        let mut walk = ctx.subtree_top_down(root).unwrap();
        expect_that!(walk.next(), some(eq(root)));
        walk.skip_children();
        expect_that!(walk.next(), none());
    }

    #[gtest]
    fn subtree_bottom_up_visits_children_first() {
        let (ctx, root, [a, a1, a2, b, b1]) = tree();
        let order: Vec<_> = ctx.subtree_bottom_up(root).unwrap().collect();
        expect_eq!(order, [a1, a2, a, b1, b, root]);
    }

    #[gtest]
    fn rev_variants_reverse_sibling_order() {
        let (ctx, root, [a, a1, a2, b, b1]) = tree();
        expect_eq!(
            ctx.subtree_top_down_rev(root).unwrap().collect::<Vec<_>>(),
            [root, b, b1, a, a2, a1]
        );
        expect_eq!(
            ctx.subtree_bottom_up_rev(root).unwrap().collect::<Vec<_>>(),
            [b1, b, a2, a1, a, root]
        );
    }

    #[gtest]
    fn cursor_walks_while_the_tree_changes() {
        let (mut ctx, [root, a, a_child, b]) = ctx_tree();
        let mut cursor = ctx.subtree_top_down(root).unwrap().into_cursor();
        let mut order = Vec::new();
        while let Some(key) = cursor.next(&ctx) {
            order.push(key);
            if key == a {
                ctx.node_mut(a_child).unwrap().delete().unwrap();
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
                ctx.node_mut(b).unwrap().delete().unwrap();
            }
        }
        expect_eq!(order, [root, a, a_child]);
    }

    #[gtest]
    fn top_down_rev_can_skip_children() {
        let (ctx, root, [a, a1, a2, b, _]) = tree();
        let mut walk = ctx.subtree_top_down_rev(root).unwrap();
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
    fn leaf_subtree_is_just_the_leaf() {
        let (ctx, _, [_, a1, ..]) = tree();
        expect_that!(
            ctx.subtree_top_down(a1).unwrap().collect::<Vec<_>>(),
            elements_are![eq(&a1)]
        );
        expect_that!(
            ctx.subtree_bottom_up_rev(a1).unwrap().collect::<Vec<_>>(),
            elements_are![eq(&a1)]
        );
    }

    /// Extension traits on the handles can add per-node methods from outside
    /// `trace.rs`, through its public API only.
    #[gtest]
    fn handles_can_be_extended() {
        use crate::trace::NodeRef;

        trait Label {
            fn label(&self) -> String;
        }

        impl Label for NodeRef<'_> {
            fn label(&self) -> String {
                match self.parent() {
                    Some(parent) => format!("{:?} under {parent:?}", self.id()),
                    None => format!("{:?} (root)", self.id()),
                }
            }
        }

        let (ctx, root, [a, ..]) = tree();
        expect_eq!(ctx.node(root).unwrap().label(), format!("{root:?} (root)"));
        expect_eq!(
            ctx.node(a).unwrap().label(),
            format!("{a:?} under {root:?}")
        );
    }

    #[gtest]
    fn walks_reject_unknown_nodes() {
        let (mut ctx, _, [a, a1, a2, ..]) = tree();
        for key in [a1, a2, a] {
            ctx.node_mut(key).unwrap().delete().unwrap();
        }

        expect_true!(ctx.ancestors(a1).is_err());
        expect_true!(ctx.subtree_top_down(a).is_err());
        expect_true!(ctx.subtree_top_down_rev(a).is_err());
        expect_true!(ctx.subtree_bottom_up(a).is_err());
        expect_true!(ctx.subtree_bottom_up_rev(a).is_err());
    }
}
