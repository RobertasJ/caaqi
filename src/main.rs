//! A scratch playground that exercises caaqi. It isn't part of the library's
//! API.
//!
//! A small app written with the prelude only: a root node mounts one child
//! per item into a shared output list. Changing an item and rerunning the
//! root replaces the old children with new ones.

use std::{cell::RefCell, rc::Rc};

use caaqi::prelude::*;

type List = Rc<RefCell<Vec<String>>>;

fn main() -> Result<(), RerunError> {
    let mut ctx = Context::new();
    let items: List = Rc::new(RefCell::new(vec!["a".into(), "b".into(), "c".into()]));
    let output: List = Rc::default();
    // Every node the app created, so it can count the live ones.
    let created: Rc<RefCell<Vec<NodeKey>>> = Rc::default();

    let root = ctx.root({
        let items = items.clone();
        let output = output.clone();
        let created = created.clone();
        move |ctx| {
            for item in items.borrow().clone() {
                let output = output.clone();
                let child = ctx.child(move |ctx| mount(ctx, &output, item.clone()));
                created.borrow_mut().push(child);
            }
        }
    });
    created.borrow_mut().push(root);
    report(&ctx, "run", &output, &created);

    items.borrow_mut()[1] = "B".into();
    ctx.rerun(root)?;
    report(&ctx, "rerun", &output, &created);

    Ok(())
}

/// Adds `item` to `output`, and removes it again when the node is rewound.
fn mount(ctx: &mut Context, output: &List, item: String) {
    output.borrow_mut().push(item.clone());
    let output = output.clone();
    ctx.on_rewind(move |_| {
        let mut output = output.borrow_mut();
        if let Some(index) = output.iter().position(|mounted| *mounted == item) {
            output.remove(index);
        }
    });
}

fn report(ctx: &Context, step: &str, output: &List, created: &Rc<RefCell<Vec<NodeKey>>>) {
    // Checking which nodes still exist is an inspection of the trace, which
    // only `caaqi::raw` offers; the app itself doesn't need it.
    use caaqi::raw::trace::TraceExt;

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
