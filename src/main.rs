use caaqi::{
    action::{ActionContext, finish},
    runner::run_action,
};

fn main() {
    run_action(|cx: &mut ActionContext| {
        println!("hello there");

        run_action(|cx: &mut ActionContext| {
            println!("hello again");

            cx.subscribe();

            finish()
        })?;

        finish()
    })
    .unwrap();
}
