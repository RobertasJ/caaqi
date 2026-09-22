use caaqi::action::{ActionContext, finish};

#[derive(Default)]
struct Counter(u32);

fn main() {
    let mut ctx = ActionContext::new();
    ctx.insert(Counter::default());
    ctx.run(|ctx: &mut ActionContext| {
        println!("hello there");
        ctx.get_mut::<Counter>().unwrap().0 += 1;

        ctx.run(|ctx: &mut ActionContext| {
            println!("hello again");
            ctx.get_mut::<Counter>().unwrap().0 += 1;
            finish()
        })?;
        finish()
    })
    .unwrap();
    println!("counter: {}", ctx.get::<Counter>().unwrap().0);
}
