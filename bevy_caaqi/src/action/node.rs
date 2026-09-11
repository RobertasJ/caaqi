use bevy::{
    ecs::{component::Component, entity::Entity, world::FromWorld},
    ui::DefaultUiCamera,
};

#[derive(Component)]
#[require(NeedsRerun)]
pub struct ActionNode(pub Box<dyn FnMut() + Send + Sync + 'static>);

// #[derive(Component)]
// pub struct ActionRewind(pub Box<dyn FnOnce() + Send + Sync + 'static>);

#[derive(Component)]
pub struct NeedsRerun(pub bool);

impl Default for NeedsRerun {
    fn default() -> Self {
        Self(false)
    }
}
