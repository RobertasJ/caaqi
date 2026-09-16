use std::hash::{Hash, Hasher};
use std::panic::Location;

use bevy::ecs::world::{DeferredWorld, World};

use crate::tracking::subscribe;
use crate::{
    action::{
        context_builder::{rewind, synced_rewind},
        sync::{SyncKey, create_sync_key, drop_sync_key, sync_point},
    },
    state::State,
    tracking::{TrackingKey, notify, notify_with_caller},
    value::{AtomicRefCellStorage, Storage, Value},
};

pub struct Var<T: Clone + Send + Sync + 'static> {
    value: Value<T>,
    sync_key: SyncKey,
    tracking_key: TrackingKey,
}

impl<T: Clone + Send + Sync + 'static> Clone for Var<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Clone + Send + Sync + 'static> Copy for Var<T> {}

impl<T: Clone + Send + Sync + 'static> PartialEq for Var<T> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl<T: Clone + Send + Sync + 'static> Eq for Var<T> {}

impl<T: Clone + Send + Sync + 'static> Hash for Var<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.value.hash(state);
    }
}

pub fn var<T: Clone + Send + Sync + 'static>(world: &mut World, value: T) -> Var<T> {
    let var = Var::new(world, value);

    rewind(world, move |world| {
        var.remove(world);
    });

    var
}

impl<T: Clone + Send + Sync + 'static> Var<T> {
    pub fn new_with_caller(
        world: &mut World,
        caller: &'static Location<'static>,
        value: T,
    ) -> Self {
        let value = Value::new_with_caller(world, caller, value);
        let sync_key = create_sync_key(world);
        let tracking_key = TrackingKey::new(world);
        Var {
            value,
            sync_key,
            tracking_key,
        }
    }

    #[track_caller]
    pub fn new(world: &mut World, value: T) -> Self {
        Self::new_with_caller(world, Location::caller(), value)
    }

    pub fn remove(self, world: &mut World) {
        self.value.remove(world);
        drop_sync_key(world, self.sync_key);
        self.tracking_key.remove(world);
    }

    #[track_caller]
    pub fn read(&self, world: &mut World) -> <AtomicRefCellStorage<T> as Storage>::Ref {
        sync_point(world, [self.sync_key]);
        subscribe(world.into(), self.tracking_key);
        self.value.read(world.into())
    }

    #[track_caller]
    pub fn write(&mut self, world: &mut World) -> <AtomicRefCellStorage<T> as Storage>::RefMut {
        sync_point(world, [self.sync_key]);

        subscribe(world.into(), self.tracking_key);
        let old = self.value.read(world.into()).clone();

        let mut value = self.value;
        let tracking_key = self.tracking_key;
        let caller = std::panic::Location::caller();

        synced_rewind(world, [self.sync_key], move |world| {
            *value.write(world.into()) = old;
            notify_with_caller(world.into(), caller, tracking_key, true);
        });

        notify(world.into(), tracking_key, true);
        self.value.write(world.into())
    }
}
