use std::any::Any;

use bevy::ecs::component::Component;

use crate::tracked_value::RefTypeErased;

#[derive(Component)]
pub struct ActionNode(pub Box<dyn FnMut() + Send + Sync + 'static>);

#[derive(Component)]
pub struct ActionRewind(pub Box<dyn FnOnce() + Send + Sync + 'static>);
