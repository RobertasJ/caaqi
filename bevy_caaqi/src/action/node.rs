use std::any::Any;

use bevy::ecs::component::Component;

#[derive(Component)]
pub struct ActionNode(
    pub Box<dyn FnMut() -> Box<dyn Any + Send + Sync + 'static> + Send + Sync + 'static>,
);

#[derive(Component)]
pub struct ActionRewind(pub Box<dyn FnOnce() + Send + Sync + 'static>);
