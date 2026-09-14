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
use smallvec::SmallVec;

use crate::{
    action::sync::{SyncKey, SyncKeyToActions},
    tracked_value::RefTypeErased,
};

#[derive(Component)]
#[require(SubscribedTo, SyncedWith, TreeNode)]
pub struct ActionNode(pub Box<dyn FnMut(&mut World) + Send + Sync + 'static>);

#[derive(Component, Debug, Default, Deref, DerefMut)]
pub struct SubscribedTo(pub HashSet<RefTypeErased>);

#[derive(Component, Debug, Default, Deref, DerefMut)]
pub struct SyncedWith(pub HashSet<SyncKey>);

#[derive(Component, Debug, Default)]
pub struct NeedsRun;

#[derive(Component, Debug, Default)]
pub struct Rewound;

#[derive(Debug, Clone, Copy, Component, Deref, DerefMut)]
pub struct ActionLocation(pub &'static std::panic::Location<'static>);

#[derive(Component)]
#[require(TreeNode)]
pub struct ActionRewind {
    undo: Box<dyn FnOnce(&mut World) + Send + Sync + 'static>,
}

impl ActionRewind {
    pub fn new(undo: impl FnOnce(&mut World) + Send + Sync + 'static) -> Self {
        Self {
            undo: Box::new(undo),
        }
    }

    pub fn run(world: &mut World, node: Entity) {
        let rewind = world
            .entity_mut(node)
            .take::<ActionRewind>()
            .expect("Node is not an ActionRewind");

        (rewind.undo)(world);

        world.despawn(node);
    }
}

#[derive(Default, Component)]
pub struct TreeNode;
