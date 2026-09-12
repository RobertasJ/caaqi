use std::collections::HashSet;

use bevy::{
    ecs::{
        component::Component,
        entity::Entity,
        world::{FromWorld, World},
    },
    prelude::{Deref, DerefMut},
    ui::DefaultUiCamera,
};

#[derive(Component)]
#[require(Deps)]
pub struct ActionNode(pub Box<dyn FnMut(&mut World) + Send + Sync + 'static>);

#[derive(Component, Debug, Default, Deref, DerefMut)]
pub struct Deps(pub HashSet<Entity>);

#[derive(Component, Debug, Default)]
pub struct Stale;

// #[derive(Component)]
// pub struct ActionRewind(pub Box<dyn FnOnce() + Send + Sync + 'static>);
