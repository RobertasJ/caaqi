use caaqi::prelude::*;

#[derive(Default)]
struct Counter(u32);

fn main() {
    let mut ctx = Context::new();
    ctx.insert(Counter::default());
    let group = ctx.create_group();

    let (root, ()) = ctx.run_node(|ctx: &mut Context| {
        println!("hello there");
        ctx.get_mut::<Counter>().unwrap().0 += 1;

        let (child, ()) = ctx.run_node(|ctx: &mut Context| {
            println!("hello again");
            ctx.get_mut::<Counter>().unwrap().0 += 1;
        });
        ctx.add_to_group(group, child).unwrap();
    });
    println!("counter: {}", ctx.get::<Counter>().unwrap().0);

    let members = |ctx: &Context| ctx.get::<Groups>().unwrap().members(group).unwrap().count();
    println!("group members: {}", members(&ctx));
    ctx.clear_children(root).unwrap();
    println!("group members after clearing: {}", members(&ctx));
}
