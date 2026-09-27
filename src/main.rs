use caaqi::prelude::*;

fn main() -> Result<(), SetParentError> {
    let mut ctx = Context::new();

    let root = ctx.create_node().id();
    let a = ctx.create_node().id();
    let a1 = ctx.create_node().id();
    let b = ctx.create_node().id();

    ctx.node_mut(a).unwrap().set_parent(root)?;
    ctx.node_mut(a1).unwrap().set_parent(a)?;
    ctx.node_mut(b).unwrap().set_parent(root)?;

    let names = [(root, "root"), (a, "a"), (a1, "a1"), (b, "b")];
    let name = |key| names.iter().find(|&&(k, _)| k == key).unwrap().1;

    for key in ctx.subtree_top_down(root).unwrap() {
        let depth = ctx.ancestors(key).unwrap().count();
        println!("{}{}", "  ".repeat(depth), name(key));
    }

    Ok(())
}
