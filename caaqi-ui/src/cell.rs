use std::{
    cell::{BorrowError, BorrowMutError, Ref, RefCell, RefMut},
    fmt, hash,
};

use caaqi::{
    context::Context,
    storage::{StorageExt, Stored, UnknownStored},
};

pub struct StoredCell<T: 'static>(Stored<RefCell<T>>);

// Implemented by hand: deriving would require `T` to implement each trait.
impl<T: 'static> Clone for StoredCell<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: 'static> Copy for StoredCell<T> {}

impl<T: 'static> PartialEq for StoredCell<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<T: 'static> Eq for StoredCell<T> {}

impl<T: 'static> hash::Hash for StoredCell<T> {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl<T: 'static> fmt::Debug for StoredCell<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("StoredCell").field(&self.0).finish()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BorrowCellError {
    #[error(transparent)]
    UnknownStored(#[from] UnknownStored),
    #[error(transparent)]
    Borrow(#[from] BorrowError),
}

#[derive(Debug, thiserror::Error)]
pub enum BorrowCellMutError {
    #[error(transparent)]
    UnknownStored(#[from] UnknownStored),
    #[error(transparent)]
    BorrowMut(#[from] BorrowMutError),
}

pub trait CellExt {
    fn store_cell<T: 'static>(&mut self, value: T) -> StoredCell<T>;

    /// Like [`try_borrow_cell`](CellExt::try_borrow_cell), but panics on error.
    fn borrow_cell<T: 'static>(&self, cell: StoredCell<T>) -> Ref<'_, T>;

    fn try_borrow_cell<T: 'static>(
        &self,
        cell: StoredCell<T>,
    ) -> Result<Ref<'_, T>, BorrowCellError>;

    /// Like [`try_borrow_cell_mut`](CellExt::try_borrow_cell_mut), but panics on error.
    fn borrow_cell_mut<T: 'static>(&self, cell: StoredCell<T>) -> RefMut<'_, T>;

    fn try_borrow_cell_mut<T: 'static>(
        &self,
        cell: StoredCell<T>,
    ) -> Result<RefMut<'_, T>, BorrowCellMutError>;

    fn remove_cell<T: 'static>(&mut self, cell: StoredCell<T>) -> Result<T, UnknownStored>;
}

impl CellExt for Context {
    fn store_cell<T: 'static>(&mut self, value: T) -> StoredCell<T> {
        StoredCell(self.store(RefCell::new(value)))
    }

    #[track_caller]
    fn borrow_cell<T: 'static>(&self, cell: StoredCell<T>) -> Ref<'_, T> {
        self.try_borrow_cell(cell)
            .unwrap_or_else(|error| panic!("{error}"))
    }

    fn try_borrow_cell<T: 'static>(
        &self,
        cell: StoredCell<T>,
    ) -> Result<Ref<'_, T>, BorrowCellError> {
        Ok(self.get_stored(cell.0)?.try_borrow()?)
    }

    #[track_caller]
    fn borrow_cell_mut<T: 'static>(&self, cell: StoredCell<T>) -> RefMut<'_, T> {
        self.try_borrow_cell_mut(cell)
            .unwrap_or_else(|error| panic!("{error}"))
    }

    fn try_borrow_cell_mut<T: 'static>(
        &self,
        cell: StoredCell<T>,
    ) -> Result<RefMut<'_, T>, BorrowCellMutError> {
        Ok(self.get_stored(cell.0)?.try_borrow_mut()?)
    }

    fn remove_cell<T: 'static>(&mut self, cell: StoredCell<T>) -> Result<T, UnknownStored> {
        Ok(self.remove_stored(cell.0)?.into_inner())
    }
}

#[cfg(test)]
#[path = "cell_tests.rs"]
mod tests;
