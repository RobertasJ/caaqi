//! Values of any type kept in the context, like [`caaqi::storage`].
//!
//! The getters take a closure instead of returning a reference; it mustn't
//! call this crate's functions.

use std::{fmt, hash};

use caaqi::storage::StorageExt;
pub use caaqi::storage::UnknownStored;

use crate::context::with_ctx;

/// A handle to a value kept in the context: a newtype over caaqi's
/// [`Stored`](caaqi::storage::Stored) with the value's operations as
/// methods. Convert with `From` either way.
///
/// `Copy` whatever `T` is. Once the value is [removed](Self::remove), every
/// use of the handle and its copies fails with [`UnknownStored`].
pub struct Stored<T: 'static>(caaqi::storage::Stored<T>);

impl<T: 'static> Clone for Stored<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: 'static> Copy for Stored<T> {}

impl<T: 'static> PartialEq for Stored<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<T: 'static> Eq for Stored<T> {}

impl<T: 'static> hash::Hash for Stored<T> {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state)
    }
}

impl<T: 'static> fmt::Debug for Stored<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<T: 'static> From<caaqi::storage::Stored<T>> for Stored<T> {
    fn from(stored: caaqi::storage::Stored<T>) -> Self {
        Stored(stored)
    }
}

impl<T: 'static> From<Stored<T>> for caaqi::storage::Stored<T> {
    fn from(stored: Stored<T>) -> Self {
        stored.0
    }
}

impl<T: 'static> Stored<T> {
    /// [`StorageExt::get_stored`]: calls `f` with the value. `f` mustn't
    /// call this crate's functions.
    pub fn get<R>(self, f: impl FnOnce(&T) -> R) -> Result<R, UnknownStored> {
        with_ctx(|ctx| ctx.get_stored(self.0).map(f))
    }

    /// [`StorageExt::get_stored_mut`]: calls `f` with the value mutably. `f`
    /// mustn't call this crate's functions.
    pub fn get_mut<R>(self, f: impl FnOnce(&mut T) -> R) -> Result<R, UnknownStored> {
        with_ctx(|ctx| ctx.get_stored_mut(self.0).map(f))
    }

    /// [`StorageExt::remove_stored`]: takes the value out of the context.
    pub fn remove(self) -> Result<T, UnknownStored> {
        with_ctx(|ctx| ctx.remove_stored(self.0))
    }
}

/// [`StorageExt::store`]: moves `value` into the context.
pub fn store<T: 'static>(value: T) -> Stored<T> {
    with_ctx(|ctx| Stored(ctx.store(value)))
}

#[cfg(test)]
#[path = "storage_tests.rs"]
mod tests;
