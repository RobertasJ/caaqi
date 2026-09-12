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
        node::{ActionLocation, ActionNode, ActionRewind, Deps, Stale},
    },
    tracked_value::{RefInitLocation, RefValue, SubscribeScope, WriteLocations, WrittenTo},
};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ActionScope;

impl ScopeKind for ActionScope {}

#[track_caller]
pub fn detached_action(
    world: &mut World,
    action: impl FnMut(&mut World) + Send + Sync + 'static,
) -> DetachedNode<ActionScope> {
    DetachedNode::from_entity(
        world
            .spawn((
                ActionNode(Box::new(action)),
                ActionLocation(Location::caller()),
            ))
            .id(),
    )
    .into()
}

#[track_caller]
pub fn action(world: &mut World, action: impl FnMut(&mut World) + Send + Sync + 'static) {
    let node = detached_action(world, action);
    attach_node::<ActionScope>(&mut *world, node);
    run_action_node(world, *node);
}

pub fn flush_tracked_writes(world: &mut World) {
    let written_to = world
        .get_resource_mut::<WrittenTo>()
        .unwrap()
        .drain()
        .collect::<HashSet<_>>();

    let mut affected_nodes = vec![];
    let mut action_nodes =
        world.query_filtered::<(Entity, &Deps, &ActionLocation), With<ActionNode>>();

    for (node, deps, location) in action_nodes.iter_mut(world) {
        let affected = !deps.is_disjoint(&written_to);

        if affected {
            affected_nodes.push((node, location.0));
        }
    }

    for (affected, location) in &affected_nodes {
        world.entity_mut(*affected).insert(Stale);
    }

    for (ref_, write_locations) in world
        .get_resource_mut::<WriteLocations>()
        .unwrap()
        .drain()
        .collect::<HashMap<_, Vec<_>>>()
    {
        let ref_location = *world
            .get::<RefInitLocation>(ref_)
            .expect("the Ref has been deallocated");

        debug!(
            "[execute_action_tree] Ref at location {} was written to at locations:\n{}",
            *ref_location,
            write_locations
                .iter()
                .map(|l| format!("\t{}", l))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}

pub fn action_root(action_root: impl FnMut(&mut World) + Send + Sync + 'static) -> ActionNode {
    ActionNode(Box::new(action_root))
}

pub fn defer_action_eval(
    mut commands: Commands,
    action: impl FnMut(&mut World) + Send + Sync + 'static,
) {
    let node = action_root(action);

    let node = commands.spawn((node, Stale)).id();

    commands.queue(ExecuteActionTrees);
}

pub fn rewind(world: &mut World, undo: impl FnOnce(&mut World) + Send + Sync + 'static) {
    let node = DetachedNode::from_entity(world.spawn(ActionRewind(Box::new(undo))).id());
    attach_node::<ActionScope>(&mut *world, node);
}
