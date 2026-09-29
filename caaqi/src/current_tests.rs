use std::panic::{AssertUnwindSafe, catch_unwind};

use googletest::prelude::*;

use super::*;
use crate::{rewind::RewindExt, trace::TraceExt};

/// What `current_node` / `current_rewind` were, in the order they were read.
#[derive(Default)]
struct Seen {
    nodes: Vec<Option<NodeKey>>,
    rewinds: Vec<Option<RewindKey>>,
}

fn see(ctx: &mut Context) {
    let node = ctx.current_node();
    let rewind = ctx.current_rewind();
    let seen = ctx.get_or_insert_with(Seen::default);
    seen.nodes.push(node);
    seen.rewinds.push(rewind);
}

fn seen_nodes(ctx: &Context) -> Vec<Option<NodeKey>> {
    ctx.get::<Seen>()
        .map_or_else(Vec::new, |seen| seen.nodes.clone())
}

fn seen_rewinds(ctx: &Context) -> Vec<Option<RewindKey>> {
    ctx.get::<Seen>()
        .map_or_else(Vec::new, |seen| seen.rewinds.clone())
}

fn run(ctx: &mut Context, node: NodeKey) {
    ctx.node_mut(node).unwrap().run().unwrap();
}

#[gtest]
fn outside_of_anything_running_there_is_nothing_current() {
    let ctx = Context::new();

    expect_that!(ctx.current_node(), none());
    expect_that!(ctx.current_rewind(), none());
}

#[gtest]
fn current_node_is_the_innermost_running_node() {
    let mut ctx = Context::new();
    let inner = ctx.create_node(WithCurrent(see)).id();
    let outer = ctx
        .create_node(WithCurrent(move |ctx: &mut Context| {
            see(ctx);
            run(ctx, inner);
            see(ctx);
        }))
        .id();

    run(&mut ctx, outer);

    expect_eq!(seen_nodes(&ctx), [Some(outer), Some(inner), Some(outer)]);
    expect_that!(ctx.current_node(), none());
}

#[gtest]
fn a_runner_with_node_does_not_change_the_current_node() {
    let mut ctx = Context::new();
    let raw = ctx.create_node(|ctx: &mut Context, _| see(ctx)).id();
    let outer = ctx
        .create_node(WithCurrent(move |ctx: &mut Context| run(ctx, raw)))
        .id();

    run(&mut ctx, outer);

    expect_eq!(seen_nodes(&ctx), [Some(outer)]);
}

#[gtest]
fn a_panicking_node_is_popped() {
    let mut ctx = Context::new();
    let panics = ctx
        .create_node(WithCurrent(|_: &mut Context| panic!("the node panics")))
        .id();
    let outer = ctx
        .create_node(WithCurrent(move |ctx: &mut Context| {
            let panicked = catch_unwind(AssertUnwindSafe(|| run(ctx, panics)));
            assert!(panicked.is_err());
            see(ctx);
        }))
        .id();

    run(&mut ctx, outer);

    expect_eq!(seen_nodes(&ctx), [Some(outer)]);
    expect_that!(ctx.current_node(), none());
}

#[gtest]
fn current_rewind_is_the_innermost_running_rewind() {
    let mut ctx = Context::new();
    let node = ctx.create_node(|_: &mut Context, _| {}).id();
    let inner = ctx.register_rewind(node, WithCurrent(see)).unwrap();
    let outer = ctx
        .register_rewind(
            node,
            WithCurrent(move |ctx: &mut Context| {
                see(ctx);
                ctx.run_rewind(inner);
                see(ctx);
            }),
        )
        .unwrap();

    ctx.run_rewind(outer);

    expect_eq!(seen_rewinds(&ctx), [Some(outer), Some(inner), Some(outer)]);
    expect_that!(ctx.current_rewind(), none());
}

#[gtest]
fn a_rewind_with_key_does_not_change_the_current_rewind() {
    let mut ctx = Context::new();
    let node = ctx.create_node(|_: &mut Context, _| {}).id();
    ctx.register_rewind(node, |ctx: &mut Context, _| see(ctx))
        .unwrap();

    ctx.rewind(node).unwrap();

    expect_eq!(seen_rewinds(&ctx), [None]);
}

#[gtest]
fn a_panicking_rewind_is_popped() {
    let mut ctx = Context::new();
    let node = ctx.create_node(|_: &mut Context, _| {}).id();
    let panics = ctx
        .register_rewind(
            node,
            WithCurrent(|_: &mut Context| panic!("the rewind panics")),
        )
        .unwrap();

    let panicked = catch_unwind(AssertUnwindSafe(|| ctx.run_rewind(panics)));

    expect_true!(panicked.is_err());
    expect_that!(ctx.current_rewind(), none());
}

#[gtest]
fn a_rewind_run_inside_a_node_sees_both() {
    let mut ctx = Context::new();
    let node = ctx
        .create_node(WithCurrent(|ctx: &mut Context| {
            let node = ctx.current_node().unwrap();
            let rewind = ctx.register_rewind(node, WithCurrent(see)).unwrap();
            ctx.run_rewind(rewind);
            let seen = ctx.get_or_insert_with(Seen::default);
            assert_eq!(seen.rewinds, [Some(rewind)]);
        }))
        .id();

    run(&mut ctx, node);

    expect_eq!(seen_nodes(&ctx), [Some(node)]);
}
