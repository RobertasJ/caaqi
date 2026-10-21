use super::*;
use crate::{
    context::{Context, scope},
    rewind::{register_rewind, run_rewind},
    trace::create_node,
};

#[test]
fn nothing_is_current_outside_of_runs() {
    let mut ctx = Context::new();
    scope(&mut ctx, || {
        assert_eq!(current_node(), None);
        assert_eq!(current_rewind(), None);
    });
}

#[test]
fn with_current_rewinds_see_their_key_and_their_node() {
    let mut ctx = Context::new();
    scope(&mut ctx, || {
        let node = create_node(WithCurrent(|| {
            let node = current_node().unwrap();
            let rewind = register_rewind(
                node,
                WithCurrent(move || {
                    assert!(current_rewind().is_some());
                    assert_eq!(current_node(), Some(node));
                }),
            )
            .unwrap();
            assert!(run_rewind(rewind));
            // Registered again, for the rewind below, outside the node.
            register_rewind(
                node,
                WithCurrent(|| {
                    assert!(current_rewind().is_some());
                    assert_eq!(current_node(), None);
                }),
            )
            .unwrap();
        }));

        node.run().unwrap();
        node.rewind();
    });
}
