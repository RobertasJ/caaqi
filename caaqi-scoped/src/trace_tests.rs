use std::cmp::Ordering;

use super::*;
use crate::{
    context::{get_or_default, scope},
    current::{WithCurrent, current_node},
};

#[derive(Default)]
struct Ran(Vec<TraceNode>);

fn ran() -> Vec<TraceNode> {
    get_or_default(|ran: &mut Ran| ran.0.clone())
}

#[test]
fn trace_nodes_build_and_read_the_trace() {
    let mut ctx = caaqi::context::Context::new();
    scope(&mut ctx, || {
        let root = create_node(|_: TraceNode| {});
        let a = create_node(|_: TraceNode| {});
        let b = create_node(|_: TraceNode| {});
        a.set_parent(root).unwrap();
        root.add_child(b).unwrap();

        assert_eq!(root.children(), [a, b]);
        assert_eq!(a.parent(), Some(root));
        assert_eq!(a.next(), Some(b));
        assert_eq!(b.prev(), Some(a));
        assert_eq!(a.trace_cmp(b), Ok(Ordering::Less));

        assert!(b.detach());
        b.delete().unwrap();
        assert!(!b.exists());
        assert!(a.exists());
    });
}

#[test]
fn runners_can_use_the_crate() {
    let mut ctx = caaqi::context::Context::new();
    scope(&mut ctx, || {
        let with_key = create_node(|node: TraceNode| {
            get_or_default(|ran: &mut Ran| ran.0.push(node));
            assert!(node.is_running());
        });
        let current = create_node(WithCurrent(|| {
            let node = current_node().unwrap();
            get_or_default(|ran: &mut Ran| ran.0.push(node));
        }));

        with_key.run().unwrap();
        current.run().unwrap();
        assert_eq!(ran(), [with_key, current]);
        assert!(!with_key.is_running());
    });
}

#[test]
fn running_a_running_node_is_refused() {
    let mut ctx = caaqi::context::Context::new();
    scope(&mut ctx, || {
        let node = create_node(WithCurrent(|| {
            let node = current_node().unwrap();
            assert_eq!(node.run(), Err(RunnerInUse(node.into())));
        }));
        node.run().unwrap();
    });
}

#[test]
#[should_panic(expected = "was deleted")]
fn a_deleted_node_panics() {
    let mut ctx = caaqi::context::Context::new();
    scope(&mut ctx, || {
        let node = create_node(|_: TraceNode| {});
        node.delete().unwrap();
        node.parent();
    });
}

#[test]
fn trace_cmp_compares_by_trace_order() {
    let mut ctx = caaqi::context::Context::new();
    scope(&mut ctx, || {
        let root = create_node(|_: TraceNode| {});
        let a = create_node(|_: TraceNode| {}).set_parent(root).unwrap();
        let a1 = create_node(|_: TraceNode| {}).set_parent(a).unwrap();
        let b = create_node(|_: TraceNode| {}).set_parent(root).unwrap();
        let other_tree = create_node(|_: TraceNode| {});

        assert_eq!(root.trace_cmp(a), Ok(Ordering::Less));
        assert_eq!(a1.trace_cmp(b), Ok(Ordering::Less));
        assert_eq!(b.trace_cmp(a1), Ok(Ordering::Greater));
        assert_eq!(a.trace_cmp(a), Ok(Ordering::Equal));
        assert_eq!(
            a.trace_cmp(other_tree),
            Err(CmpNodesError::DifferentTrees(DifferentTrees {
                a: a.into(),
                b: other_tree.into()
            }))
        );

        let mut nodes = vec![b, a1, root, a];
        nodes.sort_by(|x, y| x.trace_cmp(*y).unwrap());
        assert_eq!(nodes, [root, a, a1, b]);

        b.detach();
        b.delete().unwrap();
        assert_eq!(a.trace_cmp(b), Err(UnknownNode(b.into()).into()));
    });
}

#[test]
fn nodes_convert_to_and_from_node_keys() {
    let mut ctx = caaqi::context::Context::new();
    scope(&mut ctx, || {
        let node = create_node(|_: TraceNode| {});
        let key = NodeKey::from(node);
        assert_eq!(TraceNode::from(key), node);
        assert!(TraceNode::from(key).exists());
    });
}

#[test]
fn nodes_are_partially_ordered_by_trace_order() {
    let mut ctx = caaqi::context::Context::new();
    scope(&mut ctx, || {
        let root = create_node(|_: TraceNode| {});
        let a = create_node(|_: TraceNode| {}).set_parent(root).unwrap();
        let a1 = create_node(|_: TraceNode| {}).set_parent(a).unwrap();
        let b = create_node(|_: TraceNode| {}).set_parent(root).unwrap();
        let other_tree = create_node(|_: TraceNode| {});

        assert!(root < a && a < a1 && a1 < b);
        assert!(b > a1);
        assert_eq!(a.partial_cmp(&a), Some(Ordering::Equal));
        assert_eq!(a.partial_cmp(&other_tree), None);
    });
}
