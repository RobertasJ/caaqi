use std::{
    any::{Any, TypeId},
    collections::HashMap,
};

/// A type-map of resources: one value per type, global to the context.
///
/// Everything lives here, including caaqi's own modules (see
/// [`ActionTree`](crate::action::ActionTree)). Modules expose their API as
/// extension traits implemented for `Context`.
///
/// Values survive reruns and descendant removal, and are dropped only by
/// `remove`. Returned references borrow the context, so they can't be held
/// across a nested run; drop the borrow first, or `remove` + `insert` around it.
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
