use googletest::prelude::*;

use super::*;

/// A runner for nodes whose runs don't matter.
fn noop(_: &mut Context, _: NodeKey) {}

/// What runners did, in the order they did it.
#[derive(Default)]
struct Log(Vec<String>);

fn log(ctx: &mut Context, entry: impl ToString) {
    ctx.get_or_insert_with(Log::default)
        .0
        .push(entry.to_string());
}

fn logged(ctx: &Context) -> Vec<String> {
    ctx.get::<Log>().map_or_else(Vec::new, |log| log.0.clone())
}

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
    let root2 = ctx.create_node(noop).id();
    let c = child(&mut ctx, root2);
    let a3 = child(&mut ctx, a);
    expect_eq!(tree_order(&ctx, root), [root, a, a1, a2, a3, b, b1]);
    expect_eq!(tree_order(&ctx, root2), [root2, c]);
    expect_that!(ctx.node(b1).unwrap().next(), none());
}

#[gtest]
fn deletion_keeps_the_trace_order_linked() {
    let (mut ctx, root, [a, a1, a2, b, b1]) = tree();
    let root2 = ctx.create_node(noop).id();

    ctx.node_mut(a1).unwrap().delete().unwrap();
    expect_eq!(tree_order(&ctx, root), [root, a, a2, b, b1]);
    expect_eq!(ctx.node(a).unwrap().children(), &[a2]);

    ctx.node_mut(b1).unwrap().delete().unwrap();
    expect_eq!(tree_order(&ctx, root), [root, a, a2, b]);

    ctx.node_mut(a2).unwrap().delete().unwrap();
    ctx.node_mut(a).unwrap().delete().unwrap();
    expect_eq!(tree_order(&ctx, root), [root, b]);

    ctx.node_mut(b).unwrap().delete().unwrap();
    ctx.node_mut(root).unwrap().delete().unwrap();
    expect_false!(ctx.contains_node(root));
    expect_eq!(tree_order(&ctx, root2), [root2]);
}

#[gtest]
fn set_parent_and_add_child_attach_as_last_child() {
    let (mut ctx, root, [a, a1, a2, b, b1]) = tree();
    let c = ctx.create_node(noop).id();
    let d = ctx.create_node(noop).id();

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
    let deleted = ctx.create_node(noop).id();
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
    let (mut ctx, _, [a, a1, a2, ..]) = tree();
    for key in [a1, a2, a] {
        ctx.node_mut(key).unwrap().delete().unwrap();
    }

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
        some(eq(DeleteError::HasChildren(HasChildren {
            key: a,
            children: 2
        })))
    );
    expect_eq!(ctx.node(a).unwrap().children(), &[a1, a2]);
}

#[gtest]
fn moving_subtrees_keeps_the_trace_order_linked() {
    let (mut ctx, root, [a, a1, a2, b, b1]) = tree();
    let root2 = ctx.create_node(noop).id();

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

#[gtest]
fn a_runner_gets_its_node_and_cant_run_it_again() {
    struct RanAs(NodeKey);

    let mut ctx = Context::new();
    let key = ctx
        .create_node(|ctx: &mut Context, key: NodeKey| {
            expect_true!(ctx.node(key).unwrap().is_running());
            expect_that!(
                ctx.node_mut(key).unwrap().run().err(),
                some(eq(RunnerInUse(key)))
            );
            ctx.insert(RanAs(key));
        })
        .id();

    expect_false!(ctx.node(key).unwrap().is_running());
    expect_eq!(ctx.node_mut(key).unwrap().run(), Ok(()));
    expect_that!(ctx.get::<RanAs>().map(|ran| ran.0), some(eq(key)));
    expect_false!(ctx.node(key).unwrap().is_running());
}

#[gtest]
fn delete_refuses_running_nodes() {
    let mut ctx = Context::new();
    let running = ctx
        .create_node(|ctx: &mut Context, key: NodeKey| {
            expect_that!(
                ctx.node_mut(key).unwrap().delete().err(),
                some(eq(DeleteError::RunnerInUse(RunnerInUse(key))))
            );
        })
        .id();
    ctx.node_mut(running).unwrap().run().unwrap();
    expect_true!(ctx.contains_node(running));
    expect_eq!(ctx.node_mut(running).unwrap().delete(), Ok(()));
}

#[gtest]
fn a_panicking_runner_is_put_back() {
    let mut ctx = Context::new();
    let key = ctx
        .create_node(|ctx: &mut Context, _: NodeKey| {
            log(ctx, "ran");
            if logged(ctx).len() == 1 {
                panic!("the first run panics");
            }
        })
        .id();

    let result = panic::catch_unwind(AssertUnwindSafe(|| ctx.node_mut(key).unwrap().run()));
    expect_true!(result.is_err());
    expect_false!(ctx.node(key).unwrap().is_running());
    expect_eq!(ctx.node_mut(key).unwrap().run(), Ok(()));
    expect_eq!(logged(&ctx), ["ran", "ran"]);
}
