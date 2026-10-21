//! Nodes that read tracked cells, rerun by the linear executor when those
//! cells are written.
//!
//! `greeting` reads `name`. `double` reads `count` and writes `doubled`.
//! `summary` reads `count` and `doubled`. Writing a cell queues its readers,
//! and `execute_queued` reruns them in trace order until nothing is queued.
//!
//! Run with `cargo run --example executor`.

use caaqi::prelude::*;
use caaqi_ui::{linear_executor::LinearExecutorExt, tracked_cell::TrackedCellExt};

fn main() {
    let mut ctx = Context::new();
    let name = ctx.store_tracked_cell(String::from("Ada"));
    let count = ctx.store_tracked_cell(1);
    let doubled = ctx.store_tracked_cell(0);

    // The executor sorts queued nodes by trace order, which only exists
    // inside one tree, so every reader is a child of the same root.
    println!("first run:");
    run(&mut ctx, move |ctx| {
        run(ctx, move |ctx| {
            println!("  greeting: hello, {}", ctx.borrow_tracked_cell(name));
        });

        run(ctx, move |ctx| {
            let count = *ctx.borrow_tracked_cell(count);
            let doubled = *ctx.borrow_tracked_cell(doubled);
            println!("  summary: count {count}, doubled {doubled}");
        });

        run(ctx, move |ctx| {
            let value = *ctx.borrow_tracked_cell(count) * 2;
            println!("  double: doubled = {value}");
            *ctx.borrow_tracked_cell_mut(doubled) = value;
        });
    });
    // `double` wrote `doubled` after `summary` read it, which queued
    // `summary` to run again with the new value.
    ctx.execute_queued();

    // Only `greeting` reads `name`.
    println!("name = \"Grace\":");
    *ctx.borrow_tracked_cell_mut(name) = String::from("Grace");
    ctx.execute_queued();

    // `summary` and `double` read `count`, and run in trace order. Then
    // `double` writes `doubled`, which queues `summary` again. Its turn in
    // this round has passed, so it runs again in the next round.
    println!("count = 5:");
    *ctx.borrow_tracked_cell_mut(count) = 5;
    ctx.execute_queued();

    // Writes before `execute_queued` add up: each reader is queued once.
    println!("name = \"Alan\", count = 7:");
    *ctx.borrow_tracked_cell_mut(name) = String::from("Alan");
    *ctx.borrow_tracked_cell_mut(count) = 7;
    ctx.execute_queued();
}

/// Creates a node running `f` and runs it. Called from a running node, it
/// makes a child of that node, which is rewound and deleted when the parent
/// is rewound. Called from outside of any node, it makes a root.
fn run(ctx: &mut Context, f: impl FnMut(&mut Context) + 'static) -> NodeKey {
    let parent = ctx.current_node();
    let mut node = ctx.create_node(WithCurrent(f));
    if let Some(parent) = parent {
        node.set_parent(parent)
            .expect("a new node can be attached to any node");
    }
    let node = node.id();

    if let Some(parent) = parent {
        ctx.register_rewind(parent, move |ctx: &mut Context, _| {
            // The node may have been deleted since.
            if let Ok(mut node) = ctx.node_mut(node) {
                node.rewind();
                node.delete()
                    .expect("rewinding the node deleted its children, and it isn't running");
            }
        })
        .expect("the parent is running, so it's in the trace");
    }

    ctx.node_mut(node)
        .unwrap()
        .run()
        .expect("a new node isn't running");
    node
}
