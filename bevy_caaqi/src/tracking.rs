use bevy::ecs::{
    entity::Entity,
    world::{DeferredWorld, World},
};
use caaqi_context::Attached;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TrackingKey(Entity);

impl TrackingKey {
    pub fn new(world: &mut World) -> Self {
        let entity = world.spawn_empty().id();
        TrackingKey(entity)
    }

    pub fn remove(self, world: &mut World) {
        world.despawn(self.0);
    }
}

pub struct Subscribe {
    pub key: TrackingKey,
    pub location: &'static std::panic::Location<'static>,
}

pub struct Notify {
    pub key: TrackingKey,
    pub location: &'static std::panic::Location<'static>,
    pub forward_only: bool,
}

pub fn notify_with_caller(
    world: DeferredWorld,
    caller: &'static std::panic::Location<'static>,
    key: TrackingKey,
    forward_only: bool,
) {
    Attached::attach(
        world,
        Notify {
            key,
            location: caller,
            forward_only,
        },
    );
}

#[track_caller]
pub fn notify(world: DeferredWorld, key: TrackingKey, forward_only: bool) {
    notify_with_caller(world, std::panic::Location::caller(), key, forward_only);
}

pub fn subscribe_with_caller(
    world: DeferredWorld,
    caller: &'static std::panic::Location<'static>,
    key: TrackingKey,
) {
    Attached::attach(
        world,
        Subscribe {
            key,
            location: caller,
        },
    );
}

#[track_caller]
pub fn subscribe(world: DeferredWorld, key: TrackingKey) {
    subscribe_with_caller(world, std::panic::Location::caller(), key);
}
