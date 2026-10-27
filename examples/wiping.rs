//! Which writes into the past last.
//!
//! A write into the past lasts as long as the code that made it is
//! unaffected and the event it's attached to still happens. When an earlier
//! write affects the code that made it, that code is rewritten, and so is
//! its write: it's made again only if the code still makes it.
//!
//! Both programs below write `b` at a late point, then `a` at an earlier
//! one. In the first, the code that writes `b` reads `a`, so `a`'s write
//! affects it: it runs again, sees that `a` is 1, and doesn't write `b`. In
//! the second, it doesn't read `a`, so `b`'s write is unaffected and lasts.
//!
//! For now, the machine rewrites everything after `a`'s write, more than
//! what's affected, so in the second program `b`'s write is wiped too. The
//! unaffected code makes the same write again on the next run: the same
//! result, one run more.
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
