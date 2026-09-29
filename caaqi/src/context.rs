//! [`Context`], the object that holds all of caaqi's state, and yours.
//!
//! A context stores at most one value of each type. caaqi keeps its own state
//! there (the trace, rewinds, groups and so on), and you can keep your own
//! state there too:
//!
//! ```
//! use caaqi::prelude::*;
//!
//! #[derive(Default)]
//! struct Score(u32);
//!
//! let mut ctx = Context::new();
//! ctx.get_or_insert_with(Score::default).0 += 10;
//! assert_eq!(ctx.get::<Score>().unwrap().0, 10);
//! ```
//!
//! Every feature of caaqi is a method on `Context`, added by an extension
//! trait such as [`TraceExt`](crate::trace::TraceExt) or
//! [`RewindExt`](crate::rewind::RewindExt). Import the traits you need, or
//! everything common at once with [`caaqi::prelude`](crate::prelude).
//!
//! # Borrowing
//!
//! A reference returned by the context borrows the whole context, so you
//! can't keep it while calling another method that needs the context. Either
//! let the reference go first, or take the value out with
//! [`remove`](Context::remove), make the call, and put it back with
//! [`insert`](Context::insert).

use std::{
    any::{Any, TypeId},
    collections::HashMap,
};

/// Holds all state: at most one value of each type, caaqi's own included.
///
/// A value stays until it's replaced with [`insert`](Self::insert) or taken
/// out with [`remove`](Self::remove). See the [module docs](self) for an
/// example and for how borrowing works.
///
/// # Keys belong to one context
///
/// Keys and handles, such as [`NodeKey`](crate::trace::NodeKey),
/// [`RewindKey`](crate::rewind::RewindKey),
/// [`GroupId`](crate::group::GroupId) and
/// [`Stored`](crate::storage::Stored), only work with the context that
/// created them. Using one with another context is a mistake that isn't
/// detected: it may reach something unrelated.
#[derive(Default)]
pub struct Context {
    data: HashMap<TypeId, Box<dyn Any>>,
}

impl Context {
    /// Creates an empty context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Stores `value`, replacing any value of the same type. Returns the value
    /// it replaced, if there was one.
    pub fn insert<T: 'static>(&mut self, value: T) -> Option<T> {
        self.data
            .insert(TypeId::of::<T>(), Box::new(value))
            .map(|value| *value.downcast().expect("data is keyed by its type"))
    }

    /// Returns the value of type `T`, or `None` if there isn't one.
    pub fn get<T: 'static>(&self) -> Option<&T> {
        self.data
            .get(&TypeId::of::<T>())
            .map(|value| value.downcast_ref().expect("data is keyed by its type"))
    }

    /// Returns the value of type `T` mutably, or `None` if there isn't one.
    pub fn get_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.data
            .get_mut(&TypeId::of::<T>())
            .map(|value| value.downcast_mut().expect("data is keyed by its type"))
    }

    /// Returns the value of type `T` mutably. If there isn't one yet, it's
    /// created with `f` first.
    pub fn get_or_insert_with<T: 'static>(&mut self, f: impl FnOnce() -> T) -> &mut T {
        self.data
            .entry(TypeId::of::<T>())
            .or_insert_with(|| Box::new(f()))
            .downcast_mut()
            .expect("data is keyed by its type")
    }

    /// Takes the value of type `T` out of the context and returns it, or
    /// returns `None` if there isn't one.
    pub fn remove<T: 'static>(&mut self) -> Option<T> {
        self.data
            .remove(&TypeId::of::<T>())
            .map(|value| *value.downcast().expect("data is keyed by its type"))
    }
}
