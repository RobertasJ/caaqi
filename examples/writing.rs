//! Writing into the past.
//!
//! A read sees the present: the newest write at or before its own point.
//! `write_at(point, contents)` writes right after an earlier point, and
//! rewrites everything after it that it affects: whatever observed what the
//! write changed, then whatever observed those changes, and so on.
//!
//! For now, the machine rewrites more than that, which costs runs but gives
//! the same result: the run stops at the write, and the program runs again
//! from the start, with the write in place.
//!
//! The write goes right after the point, so a read after the point sees it,
//! and a read at the point itself doesn't: looking at the past is explicit.
//!
//! Run with `just example writing`.

use caaqi::prelude::*;

fn main() {
    let (mut machine, result) = new_machine(|| {
        let greeting = create_value("hello");
        let after_creation = now();

        let seen = greeting.get(); // After `after_creation`: sees a write made there.
        let at_point = greeting.get_at(after_creation); // At `after_creation`: doesn't.

        if seen == "hello" {
            // The first run stops here. The second sees the new greeting, so
            // it doesn't write again, and runs to the end.
            greeting.write_at(after_creation, "hello from the future");
        }
        (seen, at_point)
    });

    let runs = machine.run().unwrap();
    let (seen, at_point) = scope(&mut machine, || result.get());
    println!("settled after {runs} runs");
    println!("read after `after_creation`: {seen:?}");
    println!("read at `after_creation`:    {at_point:?}");
}
