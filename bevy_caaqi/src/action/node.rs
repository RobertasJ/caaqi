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
    action::sync::{SyncKey, SyncKeyToRewinds},
    tracked_value::RefTypeErased,
};

#[derive(Component)]
#[require(Deps, TreeNode)]
pub struct ActionNode(pub Box<dyn FnMut(&mut World) + Send + Sync + 'static>);

#[derive(Component, Debug, Default, Deref, DerefMut)]
pub struct Deps(pub HashSet<RefTypeErased>);

#[derive(Component, Debug, Default)]
pub struct Stale;

#[derive(Debug, Clone, Component, Deref, DerefMut)]
pub struct ActionLocation(pub &'static std::panic::Location<'static>);

#[derive(Component)]
#[require(TreeNode)]
pub struct ActionRewind {
    undo: Box<dyn FnOnce(&mut World) + Send + Sync + 'static>,
    sync_keys: SmallVec<[SyncKey; 1]>,
}

impl ActionRewind {
    pub fn new(
        undo: impl FnOnce(&mut World) + Send + Sync + 'static,
        sync_keys: SmallVec<[SyncKey; 1]>,
    ) -> Self {
        Self {
            undo: Box::new(undo),
            sync_keys,
        }
    }

    pub fn run(world: &mut World, node: Entity) {
        let rewind = world
            .entity_mut(node)
            .take::<ActionRewind>()
            .expect("Node is not an ActionRewind");

        (rewind.undo)(world);

        let mut sync_keys_to_rewinds = world.resource_mut::<SyncKeyToRewinds>();

        for key in rewind.sync_keys {
            let rewinds = sync_keys_to_rewinds
                .get_mut(&key)
                .expect("SyncKey was destroyed before the rewind could be run");

            let last = rewinds
                .pop()
                .expect("Expected a rewind to be registered for the SyncKey");
            assert!(
                last == node,
                "Rewinds for a SyncKey must be run in reverse order of registration"
            );
        }

        world.despawn(node);
    }
}

#[derive(Default, Component)]
pub struct TreeNode;
