use caaqi::{
    action::{ActionContext, ActionRelationship, finish},
    runner::run_action,
};

fn main() {
    let mut action_context = ActionContext::new();
    run_action(&mut action_context, |ctx: &mut ActionContext| {
        println!("hello there");

        run_action(ctx, |ctx: &mut ActionContext| {
            println!("hello again");

            finish()
        })?;

        finish()
    })
    .unwrap();
}
