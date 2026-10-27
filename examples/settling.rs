//! Settling: the program decides when to stop writing into the past.
//!
//! A run settles when it makes no write into the past. Whether a program
//! settles is up to the program: guess, check, correct. It checks what's
//! already there before it writes, and a program that writes into the past
//! on every run never settles: `run` never returns.
//!
//! Here each run counts one higher at the same point, until it reaches 5. A
//! newer write at a point replaces the older one there, so the history ends
//! up with the creation and the last write only.
//!
//! Run with `just example settling`.

use caaqi::prelude::*;

fn main() {
    let (mut machine, result) = new_machine(|| {
        let count = create_value(0);
        let after_creation = now();
        let seen = count.get();
        if seen < 5 {
            count.write_at(after_creation, seen + 1);
        }
        // Without the check above, this would never settle.
        (count, seen)
    });

    let runs = machine.run().unwrap();
    scope(&mut machine, || {
        let (count, seen) = result.get();
        println!("settled after {runs} runs, seeing {seen}");
        let history: Vec<_> = count
            .history()
            .into_iter()
            .map(|entry| entry.snapshot)
            .collect();
        println!("the count's history: {history:?}");
    });
}
