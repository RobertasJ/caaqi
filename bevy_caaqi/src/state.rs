use atomic_refcell::{AtomicRef, AtomicRefCell, AtomicRefMut};
use bevy::{
    ecs::{
        component::Component,
        entity::{Entity, EntityNotSpawnedError},
        world::{self, DeferredWorld, EntityMut, EntityRef, World, error::EntityMutableFetchError},
    },
    prelude::{Deref, DerefMut},
};
use caaqi_context::Attached;
use ouroboros::self_referencing;
use std::{
    any::Any,
    hash::{Hash, Hasher},
    marker::PhantomData,
    ops::{Deref, DerefMut},
    panic::Location,
    sync::Arc,
};

use crate::{
    action::context_builder::{action, rewind},
    tracking::{TrackingKey, notify, notify_with_caller, subscribe, subscribe_with_caller},
    value::{AtomicRefCellStorage, Storage, Value, ValueReadError, ValueWriteError},
};

#[derive(Debug)]
pub struct State<T: Send + Sync + 'static, S: Storage = AtomicRefCellStorage<T>> {
    value: Value<T, S>,
    tracking_key: TrackingKey,
}

impl<T: Send + Sync + 'static, S: Storage> Copy for State<T, S> {}

impl<T: Send + Sync + 'static, S: Storage> Clone for State<T, S> {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
            tracking_key: self.tracking_key.clone(),
        }
    }
}

impl<T: Send + Sync + 'static, S: Storage> Hash for State<T, S> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.value.hash(state);
        self.tracking_key.hash(state);
    }
}

impl<T: Send + Sync + 'static, S: Storage> Eq for State<T, S> {}

impl<T: Send + Sync + 'static, S: Storage> PartialEq for State<T, S> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value && self.tracking_key == other.tracking_key
    }
}

pub fn state<T: Send + Sync + 'static>(world: &mut World, value: T) -> State<T> {
    let state = State::new(world, value);

    rewind(world, move |world| {
        state.remove(world);
    });

    state
}

impl<T: Send + Sync + 'static> State<T, AtomicRefCellStorage<T>> {
    pub fn new_with_caller(
        world: &mut World,
        caller: &'static Location<'static>,
        value: T,
    ) -> Self {
        let value = Value::from_value_with_caller(world, caller, value);
        State {
            value,
            tracking_key: TrackingKey::new(world),
        }
    }

    #[track_caller]
    pub fn new(world: &mut World, value: T) -> Self {
        Self::new_with_caller(world, Location::caller(), value)
    }
}

impl<T: Any + Send + Sync + 'static, S: Storage<Value = T>> State<T, S> {
    pub fn new_with_storage_with_caller(
        world: &mut World,
        caller: &'static Location<'static>,
        value: T,
    ) -> Self {
        let value = Value::from_value_with_storage_with_caller(world, caller, value);
        State {
            value,
            tracking_key: TrackingKey::new(world),
        }
    }

    #[track_caller]
    pub fn new_with_storage(world: &mut World, value: T) -> Self {
        Self::new_with_storage_with_caller(world, Location::caller(), value)
    }

    pub fn remove(self, world: &mut World) {
        self.value.remove(world);
        self.tracking_key.remove(world);
    }

    pub fn subscribe_with_caller(&self, world: DeferredWorld, caller: &'static Location<'static>) {
        subscribe_with_caller(world, caller, self.tracking_key);
    }

    #[track_caller]
    pub fn subscribe(&self, world: DeferredWorld) {
        subscribe(world, self.tracking_key);
    }

    pub fn notify_with_caller(&self, world: DeferredWorld, caller: &'static Location<'static>) {
        notify_with_caller(world, caller, self.tracking_key, false);
    }

    #[track_caller]
    pub fn notify(&self, world: DeferredWorld) {
        notify(world, self.tracking_key, false);
    }

    pub fn try_read_silent(&self, world: DeferredWorld) -> Result<S::Ref, ValueReadError<S, T>> {
        self.value.try_read(world)
    }

    pub fn try_write_silent(
        &mut self,
        world: DeferredWorld,
    ) -> Result<S::RefMut, ValueWriteError<S, T>> {
        self.value.try_write(world)
    }

    pub fn read_silent(&self, world: DeferredWorld) -> S::Ref {
        self.try_read_silent(world).unwrap_or_else(|e| {
            panic!(
                "Failed to read value of type {}: {}",
                std::any::type_name::<T>(),
                e
            )
        })
    }

    pub fn write_silent(&mut self, world: DeferredWorld) -> S::RefMut {
        self.try_write_silent(world).unwrap_or_else(|e| {
            panic!(
                "Failed to write value of type {}: {}",
                std::any::type_name::<T>(),
                e
            )
        })
    }

    pub fn read_with_caller(
        &self,
        mut world: DeferredWorld,
        caller: &'static std::panic::Location<'static>,
    ) -> S::Ref {
        self.subscribe_with_caller(world.reborrow(), caller);
        self.read_silent(world)
    }

    pub fn write_with_caller(
        &mut self,
        mut world: DeferredWorld,
        caller: &'static std::panic::Location<'static>,
    ) -> S::RefMut {
        self.notify_with_caller(world.reborrow(), caller);
        self.write_silent(world)
    }

    #[track_caller]
    pub fn read(&self, mut world: DeferredWorld) -> S::Ref {
        self.subscribe(world.reborrow());
        self.read_silent(world)
    }

    #[track_caller]
    pub fn write(&mut self, mut world: DeferredWorld) -> S::RefMut {
        self.notify(world.reborrow());
        self.write_silent(world)
    }

    pub fn set_with_caller(
        &mut self,
        mut world: DeferredWorld,
        value: T,
        caller: &'static std::panic::Location<'static>,
    ) {
        *self.write_with_caller(world.reborrow(), caller) = value;
    }

    #[track_caller]
    pub fn set(&mut self, mut world: DeferredWorld, value: T) {
        *self.write(world.reborrow()) = value;
    }
}
