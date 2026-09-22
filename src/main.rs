use caaqi::{
    action::{ActionState, ActiveActionState, finish},
    runner::run_action,
};

fn main() {
    run_action(
        &mut ActiveActionState::new(),
        |s: &mut ActiveActionState| {
            println!("hello there");

            run_action(s, |s: &mut ActiveActionState| {
                println!("hello again");

                finish()
            })?;

            finish()
        },
    )
    .unwrap();
}
