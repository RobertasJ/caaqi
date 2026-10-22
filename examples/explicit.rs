//! The same kind of run as `history`, with caaqi-core's explicit API: every
//! operation takes the `Machine`, and nothing is in scope.
//!
//! Run with `just example explicit`.

use caaqi_core::prelude::*;

fn main() {
    let mut machine = Machine::new();
    let total = machine.create_value(0);

    for amount in [5, 10, 20] {
        machine.run_step(|machine| {
            machine.update(total, |total| *total += amount).unwrap();
        });
    }

    let steps: Vec<StepId> = machine.steps().collect();
    for step in steps {
        let start = machine.step(step).unwrap().start();
        let seen = *machine.read_at(total, start).unwrap().unwrap();
        println!("{step:?} saw a total of {seen}");
    }
    println!("final total: {}", machine.read(total).unwrap());
}
