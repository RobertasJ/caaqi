//! One-pass layout: boxes as wide as what's in them, drawn top to bottom.
//!
//! A box's top border comes before its contents, but its width depends on
//! them. That usually takes two passes over the whole tree: measure
//! everything, then draw. Here the program draws once, top to bottom, like
//! immediate-mode UI code: each box reads its width as known so far, draws
//! its contents, measures them, and writes the width into the past, at its
//! own start, if it isn't what it drew with. Each box only knows about
//! itself, and nested boxes compose.
//!
//! The outer box writes into the past before the inner box's start, so its
//! write wipes the inner box's (rule 2). The inner box measures itself again
//! on the next run and writes again, and the machine settles once nobody
//! needs to.
//!
//! The program doesn't print, since it would print again on every run: it
//! returns its lines, and `main` prints them once the machine settles.
//!
//! Run with `just example layout`.

use caaqi::prelude::*;

#[derive(Clone, Copy)]
enum Align {
    Left,
    Center,
    Right,
}

impl Align {
    /// `text`, padded to `width` columns.
    fn pad(self, text: &str, width: usize) -> String {
        match self {
            Align::Left => format!("{text:<width$}"),
            Align::Center => format!("{text:^width$}"),
            Align::Right => format!("{text:>width$}"),
        }
    }
}

/// What goes in a box.
enum Item {
    /// A line of text.
    Text(Align, &'static str),
    /// A box of its own, as wide as what's in it.
    Boxed(Align, Vec<Item>),
}

/// Draws `items` in a box, and returns its lines.
fn draw_box(items: &[Item]) -> Vec<String> {
    let width = create_value(0);
    let start = now();

    // Each read comes after `start`, so it sees a width written there: on
    // the first run, there's none yet, and the box is drawn 0 wide.
    let mut lines = vec![format!("┌─{}─┐", "─".repeat(width.get()))];
    let mut widest = 0;
    for item in items {
        let (align, rows) = match item {
            Item::Text(align, text) => (*align, vec![text.to_string()]),
            Item::Boxed(align, items) => (*align, draw_box(items)),
        };
        for row in rows {
            widest = widest.max(row.chars().count());
            lines.push(format!("│ {} │", align.pad(&row, width.get())));
        }
    }
    lines.push(format!("└─{}─┘", "─".repeat(width.get())));

    // The box decides whether its past needs rewriting.
    if widest != width.get() {
        width.write_at(start, widest);
    }
    lines
}

fn main() {
    use Align::*;
    use Item::*;

    let dialog = vec![
        Text(Center, "Unsaved changes"),
        Text(Left, ""),
        Text(Left, "Your edits to notes.txt will be lost"),
        Text(Left, "if you close the window without saving."),
        Boxed(Right, vec![Text(Center, "Save   Discard   Cancel")]),
    ];
    let (mut machine, drawing) = new_machine(move || draw_box(&dialog));

    let runs = machine.run().unwrap();
    for line in scope(&mut machine, || drawing.get()) {
        println!("{line}");
    }
    println!("\ndrawn in one pass, settled after {runs} runs");
}
