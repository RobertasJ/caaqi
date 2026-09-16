use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use atomic_refcell::{AtomicRef, AtomicRefCell, AtomicRefMut, BorrowError, BorrowMutError};
use ouroboros::self_referencing;

use crate::value_storage::AtomicRefCellStorage;

#[self_referencing]
pub struct ArcBorrow<T: 'static> {
    cell: Arc<AtomicRefCellStorage<T>>,

    #[borrows(cell)]
    #[covariant]
    guard: AtomicRef<'this, T>,
}

impl<T: 'static> ArcBorrow<T> {
    pub fn try_borrow(cell: Arc<AtomicRefCellStorage<T>>) -> Result<Self, BorrowError> {
        Self::try_new(cell, |cell| cell.0.try_borrow())
    }

    #[track_caller]
    pub fn borrow(cell: Arc<AtomicRefCellStorage<T>>) -> Self {
        Self::try_borrow(cell).unwrap_or_else(|err| panic!("Failed to borrow value: {err}"))
    }
}

impl<T: 'static> Deref for ArcBorrow<T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.borrow_guard().deref()
    }
}

#[self_referencing]
pub struct ArcBorrowMut<T: 'static> {
    cell: Arc<AtomicRefCellStorage<T>>,

    #[borrows(cell)]
    #[covariant]
    guard: AtomicRefMut<'this, T>,
}

impl<T: 'static> ArcBorrowMut<T> {
    pub fn try_borrow(cell: Arc<AtomicRefCellStorage<T>>) -> Result<Self, BorrowMutError> {
        Self::try_new(cell, |cell| cell.0.try_borrow_mut())
    }

    #[track_caller]
    pub fn borrow(cell: Arc<AtomicRefCellStorage<T>>) -> Self {
        Self::try_borrow(cell).unwrap_or_else(|err| panic!("Failed to mutably borrow value: {err}"))
    }
}

impl<T: 'static> Deref for ArcBorrowMut<T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.borrow_guard().deref()
    }
}

impl<T: 'static> DerefMut for ArcBorrowMut<T> {
    fn deref_mut(&mut self) -> &mut T {
        self.with_guard_mut(|guard| guard.deref_mut())
    }
}
