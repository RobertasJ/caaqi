//! Reading the past.
//!
//! Every write stays in the value's history, so a value can be read as it
//! was at any earlier point: `get` reads it now, `get_at` reads it at a
//! point, and `now` gives you a point to come back to. Rule 1: a read sees
//! the newest write at or before it.
//!
//! Run with `just example reading`.

use caaqi::prelude::*;

fn main() {
    let (mut machine, result) = new_machine(|| {
        let count = create_value(1);
        let when_count_was_1 = now(); // A point to come back to.
        count.set(2);
        let when_count_was_2 = now();
        count.set(3);

        // The past can be read during the run, too.
        let was = count.get_at(when_count_was_1);
        (count, when_count_was_1, when_count_was_2, was)
    });
    machine.run().unwrap();

    // Outside the run, `scope` puts the machine back in reach to read it.
    scope(&mut machine, || {
        let (count, when_count_was_1, when_count_was_2, was) = result.get();
        println!("count now:                   {}", count.get());
        println!(
            "count at `when_count_was_1`: {:?}",
            count.get_at(when_count_was_1)
        );
        println!(
            "count at `when_count_was_2`: {:?}",
            count.get_at(when_count_was_2)
        );
        println!("read during the run:         {was:?}");

        println!("\nits history, one entry per write:");
        for (version, entry) in count.history().into_iter().enumerate() {
            println!("  version {version}: {}", entry.snapshot);
        }
    });
}
