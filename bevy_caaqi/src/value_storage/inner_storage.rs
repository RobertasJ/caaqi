use std::{
    fmt::Display,
    ops::{Deref, DerefMut},
    sync::Arc,
};

use atomic_refcell::AtomicRefCell;

use crate::value_storage::{ArcBorrow, ArcBorrowMut, arc_borrow};

pub trait Storage: Send + Sync + 'static {
    type Value: Send + Sync + 'static;

    type Ref: Deref<Target = Self::Value>;

    type RefMut: DerefMut<Target = Self::Value>;

    type ReadError: Display;
    type WriteError: Display;

    fn new(value: Self::Value) -> Self;
    fn try_read(self: Arc<Self>) -> Result<Self::Ref, Self::ReadError>;
    fn try_write(self: Arc<Self>) -> Result<Self::RefMut, Self::WriteError>;

    #[track_caller]
    fn read(self: Arc<Self>) -> Self::Ref {
        self.try_read().unwrap_or_else(|err| {
            panic!(
                "Failed to read {}: {err}",
                std::any::type_name::<Self::Value>()
            )
        })
    }

    #[track_caller]
    fn write(self: Arc<Self>) -> Self::RefMut {
        self.try_write().unwrap_or_else(|err| {
            panic!(
                "Failed to write {}: {err}",
                std::any::type_name::<Self::Value>()
            )
        })
    }
}

pub struct AtomicRefCellStorage<T>(pub(crate) AtomicRefCell<T>);

impl<T: Send + Sync + 'static> Storage for AtomicRefCellStorage<T> {
    type Value = T;

    type Ref = arc_borrow::ArcBorrow<T>;
    type RefMut = arc_borrow::ArcBorrowMut<T>;

    type ReadError = atomic_refcell::BorrowError;
    type WriteError = atomic_refcell::BorrowMutError;

    fn new(value: T) -> Self {
        Self(AtomicRefCell::new(value))
    }

    fn try_read(self: Arc<Self>) -> Result<Self::Ref, Self::ReadError> {
        Ok(ArcBorrow::try_borrow(self)?)
    }

    fn try_write(self: Arc<Self>) -> Result<Self::RefMut, Self::WriteError> {
        Ok(ArcBorrowMut::try_borrow(self)?)
    }
}
