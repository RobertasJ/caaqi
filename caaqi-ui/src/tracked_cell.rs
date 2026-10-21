//! Cells that remember which nodes read them, and queue those nodes when
//! they're written.
//!
//! A read from inside a running node (one wrapped in
//! [`WithCurrent`](caaqi::current::WithCurrent)) adds the node to the cell's
//! group, until the node is rewound. A write queues every node in the group
//! with [`queue_execution`](LinearExecutorExt::queue_execution).

use std::{
    cell::{Ref, RefMut},
    fmt, hash,
};

use caaqi::{
    context::Context,
    current::CurrentExt,
    group::{GroupId, GroupingExt},
    rewind::RewindExt,
    storage::UnknownStored,
};

use crate::{
    cell::{BorrowCellError, BorrowCellMutError, CellExt, StoredCell},
    linear_executor::LinearExecutorExt,
};

/// A handle to a cell whose readers are tracked. `Copy` whatever `T` is.
pub struct TrackedCell<T: 'static> {
    cell: StoredCell<T>,
    /// The nodes that read the cell. It lives as long as the cell.
    readers: GroupId,
}

// Implemented by hand: deriving would require `T` to implement each trait.
impl<T: 'static> Clone for TrackedCell<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: 'static> Copy for TrackedCell<T> {}

impl<T: 'static> PartialEq for TrackedCell<T> {
    fn eq(&self, other: &Self) -> bool {
        self.cell == other.cell
    }
}

impl<T: 'static> Eq for TrackedCell<T> {}

impl<T: 'static> hash::Hash for TrackedCell<T> {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        self.cell.hash(state);
    }
}

impl<T: 'static> fmt::Debug for TrackedCell<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TrackedCell")
            .field("cell", &self.cell)
            .field("readers", &self.readers)
            .finish()
    }
}

pub trait TrackedCellExt {
    fn store_tracked_cell<T: 'static>(&mut self, value: T) -> TrackedCell<T>;

    /// Like [`try_borrow_tracked_cell`](TrackedCellExt::try_borrow_tracked_cell),
    /// but panics on error.
    fn borrow_tracked_cell<T: 'static>(&mut self, cell: TrackedCell<T>) -> Ref<'_, T>;

    /// Borrows the value. Inside a running node, also adds the node to the
    /// cell's readers until the node is rewound.
    fn try_borrow_tracked_cell<T: 'static>(
        &mut self,
        cell: TrackedCell<T>,
    ) -> Result<Ref<'_, T>, BorrowCellError>;

    /// Like [`try_borrow_tracked_cell_mut`](TrackedCellExt::try_borrow_tracked_cell_mut),
    /// but panics on error.
    fn borrow_tracked_cell_mut<T: 'static>(&mut self, cell: TrackedCell<T>) -> RefMut<'_, T>;

    /// Borrows the value mutably, and queues the cell's readers for
    /// execution.
    fn try_borrow_tracked_cell_mut<T: 'static>(
        &mut self,
        cell: TrackedCell<T>,
    ) -> Result<RefMut<'_, T>, BorrowCellMutError>;

    /// Removes the cell and its group of readers, and returns the value.
    fn remove_tracked_cell<T: 'static>(&mut self, cell: TrackedCell<T>)
    -> Result<T, UnknownStored>;
}

impl TrackedCellExt for Context {
    fn store_tracked_cell<T: 'static>(&mut self, value: T) -> TrackedCell<T> {
        TrackedCell {
            cell: self.store_cell(value),
            readers: self.create_group(),
        }
    }

    #[track_caller]
    fn borrow_tracked_cell<T: 'static>(&mut self, cell: TrackedCell<T>) -> Ref<'_, T> {
        self.try_borrow_tracked_cell(cell)
            .unwrap_or_else(|error| panic!("{error}"))
    }

    fn try_borrow_tracked_cell<T: 'static>(
        &mut self,
        cell: TrackedCell<T>,
    ) -> Result<Ref<'_, T>, BorrowCellError> {
        // Check the borrow first, so a failed read isn't tracked.
        drop(self.try_borrow_cell(cell.cell)?);

        if let Some(node) = self.current_node() {
            let readers = cell.readers;
            let added = self.add_to_group(readers, node).expect(
                "a tracked cell's group lives as long as the cell, and the current node is running",
            );
            if added {
                self.register_rewind(node, move |ctx: &mut Context, _| {
                    // The cell may have been removed since, taking the group
                    // with it; then there's nothing to undo.
                    let _ = ctx.remove_from_group(readers, node);
                })
                .expect("the current node is running, so it's in the trace");
            }
        }

        self.try_borrow_cell(cell.cell)
    }

    #[track_caller]
    fn borrow_tracked_cell_mut<T: 'static>(&mut self, cell: TrackedCell<T>) -> RefMut<'_, T> {
        self.try_borrow_tracked_cell_mut(cell)
            .unwrap_or_else(|error| panic!("{error}"))
    }

    fn try_borrow_tracked_cell_mut<T: 'static>(
        &mut self,
        cell: TrackedCell<T>,
    ) -> Result<RefMut<'_, T>, BorrowCellMutError> {
        // Check the borrow first, so a failed write doesn't queue anything.
        drop(self.try_borrow_cell_mut(cell.cell)?);

        let readers: Vec<_> = self
            .group_members(cell.readers)
            .expect("a tracked cell's group lives as long as the cell")
            .collect();
        for node in readers {
            self.queue_execution(node);
        }

        self.try_borrow_cell_mut(cell.cell)
    }

    fn remove_tracked_cell<T: 'static>(
        &mut self,
        cell: TrackedCell<T>,
    ) -> Result<T, UnknownStored> {
        let value = self.remove_cell(cell.cell)?;
        self.remove_group(cell.readers)
            .expect("a tracked cell's group lives as long as the cell");
        Ok(value)
    }
}

#[cfg(test)]
#[path = "tracked_cell_tests.rs"]
mod tests;
