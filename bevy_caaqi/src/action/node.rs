use std::collections::HashSet;

use bevy::{
    ecs::{
        component::Component,
        entity::Entity,
        lifecycle::HookContext,
        world::{DeferredWorld, FromWorld, World},
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
#[component(on_remove = Self::on_remove)]
pub struct SyncedWith(pub HashSet<SyncKey>);

impl SyncedWith {
    fn on_remove(mut world: DeferredWorld, context: HookContext) {
        let keys = world
            .get::<SyncedWith>(context.entity)
            .unwrap()
            .iter()
            .copied()
            .collect::<Vec<_>>();

        let mut index = world.resource_mut::<SyncKeyToActions>();

        for key in keys {
            index
                .get_mut(&key)
                .expect("SyncKey was destroyed before its action")
                .remove(&context.entity);
        }
    }
}

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
