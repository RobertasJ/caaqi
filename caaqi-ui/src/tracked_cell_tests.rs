use caaqi_scoped::{
    context::{Context, get_or_default, scope},
    current::WithCurrent,
    trace::{TraceNode, create_node},
};

use super::*;
use crate::linear_executor::execute_queued;

/// The readers that ran, in the order they ran.
#[derive(Default)]
struct Runs(Vec<TraceNode>);

fn runs() -> Vec<TraceNode> {
    get_or_default(|runs: &mut Runs| runs.0.clone())
}

/// A node that reads `cell` each time it runs.
fn reader(cell: TrackedCell<i32>) -> TraceNode {
    let node = create_node(WithCurrent(move || {
        let _ = *cell.borrow();
        let node = current_node().unwrap();
        get_or_default(|runs: &mut Runs| runs.0.push(node));
    }));
    node.run().unwrap();
    node
}

#[test]
fn reads_and_writes_go_through_to_the_value() {
    let mut ctx = Context::new();
    scope(&mut ctx, || {
        let cell = store_tracked_cell(1);

        *cell.borrow_mut() += 1;
        assert_eq!(*cell.borrow(), 2);
        cell.remove().unwrap();
    });
}

#[test]
fn writing_reruns_readers() {
    let mut ctx = Context::new();
    scope(&mut ctx, || {
        let cell = store_tracked_cell(0);
        let node = reader(cell);

        *cell.borrow_mut() = 1;
        execute_queued();
        assert_eq!(runs(), [node, node]);

        // The rerun read the cell again, so it's still a reader.
        *cell.borrow_mut() = 2;
        execute_queued();
        assert_eq!(runs(), [node, node, node]);
    });
}

#[test]
fn reading_outside_a_node_isnt_tracked() {
    let mut ctx = Context::new();
    scope(&mut ctx, || {
        let cell = store_tracked_cell(0);

        let _ = *cell.borrow();
        assert_eq!(cell.readers.members().unwrap().count(), 0);
    });
}

#[test]
fn rewinding_a_reader_stops_tracking_it() {
    let mut ctx = Context::new();
    scope(&mut ctx, || {
        let cell = store_tracked_cell(0);
        let node = reader(cell);

        node.rewind();
        *cell.borrow_mut() = 1;
        execute_queued();
        assert_eq!(runs(), [node]);
    });
}

#[test]
fn removing_the_cell_removes_its_group() {
    let mut ctx = Context::new();
    scope(&mut ctx, || {
        let cell = store_tracked_cell(0);
        let node = reader(cell);

        cell.remove().unwrap();
        assert!(!cell.readers.exists());
        // The reader's rewind finds the group gone, and does nothing.
        node.rewind();

        assert!(matches!(
            cell.try_borrow(),
            Err(BorrowCellError::UnknownStored(_))
        ));
        assert!(matches!(
            cell.try_borrow_mut(),
            Err(BorrowCellMutError::UnknownStored(_))
        ));
        assert!(cell.remove().is_err());
    });
}
