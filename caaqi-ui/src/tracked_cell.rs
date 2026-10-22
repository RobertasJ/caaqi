//! Cells that remember which nodes read them, and queue those nodes when
//! they're written.
//!
//! A read from inside a running node (one wrapped in
//! [`WithCurrent`](caaqi_scoped::current::WithCurrent)) adds the node to the
//! cell's group, until the node is rewound. A write queues every node in the
//! group with [`queue_execution`].

use std::{fmt, hash};

use caaqi_scoped::{
    current::current_node,
    group::{Group, create_group},
    rewind::{RewindKey, register_rewind},
    storage::UnknownStored,
};

use crate::{
    cell::{BorrowCellError, BorrowCellMutError, CellRef, CellRefMut, StoredCell, store_cell},
    linear_executor::queue_execution,
};

/// A handle to a cell whose readers are tracked. `Copy` whatever `T` is.
pub struct TrackedCell<T: 'static> {
    cell: StoredCell<T>,
    /// The nodes that read the cell. It lives as long as the cell.
    readers: Group,
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

/// Moves `value` into the context, in a cell whose readers are tracked.
pub fn store_tracked_cell<T: 'static>(value: T) -> TrackedCell<T> {
    TrackedCell {
        cell: store_cell(value),
        readers: create_group(),
    }
}

impl<T: 'static> TrackedCell<T> {
    /// Like [`try_borrow`](Self::try_borrow), but panics on error.
    #[track_caller]
    pub fn borrow(self) -> CellRef<T> {
        self.try_borrow().unwrap_or_else(|error| panic!("{error}"))
    }

    /// Borrows the value. Inside a running node, also adds the node to the
    /// cell's readers until the node is rewound.
    pub fn try_borrow(self) -> Result<CellRef<T>, BorrowCellError> {
        // Borrow first, so a failed read isn't tracked.
        let value = self.cell.try_borrow()?;

        if let Some(node) = current_node() {
            let readers = self.readers;
            let added = readers.insert(node).expect(
                "a tracked cell's group lives as long as the cell, and the current node is running",
            );
            if added {
                register_rewind(node, move |_: RewindKey| {
                    // The cell may have been removed since, taking the group
                    // with it; then there's nothing to undo.
                    let _ = readers.remove(node);
                })
                .expect("the current node is running, so it's in the trace");
            }
        }

        Ok(value)
    }

    /// Like [`try_borrow_mut`](Self::try_borrow_mut), but panics on error.
    #[track_caller]
    pub fn borrow_mut(self) -> CellRefMut<T> {
        self.try_borrow_mut()
            .unwrap_or_else(|error| panic!("{error}"))
    }

    /// Borrows the value mutably, and queues the cell's readers for
    /// execution.
    pub fn try_borrow_mut(self) -> Result<CellRefMut<T>, BorrowCellMutError> {
        // Borrow first, so a failed write doesn't queue anything.
        let value = self.cell.try_borrow_mut()?;

        let readers = self
            .readers
            .members()
            .expect("a tracked cell's group lives as long as the cell");
        for node in readers {
            queue_execution(node);
        }

        Ok(value)
    }

    /// Removes the cell and its group of readers, like
    /// [`StoredCell::remove`]: borrows made before stay valid, and the
    /// value is dropped once the last of them ends.
    pub fn remove(self) -> Result<(), UnknownStored> {
        self.cell.remove()?;
        self.readers
            .delete()
            .expect("a tracked cell's group lives as long as the cell");
        Ok(())
    }
}

#[cfg(test)]
#[path = "tracked_cell_tests.rs"]
mod tests;
