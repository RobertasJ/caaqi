//! Nodes that read tracked cells, rerun by the linear executor when those
//! cells are written.
//!
//! `greeting` reads `name`. `double` reads `count` and writes `doubled`.
//! `summary` reads `count` and `doubled`. Writing a cell queues its readers,
//! and `execute_queued` reruns them in trace order until nothing is queued.
//!
//! Run with `cargo run --example executor`.

use caaqi_scoped::{
    current::current_node, prelude::*, rewind::register_rewind, trace::create_node,
};
use caaqi_ui::{
    linear_executor::{NodeExecutionExt, execute_queued},
    tracked_cell::store_tracked_cell,
};

fn main() {
    let mut ctx = Context::new();
    scope(&mut ctx, || {
        let name = store_tracked_cell(String::from("Ada"));
        let count = store_tracked_cell(1);
        let doubled = store_tracked_cell(0);

        // The executor sorts queued nodes by trace order, which only exists
        // inside one tree, so every reader is a child of the same root.
        println!("first run:");
        run(move || {
            run(move || {
                println!("  greeting: hello, {}", *name.read());
            });

            run(move || {
                let count = *count.read();
                let doubled = *doubled.read();
                println!("  summary: count {count}, doubled {doubled}");
            });

            run(move || {
                let value = *count.read() * 2;
                println!("  double: doubled = {value}");
                *doubled.write() = value;
            });
        });
        // `double` wrote `doubled` after `summary` read it, which queued
        // `summary` to run again with the new value.
        execute_queued();

        // Only `greeting` reads `name`.
        println!("name = \"Grace\":");
        *name.write() = String::from("Grace");
        execute_queued();

        // `summary` and `double` read `count`, and run in trace order. Then
        // `double` writes `doubled`, which queues `summary` again. Its turn in
        // this round has passed, so it runs again in the next round.
        println!("count = 5:");
        *count.write() = 5;
        execute_queued();

        // Writes before `execute_queued` add up: each reader is queued once.
        println!("name = \"Alan\", count = 7:");
        *name.write() = String::from("Alan");
        *count.write() = 7;
        execute_queued();
    });
}

/// Creates a node running `f` and runs it. Called from a running node, it
/// makes a child of that node, which is rewound and deleted when the parent
/// is rewound. Called from outside of any node, it makes a root.
fn run(f: impl FnMut() + 'static) -> TraceNode {
    let parent = current_node();
    let node = create_node(WithCurrent(f));

    if let Some(parent) = parent {
        node.set_parent(parent)
            .expect("a new node can be attached to any node");
        register_rewind(parent, move |_: RewindKey| {
            // The node may have been deleted since.
            if node.exists() {
                node.rewind();
                node.unqueue_execution();
                node.delete()
                    .expect("rewinding the node deleted its children, and it isn't running");
            }
        })
        .expect("the parent is running, so it's in the trace");
    }

    node.run().expect("a new node isn't running");
    node
}
