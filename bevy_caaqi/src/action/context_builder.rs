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
        execute::ExecuteActionTrees,
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

pub fn run_action_node(world: &mut World, node: Entity) {
    let mut entity_mut = world.entity_mut(node);
    entity_mut.despawn_children();

    let mut rewinds = world
        .query_filtered::<(Entity, &Children, &ChildOf, Has<ActionRewind>), Or<(With<ActionRewind>, With<ActionNode>)>>();
    let rewinds = rewinds.query(world);

    let mut rewinds_to_run = vec![];

    fn rec(
        node: Entity,
        rewinds: &Query<
            (Entity, &Children, &ChildOf, Has<ActionRewind>),
            Or<(With<ActionRewind>, With<ActionNode>)>,
        >,
        rewinds_to_run: &mut Vec<Entity>,
    ) {
        for child in rewinds.get(node).unwrap().1 {
            let (child_entity, _, _, has_rewind) = rewinds.get(*child).unwrap();
            if has_rewind {
                rewinds_to_run.push(child_entity);
            } else {
                rec(child_entity, rewinds, rewinds_to_run);
            }
        }
    }

    rec(node, &rewinds, &mut rewinds_to_run);

    let mut action_node = if let Some(action_node) = world.entity_mut(node).take::<ActionNode>() {
        action_node
    } else {
        panic!("entity {:?} is not an ActionNode", node);
    };

    let (action_result, depends_on) = collect_in_scope::<SubscribeScope, _>(world, |world| {
        collect_in_scope::<ActionScope, _>(world, |world| {
            (action_node.0)(world);
        })
    });

    let (_, sub_actions) = action_result;

    let mut entity_mut = world.entity_mut(node);
    entity_mut.insert(action_node);
    entity_mut.add_children(&sub_actions);

    entity_mut.insert(Deps(depends_on.into_iter().collect()));

    flush_tracked_writes(world);
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
