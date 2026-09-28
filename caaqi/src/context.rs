//! [`Context`], the type-map that holds all state, and the pattern every
//! layer is built with.
//!
//! A layer defines a resource type, stored in the context and fetched lazily
//! with [`get_or_insert_with`](Context::get_or_insert_with), and an
//! extension trait implemented for `Context` that holds every public
//! operation on it, reads included. The resource's own methods stay private.
//! [`Trace`](crate::trace::Trace) with [`TraceExt`](crate::trace::TraceExt)
//! and [`Rewinds`](crate::rewind::Rewinds) with
//! [`RewindExt`](crate::rewind::RewindExt) are built this way, and user
//! state can live in the context the same way.
//!
//! Getters borrow the whole context, so a returned reference can't be held
//! across another call that needs the context. Drop the borrow first, or
//! [`remove`](Context::remove) the value and [`insert`](Context::insert) it
//! back around the call.

use std::{
    any::{Any, TypeId},
    collections::HashMap,
};

/// A type-map of resources: one value per type, global to the context.
///
/// Everything lives here, including caaqi's own state (see
/// [`Trace`](crate::trace::Trace)). Modules expose their API as
/// extension traits implemented for `Context`.
///
/// Values are dropped only by `remove`. Returned references borrow the whole
/// context, so they can't be held across another call that needs it; drop the
/// borrow first, or `remove` + `insert` around it.
#[derive(Default)]
pub struct Context {
    data: HashMap<TypeId, Box<dyn Any>>,
}

impl Context {
    pub fn new() -> Self {
        Self::default()
    }

    /// Stores `value`, returning the previous value of the same type.
    pub fn insert<T: 'static>(&mut self, value: T) -> Option<T> {
        self.data
            .insert(TypeId::of::<T>(), Box::new(value))
            .map(|value| *value.downcast().expect("data is keyed by its type"))
    }

    pub fn get<T: 'static>(&self) -> Option<&T> {
        self.data
            .get(&TypeId::of::<T>())
            .map(|value| value.downcast_ref().expect("data is keyed by its type"))
    }

    pub fn get_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.data
            .get_mut(&TypeId::of::<T>())
            .map(|value| value.downcast_mut().expect("data is keyed by its type"))
    }

    pub fn get_or_insert_with<T: 'static>(&mut self, f: impl FnOnce() -> T) -> &mut T {
        self.data
            .entry(TypeId::of::<T>())
            .or_insert_with(|| Box::new(f()))
            .downcast_mut()
            .expect("data is keyed by its type")
    }

    pub fn remove<T: 'static>(&mut self) -> Option<T> {
        self.data
            .remove(&TypeId::of::<T>())
            .map(|value| *value.downcast().expect("data is keyed by its type"))
    }
}
