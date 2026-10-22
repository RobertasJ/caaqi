//! Cells that remember which nodes read them, and queue those nodes when
//! they're written.
//!
//! A read from inside a running node (one wrapped in
//! [`WithCurrent`](caaqi_scoped::current::WithCurrent)) adds the node to the
//! cell's group, until the node is rewound. A write queues every node in the
//! group with
//! [`queue_execution`](crate::linear_executor::NodeExecutionExt::queue_execution).

use std::{fmt, hash};

use caaqi_scoped::{
    current::current_node,
    group::{Group, create_group},
    rewind::{RewindKey, register_rewind},
    storage::UnknownStored,
};

use crate::{
    cell::{BorrowCellError, BorrowCellMutError, CellRef, CellRefMut, StoredCell, store_cell},
    linear_executor::NodeExecutionExt,
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
    /// Like [`try_read`](Self::try_read), but panics on error.
    #[track_caller]
    pub fn read(self) -> CellRef<T> {
        self.try_read().unwrap_or_else(|error| panic!("{error}"))
    }

    /// Reads the value, and [subscribes](Self::subscribe) the current node.
    pub fn try_read(self) -> Result<CellRef<T>, BorrowCellError> {
        // Borrow first, so a failed read doesn't subscribe.
        let value = self.try_silent_read()?;
        self.subscribe()?;
        Ok(value)
    }

    /// Like [`try_silent_read`](Self::try_silent_read), but panics on error.
    #[track_caller]
    pub fn silent_read(self) -> CellRef<T> {
        self.try_silent_read()
            .unwrap_or_else(|error| panic!("{error}"))
    }

    /// Reads the value without subscribing: a write won't rerun the node
    /// reading it.
    pub fn try_silent_read(self) -> Result<CellRef<T>, BorrowCellError> {
        self.cell.try_borrow()
    }

    /// Like [`try_write`](Self::try_write), but panics on error.
    #[track_caller]
    pub fn write(self) -> CellRefMut<T> {
        self.try_write().unwrap_or_else(|error| panic!("{error}"))
    }

    /// Writes the value, and [notifies](Self::notify) the cell's readers.
    pub fn try_write(self) -> Result<CellRefMut<T>, BorrowCellMutError> {
        // Borrow first, so a failed write doesn't notify.
        let value = self.try_silent_write()?;
        self.notify()?;
        Ok(value)
    }

    /// Like [`try_silent_write`](Self::try_silent_write), but panics on
    /// error.
    #[track_caller]
    pub fn silent_write(self) -> CellRefMut<T> {
        self.try_silent_write()
            .unwrap_or_else(|error| panic!("{error}"))
    }

    /// Writes the value without notifying: the cell's readers aren't
    /// queued. Call [`notify`](Self::notify) to queue them later.
    pub fn try_silent_write(self) -> Result<CellRefMut<T>, BorrowCellMutError> {
        self.cell.try_borrow_mut()
    }

    /// Like [`try_set`](Self::try_set), but panics on error.
    #[track_caller]
    pub fn set(self, value: T) {
        self.try_set(value)
            .unwrap_or_else(|error| panic!("{error}"));
    }

    /// Replaces the value, and [notifies](Self::notify) the cell's readers.
    pub fn try_set(self, value: T) -> Result<(), BorrowCellMutError> {
        *self.try_write()? = value;
        Ok(())
    }

    /// Like [`try_set_if_changed`](Self::try_set_if_changed), but panics on
    /// error.
    #[track_caller]
    pub fn set_if_changed(self, value: T) -> bool
    where
        T: PartialEq,
    {
        self.try_set_if_changed(value)
            .unwrap_or_else(|error| panic!("{error}"))
    }

    /// Replaces the value and [notifies](Self::notify) the cell's readers,
    /// unless `value` equals the current value; then it leaves both alone.
    /// Returns whether the value changed.
    pub fn try_set_if_changed(self, value: T) -> Result<bool, BorrowCellMutError>
    where
        T: PartialEq,
    {
        let mut current = self.try_silent_write()?;
        if *current == value {
            return Ok(false);
        }
        *current = value;
        drop(current);
        self.notify()?;
        Ok(true)
    }

    /// Adds the current node to the cell's readers until the node is
    /// rewound, as reading does. Outside a running node (one wrapped in
    /// [`WithCurrent`](caaqi_scoped::current::WithCurrent)), there's no node
    /// to add, and it does nothing.
    pub fn subscribe(self) -> Result<(), UnknownStored> {
        self.cell.shared()?;
        let Some(node) = current_node() else {
            return Ok(());
        };

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
        Ok(())
    }

    /// Queues the cell's readers for execution, as writing does.
    pub fn notify(self) -> Result<(), UnknownStored> {
        self.cell.shared()?;
        let readers = self
            .readers
            .members()
            .expect("a tracked cell's group lives as long as the cell");
        for node in readers {
            node.queue_execution();
        }
        Ok(())
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
