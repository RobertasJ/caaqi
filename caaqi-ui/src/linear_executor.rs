//! An executor kept in the context:
//! [`queue_execution`](NodeExecutionExt::queue_execution) queues nodes, and
//! [`execute_queued`] runs them.
//!
//! The executor never checks for deleted nodes: whoever deletes a node calls
//! [`unqueue_execution`](NodeExecutionExt::unqueue_execution) on it first,
//! usually in the rewind that deletes it.

use std::{cmp::Ordering, collections::VecDeque};

use caaqi_scoped::{
    context::{get_or_default, resource_scope},
    current::{current_node, current_rewind},
    trace::TraceNode,
};

/// The executor's resource. Everything public goes through
/// [`NodeExecutionExt`] and [`execute_queued`].
///
/// Execution goes through `queue` left to right in rounds. Each node is
/// inserted where it belongs in trace order, so the queue never needs
/// sorting, and taken out right before it's rewound and run.
#[derive(Default)]
struct LinearExecutor {
    /// The queued nodes, in trace order, each at most once.
    queue: VecDeque<TraceNode>,
    /// While executing, the index of the next node to run. The nodes from
    /// it on run later in this round. The ones before it were queued after
    /// their turn had passed, and wait for the next round.
    cursor: Option<usize>,
    /// While executing, the node being rewound and run. It's no longer in
    /// the queue.
    running: Option<TraceNode>,
}

impl LinearExecutor {
    /// Starts a round at the left end of the queue. Returns `false`, and
    /// stops executing, if nothing is queued.
    fn start_round(&mut self) -> bool {
        self.running = None;
        if self.queue.is_empty() {
            self.cursor = None;
            false
        } else {
            self.cursor = Some(0);
            true
        }
    }

    /// Takes the node at the cursor out of the queue, leaving the cursor on
    /// the node after it, and marks it as running. Returns it, or `None` once
    /// the round has reached the end of the queue.
    fn start_next(&mut self) -> Option<TraceNode> {
        self.running = self.queue.remove(self.cursor?);
        self.running
    }

    /// Inserts `node` where it belongs in trace order, unless it's already
    /// queued or running now. Inserted before the running node, it waits for
    /// the next round, and the cursor moves to stay ahead of it.
    fn insert(&mut self, node: TraceNode) {
        if self.running == Some(node) {
            return;
        }

        let position = self
            .queue
            .partition_point(|&queued| cmp_queued(queued, node) == Ordering::Less);
        if self.queue.get(position) == Some(&node) {
            return;
        }

        self.queue.insert(position, node);
        // The nodes before the cursor come before the running node, and the
        // ones from it on after, so comparing with the running node says
        // which side `node` landed on.
        if let Some(cursor) = &mut self.cursor
            && let Some(running) = self.running
            && cmp_queued(node, running) == Ordering::Less
        {
            *cursor += 1;
        }
    }

    /// Takes `node` out of the queue, and returns whether it was there.
    /// Removed before the cursor, the cursor moves back to stay on the same
    /// node; removed at the cursor, the cursor moves on to the node after it.
    fn remove(&mut self, node: TraceNode) -> bool {
        // Searched linearly: it needn't compare nodes, so `node` may already
        // be deleted, and removing from the middle is linear anyway.
        let Some(position) = self.queue.iter().position(|&queued| queued == node) else {
            return false;
        };
        self.queue.remove(position);
        if let Some(cursor) = &mut self.cursor
            && position < *cursor
        {
            *cursor -= 1;
        }
        true
    }
}

/// Compares two nodes that the executor holds in trace order.
fn cmp_queued(a: TraceNode, b: TraceNode) -> Ordering {
    a.trace_cmp(b).expect(
        "queued nodes are in the trace, since they're unqueued before they're deleted, and in the same tree",
    )
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
        while let Some(node) = get_or_default(|executor: &mut LinearExecutor| executor.start_next())
        {
            node.rewind();
            node.run().unwrap();
        }
    }
}

/// The executor's operations on a node.
pub trait NodeExecutionExt {
    /// Queues the node to run. While executing, a node after the running one
    /// runs in the same round; any other runs in the next round. Queuing a
    /// node that's already queued, or running, does nothing.
    ///
    /// Call [`unqueue_execution`](Self::unqueue_execution) before deleting a
    /// queued node.
    ///
    /// # Panics
    ///
    /// If the node isn't in the trace.
    fn queue_execution(self);

    /// Takes the node out of the queue, so it doesn't run. Returns whether
    /// it was queued; the node running now isn't. Call it before deleting a
    /// node that may be queued, usually in the rewind that deletes it: the
    /// executor doesn't check for deleted nodes.
    fn unqueue_execution(self) -> bool;
}

impl NodeExecutionExt for TraceNode {
    fn queue_execution(self) {
        assert!(self.exists(), "can't queue a node that isn't in the trace");

        get_or_default(|_: &mut LinearExecutor| {});
        // Lend the executor out, so the trace can be read while changing it.
        resource_scope(|executor: &mut LinearExecutor| executor.insert(self));
    }

    fn unqueue_execution(self) -> bool {
        get_or_default(|executor: &mut LinearExecutor| executor.remove(self))
    }
}

#[cfg(test)]
#[path = "linear_executor_tests.rs"]
mod tests;
