use bevy::{
    ecs::{entity::Entity, resource::Resource, world::World},
    platform::collections::HashMap,
    prelude::{Deref, DerefMut},
};
use caaqi_context::Attached;

use crate::action::{
    context_builder::{ActionEntity, action, rewind},
    node::ActionRewind,
};

#[derive(Debug, Deref, PartialEq, Eq, Hash, Clone, Copy)]
pub struct SyncKey(pub Entity);

#[derive(Debug, Deref, DerefMut, Default, Resource)]
pub struct SyncKeyToActions(HashMap<SyncKey, Vec<Entity>>);

pub fn sync_key(world: &mut World) -> SyncKey {
    let entity = world.spawn_empty().id();
    let key = SyncKey(entity);
    world
        .resource_mut::<SyncKeyToActions>()
        .0
        .insert(key, Vec::new());

    rewind(world, move |world| {
        let remaining = world.resource_mut::<SyncKeyToActions>().0.remove(&key);

        debug_assert!(
            remaining.as_ref().is_some_and(Vec::is_empty),
            "SyncKey was destroyed with registered synced rewinds"
        );

        world.despawn(entity);
    });

    key
}
