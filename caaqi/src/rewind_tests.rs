use std::panic::{self, AssertUnwindSafe};

use googletest::prelude::*;

use super::*;

/// A runner for nodes whose runs don't matter.
fn noop(_: &mut Context, _: NodeKey) {}

/// What runners and rewinds did, in the order they did it.
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

fn sorted<T: Ord>(mut items: Vec<T>) -> Vec<T> {
    items.sort();
    items
}

#[gtest]
fn rewind_runs_every_rewind_once() {
    let mut ctx = Context::new();
    let node = ctx.create_node(noop).id();
    let keys: Vec<_> = (0..5)
        .map(|i| {
            ctx.register_rewind(node, move |ctx: &mut Context, _| log(ctx, i))
                .unwrap()
        })
        .collect();
    expect_eq!(
        sorted(ctx.rewind_keys(node).unwrap().collect::<Vec<_>>()),
        sorted(keys.clone())
    );

    ctx.rewind(node).unwrap();
    expect_that!(
        ctx.rewind_keys(node).unwrap().collect::<Vec<_>>(),
        is_empty()
    );
    ctx.rewind(node).unwrap();
    expect_eq!(sorted(logged(&ctx)), ["0", "1", "2", "3", "4"]);
    for key in keys {
        expect_false!(ctx.run_rewind(key));
        expect_that!(ctx.rewind_node_of(key), none());
    }
    expect_true!(ctx.get::<Rewinds>().unwrap().entries.is_empty());
    expect_true!(ctx.get::<Rewinds>().unwrap().by_node.is_empty());
}

#[gtest]
fn rewinds_registered_during_rewind_are_left_registered() {
    let mut ctx = Context::new();
    let node = ctx.create_node(noop).id();
    for i in 0..3 {
        ctx.register_rewind(node, move |ctx: &mut Context, _| {
            log(ctx, i);
            ctx.register_rewind(node, move |ctx: &mut Context, _| {
                log(ctx, format!("late {i}"))
            })
            .unwrap();
        })
        .unwrap();
    }

    ctx.rewind(node).unwrap();
    expect_eq!(sorted(logged(&ctx)), ["0", "1", "2"]);
    expect_that!(ctx.rewind_keys(node).unwrap().count(), eq(3));

    ctx.rewind(node).unwrap();
    expect_eq!(
        sorted(logged(&ctx)),
        ["0", "1", "2", "late 0", "late 1", "late 2"]
    );
    expect_that!(
        ctx.rewind_keys(node).unwrap().collect::<Vec<_>>(),
        is_empty()
    );
}

#[gtest]
fn a_rewind_gets_its_own_key_which_is_stale_afterwards() {
    /// The key the rewind got, and what it saw about it.
    struct Seen {
        key: RewindKey,
        node: Option<NodeKey>,
        listed: bool,
        ran_itself: bool,
    }

    let mut ctx = Context::new();
    let node = ctx.create_node(noop).id();
    let key = ctx
        .register_rewind(node, move |ctx: &mut Context, key| {
            // The iterator borrows `ctx`, so finish with it before `run_rewind`.
            let listed = ctx.rewind_keys(node).unwrap().any(|k| k == key);
            let seen = Seen {
                key,
                node: ctx.rewind_node_of(key),
                listed,
                ran_itself: ctx.run_rewind(key),
            };
            ctx.insert(seen);
        })
        .unwrap();

    ctx.rewind(node).unwrap();
    let seen = ctx.get::<Seen>().unwrap();
    expect_eq!(seen.key, key);
    expect_that!(seen.node, some(eq(node)));
    expect_true!(seen.listed);
    expect_false!(seen.ran_itself);

    expect_that!(ctx.rewind_node_of(key), none());
    expect_false!(ctx.run_rewind(key));
}

#[cfg(debug_assertions)]
#[gtest]
fn rewind_order_varies_in_debug_builds() {
    let orders: std::collections::HashSet<_> = (0..8)
        .map(|_| {
            let mut ctx = Context::new();
            let node = ctx.create_node(noop).id();
            for i in 0..8 {
                ctx.register_rewind(node, move |ctx: &mut Context, _| log(ctx, i))
                    .unwrap();
            }
            ctx.rewind(node).unwrap();
            logged(&ctx)
        })
        .collect();
    expect_that!(orders.len(), gt(1));
}

#[gtest]
fn run_rewind_on_a_sibling_runs_it_first_and_only_once() {
    // The order `rewind` runs them in varies, so try it a few times.
    for _ in 0..8 {
        let mut ctx = Context::new();
        let node = ctx.create_node(noop).id();
        let inner = ctx
            .register_rewind(node, |ctx: &mut Context, _| log(ctx, "inner"))
            .unwrap();
        ctx.register_rewind(node, move |ctx: &mut Context, _| {
            let inner_ran = !logged(ctx).is_empty();
            expect_eq!(ctx.run_rewind(inner), !inner_ran);
            expect_eq!(logged(ctx), ["inner"]);
            expect_false!(ctx.run_rewind(inner));
            log(ctx, "outer");
        })
        .unwrap();

        ctx.rewind(node).unwrap();
        expect_eq!(logged(&ctx), ["inner", "outer"]);
    }
}

#[gtest]
fn rewinding_its_own_node_runs_the_others_first() {
    for _ in 0..8 {
        let mut ctx = Context::new();
        let node = ctx.create_node(noop).id();
        for i in 0..3 {
            ctx.register_rewind(node, move |ctx: &mut Context, _| log(ctx, i))
                .unwrap();
        }
        ctx.register_rewind(node, move |ctx: &mut Context, _| {
            ctx.rewind(node).unwrap();
            log(ctx, "last");
        })
        .unwrap();

        ctx.rewind(node).unwrap();
        let log = logged(&ctx);
        let (last, others) = log.split_last().unwrap();
        expect_eq!(sorted(others.to_vec()), ["0", "1", "2"]);
        expect_eq!(last, "last");
    }
}

#[gtest]
fn a_rewind_can_rewind_another_node() {
    let mut ctx = Context::new();
    let other = ctx.create_node(noop).id();
    for i in 0..3 {
        ctx.register_rewind(other, move |ctx: &mut Context, _| log(ctx, i))
            .unwrap();
    }
    let node = ctx.create_node(noop).id();
    ctx.register_rewind(node, move |ctx: &mut Context, _| {
        ctx.rewind(other).unwrap();
        log(ctx, "outer");
    })
    .unwrap();

    ctx.rewind(node).unwrap();
    expect_that!(
        ctx.rewind_keys(other).unwrap().collect::<Vec<_>>(),
        is_empty()
    );
    let log = logged(&ctx);
    let (outer, inner) = log.split_last().unwrap();
    expect_eq!(sorted(inner.to_vec()), ["0", "1", "2"]);
    expect_eq!(outer, "outer");
}

#[gtest]
fn a_panicking_rewind_is_removed_and_the_rest_stay_registered() {
    let mut ctx = Context::new();
    let node = ctx.create_node(noop).id();
    let panicking = ctx
        .register_rewind(node, |_: &mut Context, _| panic!("the rewind panics"))
        .unwrap();
    for i in 0..4 {
        ctx.register_rewind(node, move |ctx: &mut Context, _| log(ctx, i))
            .unwrap();
    }

    let result = panic::catch_unwind(AssertUnwindSafe(|| ctx.rewind(node)));
    expect_true!(result.is_err());
    expect_that!(ctx.rewind_node_of(panicking), none());
    // The ones that didn't run before the panic are still registered.
    expect_eq!(
        ctx.rewind_keys(node).unwrap().count(),
        4 - logged(&ctx).len()
    );

    ctx.rewind(node).unwrap();
    expect_that!(
        ctx.rewind_keys(node).unwrap().collect::<Vec<_>>(),
        is_empty()
    );
    expect_eq!(sorted(logged(&ctx)), ["0", "1", "2", "3"]);
}

#[gtest]
fn a_rewind_can_delete_its_own_node() {
    let mut ctx = Context::new();
    let node = ctx.create_node(noop).id();
    let first = ctx
        .register_rewind(node, |ctx: &mut Context, _| log(ctx, "first"))
        .unwrap();
    ctx.register_rewind(node, move |ctx: &mut Context, _| {
        ctx.run_rewind(first);
        ctx.node_mut(node).unwrap().delete().unwrap();
        log(ctx, "deleted");
    })
    .unwrap();

    ctx.rewind(node).unwrap();
    expect_false!(ctx.contains_node(node));
    expect_eq!(logged(&ctx), ["first", "deleted"]);
}

#[gtest]
fn a_node_with_rewinds_can_run() {
    let mut ctx = Context::new();
    let node = ctx
        .create_node(|ctx: &mut Context, _: NodeKey| log(ctx, "ran"))
        .id();
    let key = ctx
        .register_rewind(node, |ctx: &mut Context, _| log(ctx, "rewound"))
        .unwrap();

    expect_eq!(ctx.node_mut(node).unwrap().run(), Ok(()));
    expect_eq!(logged(&ctx), ["ran"]);
    expect_eq!(ctx.rewind_keys(node).unwrap().collect::<Vec<_>>(), [key]);
}

#[gtest]
fn a_node_with_rewinds_can_be_deleted() {
    let mut ctx = Context::new();
    let node = ctx.create_node(noop).id();
    let key = ctx
        .register_rewind(node, |ctx: &mut Context, _| log(ctx, "rewound"))
        .unwrap();

    expect_eq!(ctx.node_mut(node).unwrap().delete(), Ok(()));
    expect_false!(ctx.contains_node(node));
    expect_that!(logged(&ctx), is_empty());
    // The rewind is left behind.
    expect_that!(ctx.rewind_node_of(key), some(eq(node)));
}

#[gtest]
fn unknown_nodes_are_refused() {
    let mut ctx = Context::new();
    let node = ctx.create_node(noop).id();
    ctx.node_mut(node).unwrap().delete().unwrap();

    expect_that!(
        ctx.register_rewind(node, |_: &mut Context, _| {}).err(),
        some(eq(UnknownNode(node)))
    );
    expect_that!(ctx.rewind_keys(node).err(), some(eq(UnknownNode(node))));
    expect_that!(ctx.rewind(node).err(), some(eq(UnknownNode(node))));
}
