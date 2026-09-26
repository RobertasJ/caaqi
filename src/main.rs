use std::{cell::Cell, rc::Rc};

use caaqi::prelude::*;

fn main() {
    let mut ctx = Context::new();

    ctx.run_tracked(|ctx: &mut Context| {
        let count_tracking = ctx.create_tracking_id();
        let count = Rc::new(Cell::new(0));

        let doubled_tracking = ctx.create_tracking_id();
        let doubled = Rc::new(Cell::new(count.get() * 2));

        ctx.run_tracked({
            let count = Rc::clone(&count);
            let doubled = Rc::clone(&doubled);
            move |ctx: &mut Context| {
                ctx.track(count_tracking);

                doubled.set(count.get() * 2);
                ctx.notify(doubled_tracking);
            }
        });

        ctx.run_tracked({
            let count = Rc::clone(&count);
            let doubled = Rc::clone(&doubled);
            move |ctx: &mut Context| {
                ctx.track(count_tracking);
                ctx.track(doubled_tracking);

                println!("count: {}", count.get());
                println!("count * 2: {}", doubled.get());
            }
        });

        let complete = Rc::new(Cell::new(false));
        let complete_tracking = ctx.create_tracking_id();

        ctx.run_tracked({
            let count = Rc::clone(&count);
            let complete = Rc::clone(&complete);
            move |ctx: &mut Context| {
                ctx.track(count_tracking);

                if count.get() < 10 {
                    count.set(count.get() + 1);
                    ctx.notify(count_tracking);
                } else {
                    complete.set(true);
                    ctx.notify(complete_tracking);
                }
            }
        });

        ctx.run_tracked({
            let complete = Rc::clone(&complete);
            let count = Rc::clone(&count);
            let doubled = Rc::clone(&doubled);
            move |ctx: &mut Context| {
                ctx.track(complete_tracking);

                if complete.get() {
                    println!("final count: {}", count.get());
                    println!("final count * 2: {}", doubled.get());
                }
            }
        });
    });
}
