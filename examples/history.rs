//! A run recorded by caaqi, then inspected.
//!
//! A small shop runs as steps: each order reads the stock and writes it
//! back. Which steps run depends on the stock (a decision), so a restock
//! step appears inside the order that ran out. Afterwards, the stock can be
//! read as each step saw it, and each step lists what it read and wrote.
//!
//! Run with `just example history`.

use caaqi::prelude::*;

fn main() {
    let mut machine = Machine::new();
    scope(&mut machine, || {
        let stock = create_value(3);

        for (customer, amount) in [("Ada", 2), ("Grace", 2), ("Alan", 1)] {
            run_step(|| {
                if stock.get() < amount {
                    // Steps run here are inside this order's section.
                    run_step(|| stock.update(|stock| *stock += 5));
                }
                stock.update(|stock| *stock -= amount);
                println!("{customer} bought {amount}, {} left", stock.get());
            });
        }

        println!("\nthe run, step by step:");
        for (number, step) in steps().into_iter().enumerate() {
            let depth = std::iter::successors(step.parent(), |parent| parent.parent()).count();
            let seen = stock.get_at(step.start()).unwrap();
            let left = stock.get_at(step.end().unwrap()).unwrap();
            println!(
                "{:indent$}step {number}: saw {seen}, left {left}, read {:?}, wrote {:?}",
                "",
                versions(stock, step.reads()),
                versions(stock, step.writes()),
                indent = depth * 2,
            );
        }

        println!("\nthe stock's history:");
        for (version, entry) in stock.history().into_iter().enumerate() {
            println!("  version {version}: {}", entry.snapshot);
        }
    });
}

/// The versions of `stock` among the accesses. The others are the results
/// of steps, which are values too.
fn versions(stock: Value<i32>, accesses: Vec<Access>) -> Vec<u64> {
    accesses
        .iter()
        .filter(|access| access.value == stock.id())
        .map(|access| access.version)
        .collect()
}
