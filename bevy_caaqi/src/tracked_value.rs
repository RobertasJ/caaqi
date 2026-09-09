use bevy::ecs::{component::Component, entity::Entity, world::World};
use std::any::Any;

use crate::world_context::WorldContext;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ref<T>(Entity, std::marker::PhantomData<T>);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefTypeErased(Entity);

#[derive(Component)]
struct RefValue(Box<dyn Any + Send + Sync + 'static>);

pub fn create_ref<T: Any + Send + Sync + 'static>(value: T) -> Ref<T> {
    WorldContext::with(|ctx| {
        let entity = ctx.spawn((RefValue(Box::new(value)),)).id();
        Ref(entity, std::marker::PhantomData)
    })
}

pub fn remove_ref<T: Any + Send + Sync + 'static>(ref_: Ref<T>) {
    WorldContext::with(|ctx| {
        ctx.despawn(ref_.0);
    });
}
