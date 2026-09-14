use std::{collections::HashSet, panic::Location};

use bevy::{
    ecs::{
        entity::Entity,
        hierarchy::{ChildOf, Children},
        query::{Has, Or, With},
        system::{Commands, Query},
        world::{self, World},
    },
    log::debug,
    platform::collections::HashMap,
    prelude::{Deref, DerefMut},
};
use caaqi_context::Attached;
use smallvec::SmallVec;

use crate::{
    action::{
        execute::{ExecuteActionTrees, run_action_node},
        node::{ActionLocation, ActionNode, ActionRewind, NeedsRun, Rewound, SubscribedTo},
        sync::SyncKey,
    },
    tracked_value::{RefInitLocation, RefValue},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deref, DerefMut)]
pub struct ActionEntity(Entity);

#[track_caller]
pub fn detached_action(
    world: &mut World,
    action: impl FnMut(&mut World) + Send + Sync + 'static,
) -> ActionEntity {
    ActionEntity(
        world
            .spawn((
                ActionNode(Box::new(action)),
                ActionLocation(Location::caller()),
                NeedsRun,
                Rewound,
            ))
            .id(),
    )
}

#[track_caller]
pub fn action(world: &mut World, action: impl FnMut(&mut World) + Send + Sync + 'static) {
    let node = detached_action(world, action);
    Attached::attach(&mut *world, node);
    run_action_node(world, *node);
}

pub fn defer_action_eval(
    mut commands: Commands,
    action: impl FnMut(&mut World) + Send + Sync + 'static,
) {
    let node = ActionNode(Box::new(action));

    commands.spawn((node, NeedsRun, Rewound));

    commands.queue(ExecuteActionTrees);
}

pub fn rewind(world: &mut World, undo: impl FnOnce(&mut World) + Send + Sync + 'static) {
    synced_rewind(world, std::iter::empty(), undo);
}

pub fn synced_rewind(
    world: &mut World,
    keys: impl IntoIterator<Item = SyncKey>,
    undo: impl FnOnce(&mut World) + Send + Sync + 'static,
) {
    let node = ActionEntity(world.spawn(ActionRewind::new(undo)).id());
    Attached::attach(&mut *world, node);

    for key in keys {
        Attached::attach(&mut *world, key);
    }
}
