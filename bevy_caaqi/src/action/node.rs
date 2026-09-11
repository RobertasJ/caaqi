use bevy::ecs::{component::Component, entity::Entity};
use caaqi_context::DetachedNode;
use smallvec::SmallVec;

#[derive(Component)]
pub struct ActionNode(pub Box<dyn FnMut() + Send + Sync + 'static>);

// #[derive(Component)]
// pub struct ActionRewind(pub Box<dyn FnOnce() + Send + Sync + 'static>);

#[derive(Component)]
pub struct NeedsRerun;
