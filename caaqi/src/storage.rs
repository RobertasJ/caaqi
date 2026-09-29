//! Keep values of any type in the [`Context`] and get them back later.
//!
//! Call [`store`](StorageExt::store) to move a value into the context. You
//! get back a [`Stored<T>`] handle, which you use to read, change or remove
//! the value:
//!
//! ```
//! use caaqi::{prelude::*, storage::StorageExt};
//!
//! let mut ctx = Context::new();
//! let name = ctx.store(String::from("Ada"));
//!
//! ctx.get_stored_mut(name)?.push_str(" Lovelace");
//! assert_eq!(ctx.get_stored(name)?, "Ada Lovelace");
//!
//! let owned: String = ctx.remove_stored(name)?;
//! assert!(ctx.get_stored(name).is_err());
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! A handle is small and always `Copy`, even when the value itself isn't, so
//! you can move it into as many runners and rewinds as you like. The value
//! stays in the context until you remove it; after that, every use of the
//! handle returns an [`UnknownStored`] error.

use std::{fmt, hash, marker::PhantomData};

use slotmap::{SecondaryMap, SlotMap, new_key_type};

use crate::context::Context;

new_key_type! {
    struct StorageKey;
}

/// A handle to a value of type `T`, returned by [`StorageExt::store`].
///
/// Pass it to [`get_stored`](StorageExt::get_stored),
/// [`get_stored_mut`](StorageExt::get_stored_mut),
/// [`clone_stored`](StorageExt::clone_stored) or
/// [`remove_stored`](StorageExt::remove_stored) to reach the value. Handles
/// are `Copy`, and can be compared and hashed, whatever `T` is.
pub struct Stored<T: 'static> {
    key: StorageKey,
    _marker: PhantomData<fn() -> T>,
}

// Implemented by hand: deriving would require `T` to implement each trait,
// but a handle is `Copy`, comparable and printable whatever `T` is.
impl<T: 'static> Clone for Stored<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: 'static> Copy for Stored<T> {}

impl<T: 'static> PartialEq for Stored<T> {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

impl<T: 'static> Eq for Stored<T> {}

impl<T: 'static> hash::Hash for Stored<T> {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        self.key.hash(state);
    }
}

impl<T: 'static> fmt::Debug for Stored<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Stored").field(&self.key).finish()
    }
}

/// Returned when a [`Stored`] handle's value is no longer in the context,
/// because it was removed with [`remove_stored`](StorageExt::remove_stored).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the stored value {0:?} was removed")]
pub struct UnknownStored(StorageKey);

/// The storage resource for values of type `T`, keyed by the keys in
/// [`StorageKeys`]. Everything public goes through [`StorageExt`].
struct Storage<T: 'static> {
    values: SlotMap<StorageKey, T>,
}

impl<T: 'static> Default for Storage<T> {
    fn default() -> Self {
        Self {
            values: SlotMap::with_key(),
        }
    }
}

impl<T: 'static> Storage<T> {
    fn get(&self, stored: Stored<T>) -> Result<&T, UnknownStored> {
        self.values.get(stored.key).ok_or(UnknownStored(stored.key))
    }

    fn get_mut(&mut self, stored: Stored<T>) -> Result<&mut T, UnknownStored> {
        self.values
            .get_mut(stored.key)
            .ok_or(UnknownStored(stored.key))
    }

    fn remove(&mut self, stored: Stored<T>) -> Result<T, UnknownStored> {
        self.values
            .remove(stored.key)
            .ok_or(UnknownStored(stored.key))
    }
}

/// The storage for `T`, for an access through `stored`. Without it no value
/// of type `T` exists yet.
fn storage<T: 'static>(ctx: &Context, stored: Stored<T>) -> Result<&Storage<T>, UnknownStored> {
    ctx.get::<Storage<T>>().ok_or(UnknownStored(stored.key))
}

fn storage_mut<T: 'static>(
    ctx: &mut Context,
    stored: Stored<T>,
) -> Result<&mut Storage<T>, UnknownStored> {
    ctx.get_mut::<Storage<T>>().ok_or(UnknownStored(stored.key))
}

/// Store values in a [`Context`] and reach them through [`Stored`] handles.
/// See the [module docs](self) for an example.
pub trait StorageExt {
    /// Moves `value` into the context and returns a handle to it. The value
    /// stays there until you call [`remove_stored`](Self::remove_stored).
    fn store<T: 'static>(&mut self, value: T) -> Stored<T>;

    /// Returns a reference to the value.
    ///
    /// Returns [`UnknownStored`] if the value was removed.
    fn get_stored<T: 'static>(&self, stored: Stored<T>) -> Result<&T, UnknownStored>;

    /// Returns a mutable reference to the value, to change it in place.
    ///
    /// Returns [`UnknownStored`] if the value was removed.
    fn get_stored_mut<T: 'static>(&mut self, stored: Stored<T>) -> Result<&mut T, UnknownStored>;

    /// Returns a clone of the value. Useful when you need the value while
    /// also changing the context, which a reference from
    /// [`get_stored`](Self::get_stored) wouldn't allow.
    ///
    /// Returns [`UnknownStored`] if the value was removed.
    fn clone_stored<T: Clone + 'static>(&self, stored: Stored<T>) -> Result<T, UnknownStored>;

    /// Takes the value out of the context and returns it. From then on,
    /// every use of `stored` (and of its copies) returns [`UnknownStored`].
    ///
    /// Returns [`UnknownStored`] if the value was already removed.
    fn remove_stored<T: 'static>(&mut self, stored: Stored<T>) -> Result<T, UnknownStored>;
}

impl StorageExt for Context {
    fn store<T: 'static>(&mut self, value: T) -> Stored<T> {
        let key = self
            .get_or_insert_with(Storage::<T>::default)
            .values
            .insert(value);

        Stored {
            key,
            _marker: PhantomData,
        }
    }

    fn get_stored<T: 'static>(&self, stored: Stored<T>) -> Result<&T, UnknownStored> {
        storage(self, stored)?.get(stored)
    }

    fn get_stored_mut<T: 'static>(&mut self, stored: Stored<T>) -> Result<&mut T, UnknownStored> {
        storage_mut(self, stored)?.get_mut(stored)
    }

    fn clone_stored<T: Clone + 'static>(&self, stored: Stored<T>) -> Result<T, UnknownStored> {
        self.get_stored(stored).cloned()
    }

    fn remove_stored<T: 'static>(&mut self, stored: Stored<T>) -> Result<T, UnknownStored> {
        let value = storage_mut(self, stored)?.remove(stored)?;

        Ok(value)
    }
}

#[cfg(test)]
#[path = "storage_tests.rs"]
mod tests;
