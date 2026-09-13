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

use crate::tracked_value::RefTypeErased;

#[derive(Component)]
#[require(Deps)]
pub struct ActionNode(pub Box<dyn FnMut(&mut World) + Send + Sync + 'static>);

#[derive(Component, Debug, Default, Deref, DerefMut)]
pub struct Deps(pub HashSet<RefTypeErased>);

#[derive(Component, Debug, Default)]
pub struct Stale;

#[derive(Debug, Clone, Component, Deref, DerefMut)]
pub struct ActionLocation(pub &'static std::panic::Location<'static>);

#[derive(Component)]
pub struct ActionRewind(pub Box<dyn FnOnce(&mut World) + Send + Sync + 'static>);

// #[derive(Component, Debug, Default)]
// pub struct SyncKeys(pub Vec<SyncKey>);

// #[derive(Debug, Deref, PartialEq, Eq)]
// pub struct SyncKey(Entity);
