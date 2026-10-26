//! A layout with caaqi-core's explicit API: every operation takes the
//! `Machine`, and nothing is in scope.
//!
//! A parent centers a label in a 200-wide box, which needs the label's
//! width, and the label (the child) is only measured afterwards. The child
//! writes its width into the past, at the parent's point, and the parent
//! sees it on the next run.
//!
//! Run with `just example explicit`.

use caaqi_core::prelude::*;

fn main() {
    let (mut machine, layout) = Machine::new(|machine| {
        let child_width = machine.create_value::<Option<f32>>(None).unwrap();

        // The parent.
        let parent = machine.now().unwrap();
        let position = match *machine.read(child_width).unwrap() {
            Some(width) => (200.0 - width) / 2.0,
            None => 0.0,
        };

        // The child.
        let measured = 50.0;
        if *machine.read(child_width).unwrap() != Some(measured) {
            machine
                .write_at(child_width, parent, Some(measured))
                .unwrap();
        }

        (position, child_width, parent)
    });

    let runs = machine.run().unwrap();
    let (position, child_width, parent) = *machine.read(layout).unwrap();
    println!("settled after {runs} runs: the label is at x = {position}");

    let at_parent = *machine.read_at(child_width, parent).unwrap().unwrap();
    let written = machine.timeline().next(parent).unwrap().unwrap();
    let after = *machine.read_at(child_width, written).unwrap().unwrap();
    println!("the child's width at the parent's point: {at_parent:?}, right after it: {after:?}");
}
