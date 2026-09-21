use caaqi::{action::finish, runner::run_action};

fn main() {
    run_action(|| {
        println!("hello there");

        run_action(|| {
            println!("hello again");

            finish()
        })?;

        finish()
    })
    .unwrap();
}
