//! A box model whose every value is a tracked cell: the position, the
//! content box's size, and the padding and margin on each side.
//!
//! A parent box at `(0, 0)` holds two children side by side: the first at
//! the start of the parent's content box, the second right after the
//! first's margin box. The parent's content box is sized to fit both. Each
//! of these layout steps is a node, and so is the report of each box:
//! changing a value reruns only the nodes that read it, and a step whose
//! result didn't change stops there.
//!
//! Run with `cargo run --example layout`.

use caaqi_scoped::{
    current::current_node, prelude::*, rewind::register_rewind, trace::create_node,
};
use caaqi_ui::{
    linear_executor::{NodeExecutionExt, execute_queued},
    tracked_cell::{TrackedCell, store_tracked_cell},
};

/// A value for each side of a box.
#[derive(Clone, Copy)]
struct Sides {
    top: TrackedCell<f64>,
    right: TrackedCell<f64>,
    bottom: TrackedCell<f64>,
    left: TrackedCell<f64>,
}

impl Sides {
    fn new(top: f64, right: f64, bottom: f64, left: f64) -> Self {
        Sides {
            top: store_tracked_cell(top),
            right: store_tracked_cell(right),
            bottom: store_tracked_cell(bottom),
            left: store_tracked_cell(left),
        }
    }

    fn uniform(value: f64) -> Self {
        Self::new(value, value, value, value)
    }

    /// Left plus right.
    fn horizontal(self) -> f64 {
        *self.left.read() + *self.right.read()
    }

    /// Top plus bottom.
    fn vertical(self) -> f64 {
        *self.top.read() + *self.bottom.read()
    }
}

/// A content box, with padding around it and margin around that, placed
/// by its margin box's top-left corner.
#[derive(Clone, Copy)]
struct BoxModel {
    x: TrackedCell<f64>,
    y: TrackedCell<f64>,
    width: TrackedCell<f64>,
    height: TrackedCell<f64>,
    padding: Sides,
    margin: Sides,
}

impl BoxModel {
    /// A box at `(0, 0)`.
    fn new(width: f64, height: f64, padding: Sides, margin: Sides) -> Self {
        BoxModel {
            x: store_tracked_cell(0.0),
            y: store_tracked_cell(0.0),
            width: store_tracked_cell(width),
            height: store_tracked_cell(height),
            padding,
            margin,
        }
    }

    /// The margin box's top-left corner.
    fn position(self) -> (f64, f64) {
        (*self.x.read(), *self.y.read())
    }

    /// The content box's top-left corner: the position, inside the margin
    /// and the padding.
    fn content_origin(self) -> (f64, f64) {
        let (x, y) = self.position();
        (
            x + *self.margin.left.read() + *self.padding.left.read(),
            y + *self.margin.top.read() + *self.padding.top.read(),
        )
    }

    /// The content box's size.
    fn content_box(self) -> (f64, f64) {
        (*self.width.read(), *self.height.read())
    }

    /// The content box's size plus the padding.
    fn padding_box(self) -> (f64, f64) {
        let (width, height) = self.content_box();
        (
            width + self.padding.horizontal(),
            height + self.padding.vertical(),
        )
    }

    /// The padding box's size plus the margin.
    fn margin_box(self) -> (f64, f64) {
        let (width, height) = self.padding_box();
        (
            width + self.margin.horizontal(),
            height + self.margin.vertical(),
        )
    }
}

/// Sizes `parent`'s content box to fit `first` and `second` side by side:
/// as wide as both margin boxes, and as tall as the taller one.
fn size_to_fit(parent: BoxModel, first: BoxModel, second: BoxModel) {
    let (first_width, first_height) = first.margin_box();
    let (second_width, second_height) = second.margin_box();
    parent.width.set_if_changed(first_width + second_width);
    parent
        .height
        .set_if_changed(first_height.max(second_height));
}

/// Places `child` at the start of `parent`'s content box.
fn place_first(parent: BoxModel, child: BoxModel) {
    let (x, y) = parent.content_origin();
    child.x.set_if_changed(x);
    child.y.set_if_changed(y);
}

/// Places `next` right after `previous`'s margin box, at the same height.
fn place_after(previous: BoxModel, next: BoxModel) {
    let (x, y) = previous.position();
    let (width, _) = previous.margin_box();
    next.x.set_if_changed(x + width);
    next.y.set_if_changed(y);
}

fn report(name: &str, model: BoxModel) {
    let (x, y) = model.position();
    let (width, height) = model.margin_box();
    println!("  {name}: at ({x}, {y}), {width} x {height}");
}

fn main() {
    let mut ctx = Context::new();
    scope(&mut ctx, || {
        let parent = BoxModel::new(0.0, 0.0, Sides::uniform(10.0), Sides::uniform(0.0));
        let first = BoxModel::new(100.0, 50.0, Sides::uniform(10.0), Sides::uniform(5.0));
        let second = BoxModel::new(
            80.0,
            30.0,
            Sides::uniform(5.0),
            Sides::new(5.0, 20.0, 5.0, 20.0),
        );

        // The executor sorts queued nodes by trace order, which only exists
        // inside one tree, so every node is a child of the same root.
        println!("first run:");
        run(move || {
            // Each layout node comes before the nodes that read what it
            // writes, so a change reruns each of them at most once.
            run(move || size_to_fit(parent, first, second));
            run(move || place_first(parent, first));
            run(move || place_after(first, second));

            run(move || report("parent", parent));
            run(move || report("first", first));
            run(move || report("second", second));
        });

        // `second` moves right, and `parent` widens to fit.
        println!("first.width = 150:");
        first.width.set(150.0);
        execute_queued();

        // Both children move down, and `parent` grows taller.
        println!("parent.padding.top = 20:");
        parent.padding.top.set(20.0);
        execute_queued();

        // `second` is taller than `first` now, and `parent` follows it.
        println!("second.height = 100:");
        second.height.set(100.0);
        execute_queued();

        // `first` grows, but `second` is still the taller one: `parent`'s
        // size is set to what it already is, which notifies nothing.
        println!("first.height = 60:");
        first.height.set(60.0);
        execute_queued();

        // Everything moves with `parent`.
        println!("parent.x = 50:");
        parent.x.set(50.0);
        execute_queued();
    });
}

/// Creates a node running `f` and runs it. Called from a running node, it
/// makes a child of that node, which is rewound and deleted when the parent
/// is rewound. Called from outside of any node, it makes a root.
fn run(f: impl FnMut() + 'static) -> TraceNode {
    let parent = current_node();
    let node = create_node(WithCurrent(f));

    if let Some(parent) = parent {
        node.set_parent(parent)
            .expect("a new node can be attached to any node");
        register_rewind(parent, move |_: RewindKey| {
            // The node may have been deleted since.
            if node.exists() {
                node.rewind();
                node.unqueue_execution();
                node.delete()
                    .expect("rewinding the node deleted its children, and it isn't running");
            }
        })
        .expect("the parent is running, so it's in the trace");
    }

    node.run().expect("a new node isn't running");
    node
}
