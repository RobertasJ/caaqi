use bevy::{
    ecs::{entity::Entity, resource::Resource, world::World},
    platform::collections::{HashMap, HashSet},
    prelude::{Deref, DerefMut},
};

use crate::action::context_builder::rewind;

#[derive(Debug, Deref, PartialEq, Eq, Hash, Clone, Copy)]
pub struct SyncKey(pub Entity);

#[derive(Debug, Deref, DerefMut, Default, Resource)]
pub struct SyncKeyToActions(HashMap<SyncKey, HashSet<Entity>>);

pub fn sync_key(world: &mut World) -> SyncKey {
    let key = create_sync_key(world);

    rewind(world, move |world| {
        remove_sync_key(world, key);
    });

    key
}

pub fn create_sync_key(world: &mut World) -> SyncKey {
    let entity = world.spawn_empty().id();
    let key = SyncKey(entity);
    world
        .resource_mut::<SyncKeyToActions>()
        .0
        .insert(key, HashSet::new());
    key
}

pub fn remove_sync_key(world: &mut World, key: SyncKey) {
    let remaining = world.resource_mut::<SyncKeyToActions>().0.remove(&key);

    debug_assert!(
        remaining.as_ref().is_some_and(HashSet::is_empty),
        "synckey was destroyed with registered synced rewinds"
    );

    world.despawn(key.0);
}
