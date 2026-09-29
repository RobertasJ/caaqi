//! A scratch playground that exercises caaqi. It isn't part of the library's
//! API.
//!
//! A small app written with the prelude only: a root node mounts one child
//! per item into a shared output list. Changing an item, rewinding the root
//! and running it again replaces the old children with new ones.

use std::{cell::RefCell, rc::Rc};

use caaqi::prelude::*;

type List = Rc<RefCell<Vec<String>>>;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut ctx = Context::new();
    let items: List = Rc::new(RefCell::new(vec!["a".into(), "b".into(), "c".into()]));
    let output: List = Rc::default();
    // Every node the app created, so it can count the live ones.
    let created: Rc<RefCell<Vec<NodeKey>>> = Rc::default();

    let root = ctx
        .create_node(WithCurrent({
            let items = items.clone();
            let output = output.clone();
            let created = created.clone();
            move |ctx: &mut Context| {
                for item in items.borrow().clone() {
                    let output = output.clone();
                    let child = child(ctx, move |ctx| mount(ctx, &output, item.clone()));
                    created.borrow_mut().push(child);
                }
            }
        }))
        .id();
    created.borrow_mut().push(root);
    ctx.node_mut(root)?.run()?;
    report(&ctx, "run", &output, &created);

    items.borrow_mut()[1] = "B".into();
    ctx.rewind(root)?;
    ctx.node_mut(root)?.run()?;
    report(&ctx, "rerun", &output, &created);

    Ok(())
}

/// Creates a child of the current node running `f`, runs it, and deletes it
/// again when the current node is rewound.
fn child(ctx: &mut Context, f: impl FnMut(&mut Context) + 'static) -> NodeKey {
    let parent = ctx
        .current_node()
        .expect("children are made by running nodes");
    let child = ctx.create_node(WithCurrent(f)).id();
    ctx.node_mut(child)
        .unwrap()
        .set_parent(parent)
        .expect("the child was just created");
    ctx.register_rewind(
        parent,
        WithCurrent(move |ctx: &mut Context| {
            ctx.rewind(child).unwrap();
            ctx.node_mut(child).unwrap().delete().unwrap();
        }),
    )
    .unwrap();
    ctx.node_mut(child).unwrap().run().unwrap();
    child
}

/// Adds `item` to `output`, and removes it again when the node is rewound.
fn mount(ctx: &mut Context, output: &List, item: String) {
    output.borrow_mut().push(item.clone());
    let output = output.clone();
    let node = ctx.current_node().expect("mounted from a running node");
    on_rewind(ctx, node, move |_| {
        let mut output = output.borrow_mut();
        if let Some(index) = output.iter().position(|mounted| *mounted == item) {
            output.remove(index);
        }
    });
}

fn on_rewind(ctx: &mut Context, node: NodeKey, f: impl FnOnce(&mut Context) + 'static) {
    ctx.register_rewind(node, WithCurrent(f)).unwrap();
}

fn report(ctx: &Context, step: &str, output: &List, created: &Rc<RefCell<Vec<NodeKey>>>) {
    let created = created.borrow();
    let live = created
        .iter()
        .filter(|&&node| ctx.contains_node(node))
        .count();
    println!(
        "after {step}: output {:?}, {live} live nodes of {} created",
        output.borrow(),
        created.len(),
    );
}
