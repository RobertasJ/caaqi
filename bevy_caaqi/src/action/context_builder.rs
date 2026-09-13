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
};
use caaqi_context::{DetachedNode, ScopeKind, attach_node, collect_in_scope};

use crate::{
    action::{
        execute::{ExecuteActionTrees, run_action_node},
        node::{ActionLocation, ActionNode, ActionRewind, Deps, Stale, SyncKey, SyncKeys},
    },
    tracked_value::{RefInitLocation, RefValue, SubscribeScope, WriteLocations, WrittenTo},
};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ActionScope;

impl ScopeKind for ActionScope {}

#[track_caller]
pub fn detached_action(
    world: &mut World,
    sync_keys: impl IntoIterator<Item = SyncKey>,
    action: impl FnMut(&mut World) + Send + Sync + 'static,
) -> DetachedNode<ActionScope> {
    DetachedNode::from_entity(
        world
            .spawn((
                ActionNode(Box::new(action)),
                ActionLocation(Location::caller()),
                SyncKeys(sync_keys.into_iter().collect()),
            ))
            .id(),
    )
    .into()
}

#[track_caller]
pub fn action(world: &mut World, action: impl FnMut(&mut World) + Send + Sync + 'static) {
    let node = detached_action(world, [], action);
    attach_node::<ActionScope>(&mut *world, node);
    run_action_node(world, *node);
}

#[track_caller]
pub fn synced_action(
    world: &mut World,
    sync_keys: impl IntoIterator<Item = SyncKey>,
    action: impl FnMut(&mut World) + Send + Sync + 'static,
) {
    let node = detached_action(world, sync_keys, action);
    attach_node::<ActionScope>(&mut *world, node);
    run_action_node(world, *node);
}

pub fn defer_action_eval(
    mut commands: Commands,
    action: impl FnMut(&mut World) + Send + Sync + 'static,
) {
    let node = ActionNode(Box::new(action));

    let node = commands.spawn((node, Stale)).id();

    commands.queue(ExecuteActionTrees);
}

pub fn rewind(world: &mut World, undo: impl FnOnce(&mut World) + Send + Sync + 'static) {
    let node = DetachedNode::from_entity(world.spawn(ActionRewind(Box::new(undo))).id());
    attach_node::<ActionScope>(&mut *world, node);
}
