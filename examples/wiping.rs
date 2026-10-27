//! An earlier write wipes later ones.
//!
//! A write into the past is kept on every rerun, until a write further back
//! wipes it (rule 2: a write into the past wipes everything after it). Then
//! it's gone, unless the code that made it runs again and makes it again.
//!
//! Both programs below first write `b` into the past, then `a` further back,
//! which wipes `b`'s write:
//!
//! - In the first, the code that writes `b` also checks `a`. Once `a` is 1,
//!   it doesn't write `b` again, so `b` ends up 0.
//! - In the second, it only checks `b`, so the next run writes `b` again,
//!   and `b` ends up 1.
//!
//! Run with `just example wiping`.

use caaqi::prelude::*;

fn main() {
    // `b` is written only while `a` is 0. Once `a` is written, `b`'s write is
    // wiped for good.
    let (mut machine, result) = new_machine(|| {
        let a = create_value(0);
        let after_creating_a = now();
        let b = create_value(0);
        let after_creating_b = now();

        if a.get() == 0 && b.get() == 0 {
            b.write_at(after_creating_b, 1); // Run 1 stops here.
        }
        if b.get() == 1 {
            a.write_at(after_creating_a, 1); // Run 2 stops here, wiping `b`'s write.
        }
        (a.get(), b.get()) // Run 3 gets here.
    });
    let runs = machine.run().unwrap();
    let (a, b) = scope(&mut machine, || result.get());
    println!("wiped for good: a = {a}, b = {b}, after {runs} runs");

    // `b` is written whenever it's 0, so after `a`'s write wipes it, the
    // next run makes it again.
    let (mut machine, result) = new_machine(|| {
        let a = create_value(0);
        let after_creating_a = now();
        let b = create_value(0);
        let after_creating_b = now();

        if b.get() == 0 {
            b.write_at(after_creating_b, 1); // Runs 1 and 3 stop here.
        }
        if a.get() == 0 {
            a.write_at(after_creating_a, 1); // Run 2 stops here, wiping `b`'s write.
        }
        (a.get(), b.get()) // Run 4 gets here.
    });
    let runs = machine.run().unwrap();
    let (a, b) = scope(&mut machine, || result.get());
    println!("made again:     a = {a}, b = {b}, after {runs} runs");
}
