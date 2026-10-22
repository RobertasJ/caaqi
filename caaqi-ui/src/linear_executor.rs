//! An executor kept in the context: [`queue_execution`] queues nodes, and
//! [`execute_queued`] runs them.

use std::{cmp::Ordering, collections::VecDeque};

use caaqi_scoped::{
    context::{get_or_default, resource_scope},
    current::{current_node, current_rewind},
    trace::TraceNode,
};

/// The executor's resource. Everything public goes through
/// [`queue_execution`] and [`execute_queued`].
///
/// Execution goes through `queue` left to right in rounds. Each node is
/// inserted where it belongs in trace order, so the queue never needs
/// sorting, and taken out once it has run.
#[derive(Default)]
struct LinearExecutor {
    /// The queued nodes, in trace order, each at most once.
    queue: VecDeque<TraceNode>,
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
    fn current(&self) -> Option<TraceNode> {
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
    fn drop_deleted(&mut self) {
        if let Some(cursor) = &mut self.cursor {
            *cursor -= self
                .queue
                .range(..*cursor)
                .filter(|queued| !queued.exists())
                .count();
        }
        self.queue.retain(|queued| queued.exists());
    }

    /// Inserts `node` where it belongs in trace order, unless it's already
    /// queued or running now. Inserted before the running node, it waits for
    /// the next round, and the cursor moves to stay on the running node.
    fn insert(&mut self, node: TraceNode) {
        // Comparing needs every queued node in the trace.
        self.drop_deleted();

        let position = self.queue.partition_point(|&queued| {
            queued
                .trace_cmp(node)
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

/// Runs the queued nodes in trace order, rewinding each first, round after
/// round until nothing is queued.
pub fn execute_queued() {
    assert!(
        current_node().is_none(),
        "can't execute queued nodes while running a node"
    );
    assert!(
        current_rewind().is_none(),
        "can't execute queued nodes while rewinding a node"
    );

    while get_or_default(|executor: &mut LinearExecutor| executor.start_round()) {
        // Dropping deleted nodes before each one runs means the node at the
        // cursor is always in the trace. It stays there until it finishes:
        // a running node can't be deleted, and a rewind must not delete its
        // own node.
        while let Some(node) = resource_scope(|executor: &mut LinearExecutor| {
            executor.drop_deleted();
            executor.current()
        }) {
            node.rewind();
            node.run().unwrap();
            get_or_default(|executor: &mut LinearExecutor| executor.finish_at_cursor());
        }
    }
}

/// Queues `node` to run. While executing, a node after the running one runs
/// in the same round; any other runs in the next round. Queuing a node
/// that's already queued, or running, does nothing.
pub fn queue_execution(node: TraceNode) {
    get_or_default(|_: &mut LinearExecutor| {});
    // Lend the executor out, so the trace can be read while changing it.
    resource_scope(|executor: &mut LinearExecutor| executor.insert(node));
}

#[cfg(test)]
#[path = "linear_executor_tests.rs"]
mod tests;
