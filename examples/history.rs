//! A program that rewrites its own past, then its history, inspected.
//!
//! A small shop opens with some stock, and customers buy from it. When the
//! stock runs short, the shop restocks at the opening, with a write into
//! the past: every sale after the opening saw the stock, so the whole day
//! plays out again with more stock, until nobody is turned away. Afterwards,
//! the stock's history shows the day as it finally happened.
//!
//! Run with `just example history`.

use caaqi::prelude::*;

fn main() {
    let (mut machine, day) = new_machine(|| {
        let stock = create_value(3);
        let opening = now();
        // Read after the opening, so it sees a restock written there.
        let opening_stock = stock.get();

        let mut sold = Vec::new();
        let mut short = 0;
        for (customer, amount) in [
            ("Ada", 2),
            ("Grace", 2),
            ("Alan", 1),
            ("Robertas", 11),
            ("Margaret", 7),
            ("Barbara", 4),
        ] {
            let left = stock.get();
            if left >= amount {
                stock.set(left - amount);
                sold.push(customer);
            } else {
                short += amount;
            }
        }

        // The program decides whether the past needs rewriting: only when
        // someone was turned away.
        if short > 0 {
            stock.write_at(opening, opening_stock + short);
        }
        (stock, opening, sold)
    });

    let runs = machine.run().unwrap();
    println!("the day settled after {runs} runs");

    scope(&mut machine, || {
        let (stock, opening, sold) = day.get();
        println!("sold to: {}", sold.join(", "));
        println!("stock at the opening: {}", stock.get_at(opening).unwrap());
        println!(
            "stock right after it: {}",
            stock.get_at(opening.next().unwrap()).unwrap()
        );
        println!("stock at closing: {}", stock.get());

        println!("\nthe stock's history:");
        for (version, entry) in stock.history().into_iter().enumerate() {
            println!("  version {version}: {}", entry.snapshot);
        }
    });
}
