use caaqi::{
    current::WithCurrent,
    rewind::NodeRewindExt,
    trace::{NodeKey, TraceExt},
};

use super::*;

/// How many times each reader ran, in the order they ran.
#[derive(Default)]
struct Runs(Vec<NodeKey>);

fn runs(ctx: &Context) -> &[NodeKey] {
    ctx.get::<Runs>().map_or(&[], |runs| &runs.0)
}

/// A node that reads `cell` each time it runs.
fn reader(ctx: &mut Context, cell: TrackedCell<i32>) -> NodeKey {
    let node = ctx
        .create_node(WithCurrent(move |ctx: &mut Context| {
            let _ = *ctx.borrow_tracked_cell(cell);
            let node = ctx.current_node().unwrap();
            ctx.get_or_default::<Runs>().0.push(node);
        }))
        .id();
    ctx.node_mut(node).unwrap().run().unwrap();
    node
}

#[test]
fn reads_and_writes_go_through_to_the_value() {
    let mut ctx = Context::new();
    let cell = ctx.store_tracked_cell(1);

    *ctx.borrow_tracked_cell_mut(cell) += 1;
    assert_eq!(*ctx.borrow_tracked_cell(cell), 2);
    assert_eq!(ctx.remove_tracked_cell(cell).unwrap(), 2);
}

#[test]
fn writing_reruns_readers() {
    let mut ctx = Context::new();
    let cell = ctx.store_tracked_cell(0);
    let node = reader(&mut ctx, cell);

    *ctx.borrow_tracked_cell_mut(cell) = 1;
    ctx.execute_queued();
    assert_eq!(runs(&ctx), [node, node]);

    // The rerun read the cell again, so it's still a reader.
    *ctx.borrow_tracked_cell_mut(cell) = 2;
    ctx.execute_queued();
    assert_eq!(runs(&ctx), [node, node, node]);
}

#[test]
fn reading_outside_a_node_isnt_tracked() {
    let mut ctx = Context::new();
    let cell = ctx.store_tracked_cell(0);

    let _ = *ctx.borrow_tracked_cell(cell);
    assert_eq!(ctx.group_members(cell.readers).unwrap().count(), 0);
}

#[test]
fn rewinding_a_reader_stops_tracking_it() {
    let mut ctx = Context::new();
    let cell = ctx.store_tracked_cell(0);
    let node = reader(&mut ctx, cell);

    ctx.node_mut(node).unwrap().rewind();
    *ctx.borrow_tracked_cell_mut(cell) = 1;
    ctx.execute_queued();
    assert_eq!(runs(&ctx), [node]);
}

#[test]
fn removing_the_cell_removes_its_group() {
    let mut ctx = Context::new();
    let cell = ctx.store_tracked_cell(0);
    let node = reader(&mut ctx, cell);

    assert_eq!(ctx.remove_tracked_cell(cell).unwrap(), 0);
    assert!(!ctx.group_exists(cell.readers));
    // The reader's rewind finds the group gone, and does nothing.
    ctx.node_mut(node).unwrap().rewind();

    assert!(matches!(
        ctx.try_borrow_tracked_cell(cell),
        Err(BorrowCellError::UnknownStored(_))
    ));
    assert!(matches!(
        ctx.try_borrow_tracked_cell_mut(cell),
        Err(BorrowCellMutError::UnknownStored(_))
    ));
    assert!(ctx.remove_tracked_cell(cell).is_err());
}
