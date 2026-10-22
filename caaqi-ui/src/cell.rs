//! Values kept in the context and borrowed like a `RefCell`.

use std::{
    cell::{BorrowError, BorrowMutError, Ref, RefCell, RefMut},
    fmt, hash,
    ops::{Deref, DerefMut},
    rc::Rc,
};

use caaqi_scoped::storage::{Stored, UnknownStored, store};

/// A handle to a value kept in the context, borrowed like a `RefCell`.
/// `Copy` whatever `T` is.
///
/// A borrow shares the value out of the context, so it doesn't hold the
/// context: while it's alive, caaqi-scoped's functions still work, other
/// cells included. Conflicting borrows of the same cell fail as a
/// `RefCell`'s do.
pub struct StoredCell<T: 'static>(Stored<Rc<RefCell<T>>>);

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

/// A borrow of a [`StoredCell`]'s value, returned by
/// [`borrow`](StoredCell::borrow). It keeps the cell alive, so it's
/// `'static`, and derefs to the value.
#[ouroboros::self_referencing]
pub struct CellRef<T: 'static> {
    cell: Rc<RefCell<T>>,
    #[borrows(cell)]
    #[covariant]
    value: Ref<'this, T>,
}

impl<T: 'static> Deref for CellRef<T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.borrow_value()
    }
}

/// A mutable borrow of a [`StoredCell`]'s value, returned by
/// [`borrow_mut`](StoredCell::borrow_mut). It keeps the cell alive, so it's
/// `'static`, and derefs to the value.
#[ouroboros::self_referencing]
pub struct CellRefMut<T: 'static> {
    cell: Rc<RefCell<T>>,
    #[borrows(cell)]
    #[covariant]
    value: RefMut<'this, T>,
}

impl<T: 'static> Deref for CellRefMut<T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.borrow_value()
    }
}

impl<T: 'static> DerefMut for CellRefMut<T> {
    fn deref_mut(&mut self) -> &mut T {
        self.with_value_mut(|value| &mut **value)
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

/// Moves `value` into the context, in a cell.
pub fn store_cell<T: 'static>(value: T) -> StoredCell<T> {
    StoredCell(store(Rc::new(RefCell::new(value))))
}

impl<T: 'static> StoredCell<T> {
    /// The cell, shared out of the context.
    pub(crate) fn shared(self) -> Result<Rc<RefCell<T>>, UnknownStored> {
        self.0.get(Rc::clone)
    }

    /// Like [`try_borrow`](Self::try_borrow), but panics on error.
    #[track_caller]
    pub fn borrow(self) -> CellRef<T> {
        self.try_borrow().unwrap_or_else(|error| panic!("{error}"))
    }

    /// Borrows the value.
    pub fn try_borrow(self) -> Result<CellRef<T>, BorrowCellError> {
        Ok(CellRef::try_new(self.shared()?, |cell| cell.try_borrow())?)
    }

    /// Like [`try_borrow_mut`](Self::try_borrow_mut), but panics on error.
    #[track_caller]
    pub fn borrow_mut(self) -> CellRefMut<T> {
        self.try_borrow_mut()
            .unwrap_or_else(|error| panic!("{error}"))
    }

    /// Borrows the value mutably.
    pub fn try_borrow_mut(self) -> Result<CellRefMut<T>, BorrowCellMutError> {
        Ok(CellRefMut::try_new(self.shared()?, |cell| {
            cell.try_borrow_mut()
        })?)
    }

    /// Removes the cell from the context: from then on, borrowing it fails
    /// with [`UnknownStored`]. Borrows made before stay valid, and the value
    /// is dropped once the last of them ends.
    pub fn remove(self) -> Result<(), UnknownStored> {
        self.0.remove()?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "cell_tests.rs"]
mod tests;
