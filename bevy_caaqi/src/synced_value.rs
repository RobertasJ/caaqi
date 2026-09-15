use bevy::ecs::world::{DeferredWorld, World};

use crate::{
    action::{
        context_builder::{rewind, synced_rewind},
        sync::{SyncKey, create_sync_key, drop_sync_key, sync_point},
    },
    tracked_value::{ReadRef, Ref, WriteRef, create_ref, drop_ref},
};

#[derive(Debug, PartialEq, Eq)]
pub struct SyncedRef<T: Clone + Send + Sync + 'static> {
    ref_: Ref<T>,
    sync_key: SyncKey,
}

impl<T: Clone + Send + Sync + 'static> Clone for SyncedRef<T> {
    fn clone(&self) -> Self {
        Self {
            ref_: self.ref_.clone(),
            sync_key: self.sync_key,
        }
    }
}

impl<T: Clone + Send + Sync + 'static> Copy for SyncedRef<T> {}

pub fn create_synced_ref<T: Clone + Send + Sync + 'static>(
    world: &mut World,
    value: T,
) -> SyncedRef<T> {
    let sync_key = create_sync_key(world);
    let ref_ = create_ref(world, value);

    SyncedRef { ref_, sync_key }
}

pub fn drop_synced_ref<T: Clone + Send + Sync + 'static>(
    world: &mut World,
    tracked_ref: SyncedRef<T>,
) {
    let SyncedRef { ref_, sync_key } = tracked_ref;

    drop_ref(world, ref_);
    drop_sync_key(world, sync_key);
}

pub fn synced_ref<T: Clone + Send + Sync + 'static>(world: &mut World, value: T) -> SyncedRef<T> {
    let synced_ref = create_synced_ref(world, value);

    rewind(world, move |world| {
        drop_synced_ref(world, synced_ref);
    });

    synced_ref
}

impl<T: Clone + Send + Sync + 'static> SyncedRef<T> {
    #[track_caller]
    pub fn read(&self, world: &mut World) -> ReadRef<T> {
        sync_point(world, [self.sync_key]);
        self.ref_.read(&mut *world)
    }

    #[track_caller]
    pub fn write(&mut self, world: &mut World) -> WriteRef<T> {
        sync_point(world, [self.sync_key]);
        let old = self.ref_.read(&mut *world).clone();
        let mut ref_ = self.ref_;
        let caller = std::panic::Location::caller();
        synced_rewind(world, [self.sync_key], move |world| {
            *ref_.silent_write(&mut *world) = old;
            ref_.notify_forward_only_with_caller(&mut *world, caller);
        });

        self.ref_.notify_forward_only(&mut *world);
        self.ref_.silent_write(world)
    }
}
