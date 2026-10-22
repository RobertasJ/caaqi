use std::collections::HashMap;

use caaqi_scoped::{
    context::{Context, get, scope},
    rewind::{RewindKey, register_rewind},
    trace::create_node,
};

use super::*;

enum Action {
    Queue(TraceNode),
    /// Unqueues the node, and checks whether it was queued.
    Unqueue(TraceNode, bool),
    /// Rewinds the node, unqueues it and deletes it.
    Delete(TraceNode),
}

/// What each node does the first time it runs.
#[derive(Default)]
struct Plan(HashMap<TraceNode, Vec<Action>>);

/// The nodes that ran, in order.
#[derive(Default)]
struct Ran(Vec<TraceNode>);

fn runner(node: TraceNode) {
    get_or_default(|ran: &mut Ran| ran.0.push(node));
    let actions = get_or_default(|plan: &mut Plan| plan.0.remove(&node));
    for action in actions.into_iter().flatten() {
        match action {
            Action::Queue(other) => other.queue_execution(),
            Action::Unqueue(other, queued) => assert_eq!(other.unqueue_execution(), queued),
            Action::Delete(other) => {
                other.rewind();
                other.unqueue_execution();
                other.delete().unwrap();
            }
        }
    }
}

/// A root with four children, `[c0, c1, c2, c3]` in trace order. Nothing has
/// run.
fn tree() -> [TraceNode; 4] {
    let root = create_node(|_: TraceNode| {});
    [(); 4].map(|()| create_node(runner).set_parent(root).unwrap())
}

fn plan(node: TraceNode, actions: Vec<Action>) {
    get_or_default(|plan: &mut Plan| plan.0.insert(node, actions));
}

fn ran() -> Vec<TraceNode> {
    get_or_default(|ran: &mut Ran| ran.0.clone())
}

#[test]
fn runs_queued_nodes_in_trace_order() {
    let mut ctx = Context::new();
    scope(&mut ctx, || {
        let [c0, c1, c2, _] = tree();
        c2.queue_execution();
        c0.queue_execution();
        c1.queue_execution();
        c0.queue_execution();

        execute_queued();
        assert_eq!(ran(), [c0, c1, c2]);
    });
}

#[test]
fn nodes_ahead_join_the_round_and_nodes_behind_wait() {
    let mut ctx = Context::new();
    scope(&mut ctx, || {
        let [c0, c1, c2, c3] = tree();
        plan(c1, vec![Action::Queue(c0), Action::Queue(c3)]);
        c1.queue_execution();
        c2.queue_execution();

        execute_queued();
        assert_eq!(ran(), [c1, c2, c3, c0]);
    });
}

#[test]
fn queuing_the_running_node_does_nothing() {
    let mut ctx = Context::new();
    scope(&mut ctx, || {
        let [c0, ..] = tree();
        plan(c0, vec![Action::Queue(c0)]);
        c0.queue_execution();

        execute_queued();
        assert_eq!(ran(), [c0]);
    });
}

fn queue_is_empty() -> bool {
    get(|executor: &LinearExecutor| executor.queue.is_empty()).unwrap()
}

#[test]
fn unqueued_nodes_dont_run() {
    let mut ctx = Context::new();
    scope(&mut ctx, || {
        let [c0, c1, c2, _] = tree();
        c0.queue_execution();
        c1.queue_execution();
        assert!(c0.unqueue_execution());
        assert!(!c0.unqueue_execution());
        assert!(!c2.unqueue_execution());

        execute_queued();
        assert_eq!(ran(), [c1]);
    });
}

#[test]
fn rewinding_a_queued_node_keeps_it_queued() {
    let mut ctx = Context::new();
    scope(&mut ctx, || {
        let [c0, ..] = tree();
        c0.queue_execution();
        c0.rewind();

        execute_queued();
        assert_eq!(ran(), [c0]);
    });
}

#[test]
fn the_running_node_isnt_queued() {
    let mut ctx = Context::new();
    scope(&mut ctx, || {
        let [c0, ..] = tree();
        plan(c0, vec![Action::Unqueue(c0, false)]);
        c0.queue_execution();

        execute_queued();
        assert_eq!(ran(), [c0]);
    });
}

#[test]
fn deleted_nodes_ahead_are_skipped() {
    let mut ctx = Context::new();
    scope(&mut ctx, || {
        let [_, c1, c2, c3] = tree();
        plan(c1, vec![Action::Delete(c2), Action::Queue(c3)]);
        c1.queue_execution();
        c2.queue_execution();

        execute_queued();
        assert_eq!(ran(), [c1, c3]);
        assert!(queue_is_empty());
    });
}

#[test]
fn deleted_nodes_behind_keep_the_cursor_on_the_next_node() {
    let mut ctx = Context::new();
    scope(&mut ctx, || {
        let [c0, c1, c2, c3] = tree();
        plan(c1, vec![Action::Queue(c0)]);
        // `c0` waits behind the cursor; deleting it takes it out from
        // before the cursor.
        plan(c2, vec![Action::Delete(c0), Action::Queue(c3)]);
        c1.queue_execution();
        c2.queue_execution();

        execute_queued();
        assert_eq!(ran(), [c1, c2, c3]);
        assert!(queue_is_empty());
    });
}

#[test]
fn nodes_queued_while_rewinding_keep_their_place() {
    let mut ctx = Context::new();
    scope(&mut ctx, || {
        let [c0, c1, c2, c3] = tree();
        // Whichever order `c1`'s rewinds run in, `c0` waits for the next
        // round and `c3` joins this one.
        for other in [c0, c3, c1] {
            register_rewind(c1, move |_: RewindKey| other.queue_execution()).unwrap();
        }
        c1.queue_execution();
        c2.queue_execution();

        execute_queued();
        assert_eq!(ran(), [c1, c2, c3, c0]);
    });
}
