//! An executor kept in the [`Context`], reached through [`LinearExecutorExt`].

use std::{cmp::Ordering, collections::VecDeque};

use caaqi::{
    context::Context,
    current::CurrentExt,
    rewind::NodeRewindExt,
    trace::{NodeKey, TraceExt},
};

/// The executor's resource. Everything public goes through
/// [`LinearExecutorExt`].
///
/// Execution goes through `queue` left to right in rounds. Each node is
/// inserted where it belongs in trace order, so the queue never needs
/// sorting, and taken out once it has run.
#[derive(Default)]
struct LinearExecutor {
    /// The queued nodes, in trace order, each at most once.
    queue: VecDeque<NodeKey>,
    /// While executing, the index of the node running now. The nodes after
    /// it run later in this round. The ones before it were queued after
    /// their turn had passed, and wait for the next round.
    cursor: Option<usize>,
}

impl LinearExecutor {
    /// Starts a round at the left end of the queue. Returns `false`, and
    /// stops executing, if nothing is queued.
    fn start_round(&mut self) -> bool {
        if self.queue.is_empty() {
            self.cursor = None;
            false
        } else {
            self.cursor = Some(0);
            true
        }
    }

    /// The node at the cursor, which runs now, or `None` once the round has
    /// reached the end of the queue.
    fn current(&self) -> Option<NodeKey> {
        self.queue.get(self.cursor?).copied()
    }

    /// Takes the current node out of the queue once it has run, leaving the
    /// cursor on the next node.
    fn finish_at_cursor(&mut self) {
        let cursor = self.cursor.expect("a round is running");
        self.queue.remove(cursor).expect("the cursor is on a node");
    }

    /// Drops deleted nodes from the queue: they can't be compared, and
    /// can't run. The cursor moves back by the ones dropped before it, to
    /// stay on the same node.
    fn drop_deleted(&mut self, ctx: &Context) {
        if let Some(cursor) = &mut self.cursor {
            *cursor -= self
                .queue
                .range(..*cursor)
                .filter(|&&queued| !ctx.contains_node(queued))
                .count();
        }
        self.queue.retain(|&queued| ctx.contains_node(queued));
    }

    /// Inserts `node` where it belongs in trace order, unless it's already
    /// queued or running now. Inserted before the running node, it waits for
    /// the next round, and the cursor moves to stay on the running node.
    fn insert(&mut self, ctx: &Context, node: NodeKey) {
        // Comparing needs every queued node in the trace.
        self.drop_deleted(ctx);

        let position = self.queue.partition_point(|&queued| {
            ctx.cmp_nodes(queued, node)
                .expect("queued nodes are in the trace, and in the same tree")
                == Ordering::Less
        });

        if self.queue.get(position) == Some(&node) {
            return;
        }

        self.queue.insert(position, node);
        if let Some(cursor) = &mut self.cursor
            && position <= *cursor
        {
            *cursor += 1;
        }
    }
}

pub trait LinearExecutorExt {
    /// Runs the queued nodes in trace order, rewinding each first, round
    /// after round until nothing is queued.
    fn execute_queued(&mut self);

    /// Queues `node` to run. While executing, a node after the running one
    /// runs in the same round; any other runs in the next round. Queuing a
    /// node that's already queued, or running, does nothing.
    fn queue_execution(&mut self, node: NodeKey);
}

impl LinearExecutorExt for Context {
    fn execute_queued(&mut self) {
        assert!(
            self.current_node().is_none(),
            "can't execute queued nodes while running a node"
        );
        assert!(
            self.current_rewind().is_none(),
            "can't execute queued nodes while rewinding a node"
        );

        while self.get_or_default::<LinearExecutor>().start_round() {
            // Dropping deleted nodes before each one runs means the node at
            // the cursor is always in the trace. It stays there until it
            // finishes: a running node can't be deleted, and a rewind must
            // not delete its own node.
            while let Some(node) = self.resource_scope(|ctx, executor: &mut LinearExecutor| {
                executor.drop_deleted(ctx);
                executor.current()
            }) {
                let mut handle = self
                    .node_mut(node)
                    .expect("deleted nodes were dropped from the queue");
                handle.rewind();
                handle.run().unwrap();
                self.get_or_default::<LinearExecutor>().finish_at_cursor();
            }
        }
    }

    fn queue_execution(&mut self, node: NodeKey) {
        self.get_or_default::<LinearExecutor>();
        // Lend the executor out, so the trace can be read while changing it.
        self.resource_scope(|ctx, executor: &mut LinearExecutor| {
            executor.insert(ctx, node);
        });
    }
}

#[cfg(test)]
#[path = "linear_executor_tests.rs"]
mod tests;
