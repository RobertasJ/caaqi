use caaqi_core_scoped::{
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
        let _ = *cell.read();
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

        *cell.write() += 1;
        assert_eq!(*cell.read(), 2);
        cell.remove().unwrap();
    });
}

#[test]
fn writing_reruns_readers() {
    let mut ctx = Context::new();
    scope(&mut ctx, || {
        let cell = store_tracked_cell(0);
        let node = reader(cell);

        *cell.write() = 1;
        execute_queued();
        assert_eq!(runs(), [node, node]);

        // The rerun read the cell again, so it's still a reader.
        *cell.write() = 2;
        execute_queued();
        assert_eq!(runs(), [node, node, node]);
    });
}

#[test]
fn reading_outside_a_node_isnt_tracked() {
    let mut ctx = Context::new();
    scope(&mut ctx, || {
        let cell = store_tracked_cell(0);

        let _ = *cell.read();
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
        *cell.write() = 1;
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
            cell.try_read(),
            Err(BorrowCellError::UnknownStored(_))
        ));
        assert!(matches!(
            cell.try_write(),
            Err(BorrowCellMutError::UnknownStored(_))
        ));
        assert!(cell.remove().is_err());
    });
}

/// A node that runs `f` each time it runs, recording the run.
fn node_running(f: impl Fn() + 'static) -> TraceNode {
    let node = create_node(WithCurrent(move || {
        f();
        let node = current_node().unwrap();
        get_or_default(|runs: &mut Runs| runs.0.push(node));
    }));
    node.run().unwrap();
    node
}

#[test]
fn silent_reads_dont_subscribe() {
    let mut ctx = Context::new();
    scope(&mut ctx, || {
        let cell = store_tracked_cell(0);
        let node = node_running(move || {
            let _ = *cell.silent_read();
        });

        *cell.write() = 1;
        execute_queued();
        assert_eq!(runs(), [node]);
    });
}

#[test]
fn silent_writes_wait_for_notify() {
    let mut ctx = Context::new();
    scope(&mut ctx, || {
        let cell = store_tracked_cell(0);
        let node = reader(cell);

        *cell.silent_write() = 1;
        execute_queued();
        assert_eq!(runs(), [node]);

        cell.notify().unwrap();
        execute_queued();
        assert_eq!(runs(), [node, node]);
    });
}

#[test]
fn subscribing_without_reading_reruns_on_write() {
    let mut ctx = Context::new();
    scope(&mut ctx, || {
        let cell = store_tracked_cell(0);
        let node = node_running(move || cell.subscribe().unwrap());

        *cell.write() = 1;
        execute_queued();
        assert_eq!(runs(), [node, node]);
    });
}

#[test]
fn subscribing_outside_a_node_does_nothing() {
    let mut ctx = Context::new();
    scope(&mut ctx, || {
        let cell = store_tracked_cell(0);

        cell.subscribe().unwrap();
        assert_eq!(cell.readers.members().unwrap().count(), 0);
    });
}

#[test]
fn a_removed_cell_cant_subscribe_or_notify() {
    let mut ctx = Context::new();
    scope(&mut ctx, || {
        let cell = store_tracked_cell(0);
        cell.remove().unwrap();

        assert!(cell.subscribe().is_err());
        assert!(cell.notify().is_err());
    });
}

#[test]
fn set_replaces_the_value_and_notifies() {
    let mut ctx = Context::new();
    scope(&mut ctx, || {
        let cell = store_tracked_cell(0);
        let node = reader(cell);

        cell.set(0);
        execute_queued();
        assert_eq!(*cell.read(), 0);
        assert_eq!(runs(), [node, node]);
    });
}

#[test]
fn set_if_changed_notifies_only_on_a_change() {
    let mut ctx = Context::new();
    scope(&mut ctx, || {
        let cell = store_tracked_cell(0);
        let node = reader(cell);

        assert!(!cell.set_if_changed(0));
        execute_queued();
        assert_eq!(runs(), [node]);

        assert!(cell.set_if_changed(1));
        execute_queued();
        assert_eq!(*cell.read(), 1);
        assert_eq!(runs(), [node, node]);
    });
}

#[test]
fn a_failed_set_changes_nothing() {
    let mut ctx = Context::new();
    scope(&mut ctx, || {
        let cell = store_tracked_cell(0);
        let node = reader(cell);

        let held = cell.silent_read();
        assert!(matches!(
            cell.try_set(1),
            Err(BorrowCellMutError::BorrowMut(_))
        ));
        assert!(matches!(
            cell.try_set_if_changed(1),
            Err(BorrowCellMutError::BorrowMut(_))
        ));
        drop(held);

        execute_queued();
        assert_eq!(*cell.read(), 0);
        assert_eq!(runs(), [node]);
    });
}
