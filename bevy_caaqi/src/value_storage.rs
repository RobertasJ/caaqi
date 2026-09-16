use std::any::{Any, type_name_of_val};
use std::fmt::Display;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::panic::Location;
use std::sync::Arc;

use atomic_refcell::AtomicRefCell;
use bevy::ecs::component::Component;
use bevy::ecs::entity::{
    Entity, EntityNotSpawnedError, EntityValidButNotSpawnedError, InvalidEntityError,
};
use bevy::ecs::world::{DeferredWorld, World};
use thiserror::Error;

use crate::CreatedAt;
use crate::value_storage::inner_storage::{AtomicRefCellStorage, Storage};

mod arc_borrow;
mod inner_storage;

pub use arc_borrow::{ArcBorrow, ArcBorrowMut};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ValueStorageError {
    #[error("The stored value has been dropped. {0}")]
    ValueDropped(EntityValidButNotSpawnedError),
    #[error("Type mismatch: expected {requested_type}, found {value_type}")]
    TypeMismatch {
        value_type: &'static str,
        requested_type: &'static str,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ValueReadError<S: Storage<Value = T>, T> {
    #[error("Failed to read value: {0}")]
    StorageError(S::ReadError),
    #[error(transparent)]
    ValueStorageError(#[from] ValueStorageError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ValueWriteError<S: Storage<Value = T>, T> {
    #[error("Failed to write value: {0}")]
    StorageError(S::WriteError),
    #[error(transparent)]
    ValueStorageError(#[from] ValueStorageError),
}

pub trait NamedAny: Any {
    fn type_name(&self) -> &'static str;
}

impl<T: Any> NamedAny for T {
    fn type_name(&self) -> &'static str {
        std::any::type_name::<T>()
    }
}

#[derive(Component)]
struct StoredValue(Arc<dyn NamedAny + Send + Sync>);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ValueErased(Entity);

impl ValueErased {
    pub fn new_with_caller(
        world: &mut World,
        caller: &'static Location<'static>,
        value: impl Send + Sync + 'static,
    ) -> Self {
        let entity = world
            .spawn((StoredValue(Arc::new(value)), CreatedAt::new(caller)))
            .id();
        ValueErased(entity)
    }

    #[track_caller]
    pub fn new<T: Send + Sync + 'static>(world: &mut World, value: T) -> Self {
        Self::new_with_caller(world, Location::caller(), value)
    }

    /// Makes the value no longer accessible.
    pub fn remove(self, world: &mut World) {
        world.despawn(self.0);
    }

    #[track_caller]
    pub fn try_get_any(
        &self,
        world: DeferredWorld,
    ) -> Result<Arc<dyn NamedAny + Send + Sync>, ValueStorageError> {
        let stored_value = world.get_entity(self.0);

        match stored_value {
            Ok(entity) => {
                let stored_value = entity
                    .get::<StoredValue>()
                    .expect("StoredValue component should exist for the entity.");
                Ok(stored_value.0.clone())
            }
            Err(EntityNotSpawnedError::ValidButNotSpawned(e)) => {
                Err(ValueStorageError::ValueDropped(e))
            }
            Err(EntityNotSpawnedError::Invalid(e)) => {
                panic!("Invalid entity: {:?}", e);
            }
        }
    }

    #[track_caller]
    pub fn try_get<T: Send + Sync + 'static>(
        &self,
        world: DeferredWorld,
    ) -> Result<Arc<T>, ValueStorageError> {
        let stored_value = self.try_get_any(world)?;
        let erased: Arc<dyn Any + Send + Sync> = stored_value.clone();

        erased
            .downcast::<T>()
            .map_err(move |_| ValueStorageError::TypeMismatch {
                value_type: stored_value.type_name(),
                requested_type: std::any::type_name::<T>(),
            })
    }

    #[track_caller]
    pub fn get<T: Send + Sync + 'static>(&self, world: DeferredWorld) -> Arc<T> {
        self.try_get(world).unwrap_or_else(|err| {
            panic!(
                "Failed to get value of type {}: {}",
                std::any::type_name::<T>(),
                err
            )
        })
    }
}

#[derive(Debug)]
pub struct Value<T: Send + Sync + 'static, S: Storage = AtomicRefCellStorage<T>>(
    ValueErased,
    PhantomData<(T, S)>,
);

impl<T: Send + Sync + 'static, S: Storage> Clone for Value<T, S> {
    fn clone(&self) -> Self {
        Self(self.0, PhantomData)
    }
}

impl<T: Send + Sync + 'static, S: Storage> Copy for Value<T, S> {}

impl<T: Send + Sync + 'static, S: Storage> PartialEq for Value<T, S> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<T: Send + Sync + 'static, S: Storage> Eq for Value<T, S> {}

impl<T: Send + Sync + 'static, S: Storage> Hash for Value<T, S> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl<T: Send + Sync + 'static> Value<T, AtomicRefCellStorage<T>> {
    pub fn new_with_caller(
        world: &mut World,
        caller: &'static Location<'static>,
        value: T,
    ) -> Self {
        Self::new_with_storage_with_caller(world, caller, value)
    }

    #[track_caller]
    pub fn new(world: &mut World, value: T) -> Self {
        Self::new_with_caller(world, Location::caller(), value)
    }
}

impl<T: Send + Sync + 'static, S: Storage<Value = T>> Value<T, S> {
    pub fn new_with_storage_with_caller(
        world: &mut World,
        caller: &'static Location<'static>,
        value: T,
    ) -> Self {
        let erased = ValueErased::new_with_caller(world, caller, S::new(value));
        Value(erased, PhantomData)
    }

    #[track_caller]
    pub fn new_with_storage(world: &mut World, value: T) -> Self {
        Self::new_with_storage_with_caller(world, Location::caller(), value)
    }

    pub fn try_read(&self, world: DeferredWorld) -> Result<S::Ref, ValueReadError<S, T>> {
        let storage = self.0.try_get::<S>(world)?;
        storage
            .try_read()
            .map_err(|e| ValueReadError::StorageError(e))
    }

    pub fn try_write(&self, world: DeferredWorld) -> Result<S::RefMut, ValueWriteError<S, T>> {
        let storage = self.0.try_get::<S>(world)?;
        storage
            .try_write()
            .map_err(|e| ValueWriteError::StorageError(e))
    }

    pub fn read(&self, world: DeferredWorld) -> S::Ref {
        self.try_read(world).unwrap_or_else(|err| {
            panic!(
                "Failed to read value of type {}: {}",
                std::any::type_name::<T>(),
                err
            )
        })
    }

    pub fn write(&self, world: DeferredWorld) -> S::RefMut {
        self.try_write(world).unwrap_or_else(|err| {
            panic!(
                "Failed to write value of type {}: {}",
                std::any::type_name::<T>(),
                err
            )
        })
    }
}
