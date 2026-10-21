use std::collections::HashMap;

use super::*;

enum Action {
    Queue(NodeKey),
    Delete(NodeKey),
}

/// What each node does the first time it runs.
#[derive(Default)]
struct Plan(HashMap<NodeKey, Vec<Action>>);

/// The nodes that ran, in order.
#[derive(Default)]
struct Ran(Vec<NodeKey>);

fn runner(ctx: &mut Context, node: NodeKey) {
    ctx.get_or_default::<Ran>().0.push(node);
    let actions = ctx.get_or_default::<Plan>().0.remove(&node);
    for action in actions.into_iter().flatten() {
        match action {
            Action::Queue(other) => ctx.queue_execution(other),
            Action::Delete(other) => ctx.node_mut(other).unwrap().delete().unwrap(),
        }
    }
}

/// A root with four children, `[c0, c1, c2, c3]` in trace order. Nothing has
/// run.
fn tree() -> (Context, [NodeKey; 4]) {
    let mut ctx = Context::new();
    let root = ctx.create_node(|_: &mut Context, _| {}).id();
    let children = [(); 4].map(|()| {
        let mut child = ctx.create_node(runner);
        child.set_parent(root).unwrap();
        child.id()
    });
    (ctx, children)
}

fn plan(ctx: &mut Context, node: NodeKey, actions: Vec<Action>) {
    ctx.get_or_default::<Plan>().0.insert(node, actions);
}

fn ran(ctx: &Context) -> &[NodeKey] {
    ctx.get::<Ran>().map_or(&[], |ran| &ran.0)
}

#[test]
fn runs_queued_nodes_in_trace_order() {
    let (mut ctx, [c0, c1, c2, _]) = tree();
    ctx.queue_execution(c2);
    ctx.queue_execution(c0);
    ctx.queue_execution(c1);
    ctx.queue_execution(c0);

    ctx.execute_queued();
    assert_eq!(ran(&ctx), [c0, c1, c2]);
}

#[test]
fn nodes_ahead_join_the_round_and_nodes_behind_wait() {
    let (mut ctx, [c0, c1, c2, c3]) = tree();
    plan(&mut ctx, c1, vec![Action::Queue(c0), Action::Queue(c3)]);
    ctx.queue_execution(c1);
    ctx.queue_execution(c2);

    ctx.execute_queued();
    assert_eq!(ran(&ctx), [c1, c2, c3, c0]);
}

#[test]
fn queuing_the_running_node_does_nothing() {
    let (mut ctx, [c0, ..]) = tree();
    plan(&mut ctx, c0, vec![Action::Queue(c0)]);
    ctx.queue_execution(c0);

    ctx.execute_queued();
    assert_eq!(ran(&ctx), [c0]);
}

#[test]
fn deleted_nodes_ahead_are_skipped() {
    let (mut ctx, [_, c1, c2, c3]) = tree();
    plan(&mut ctx, c1, vec![Action::Delete(c2), Action::Queue(c3)]);
    ctx.queue_execution(c1);
    ctx.queue_execution(c2);

    ctx.execute_queued();
    assert_eq!(ran(&ctx), [c1, c3]);
}

#[test]
fn deleted_nodes_behind_keep_the_cursor_on_the_running_node() {
    let (mut ctx, [c0, c1, c2, c3]) = tree();
    plan(&mut ctx, c1, vec![Action::Queue(c0)]);
    // `c0` waits behind the cursor; deleting it and queuing again drops it
    // from before the cursor.
    plan(&mut ctx, c2, vec![Action::Delete(c0), Action::Queue(c3)]);
    ctx.queue_execution(c1);
    ctx.queue_execution(c2);

    ctx.execute_queued();
    assert_eq!(ran(&ctx), [c1, c2, c3]);
    assert!(ctx.get::<LinearExecutor>().unwrap().queue.is_empty());
}
