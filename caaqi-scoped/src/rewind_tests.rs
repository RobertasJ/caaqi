use super::*;
use crate::{
    context::{Context, get, get_or_default, scope},
    trace::create_node,
};

#[derive(Default)]
struct Log(Vec<&'static str>);

#[test]
fn rewinds_run_with_the_context_in_scope() {
    let mut ctx = Context::new();
    scope(&mut ctx, || {
        let node = create_node(|_: TraceNode| {});
        let key = register_rewind(node, |key: RewindKey| {
            assert!(rewind_node_of(key).is_some());
            get_or_default(|log: &mut Log| log.0.push("rewound"));
        })
        .unwrap();

        assert_eq!(rewind_keys(node).unwrap().collect::<Vec<_>>(), [key]);
        node.rewind();
        assert_eq!(get(|log: &Log| log.0.clone()), Some(vec!["rewound"]));
        assert_eq!(rewind_keys(node).unwrap().count(), 0);
        assert!(!run_rewind(key));
        assert_eq!(rewind_node_of(key), None);
    });
}
