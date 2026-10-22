use super::*;
use crate::{
    context::{Context, scope},
    trace::create_node,
};

fn child(parent: TraceNode) -> TraceNode {
    create_node(|_: TraceNode| {}).set_parent(parent).unwrap()
}

#[test]
fn walks_follow_caaqi_core() {
    let mut ctx = Context::new();
    scope(&mut ctx, || {
        let root = create_node(|_: TraceNode| {});
        let a = child(root);
        let a1 = child(a);
        let b = child(root);

        assert_eq!(ancestors(a1).unwrap().collect::<Vec<_>>(), [a, root]);
        assert_eq!(
            subtree_top_down(root).unwrap().collect::<Vec<_>>(),
            [root, a, a1, b]
        );
        assert_eq!(
            subtree_top_down_rev(root).unwrap().collect::<Vec<_>>(),
            [root, b, a, a1]
        );
        assert_eq!(
            subtree_bottom_up(root).unwrap().collect::<Vec<_>>(),
            [a1, a, b, root]
        );
        assert_eq!(
            subtree_bottom_up_rev(root).unwrap().collect::<Vec<_>>(),
            [b, a1, a, root]
        );
    });
}

#[test]
fn the_top_down_walk_can_change_the_trace_between_steps() {
    let mut ctx = Context::new();
    scope(&mut ctx, || {
        let root = create_node(|_: TraceNode| {});
        let a = child(root);
        let a1 = child(a);
        let b = child(root);

        let mut walk = subtree_top_down(root).unwrap();
        let mut seen = Vec::new();
        while let Some(node) = walk.next() {
            seen.push(node);
            if node == a {
                walk.skip_children();
                b.detach();
                b.delete().unwrap();
            }
        }
        assert_eq!(seen, [root, a]);
        assert!(a1.exists());
    });
}
