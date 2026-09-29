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
