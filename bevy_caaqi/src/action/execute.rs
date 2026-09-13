use std::{collections::HashSet, ops::Deref};

use crate::{
    action::{
        context_builder::ActionEntity,
        node::{ActionLocation, ActionRewind, Deps, Stale},
    },
    tracked_value::{RefInitLocation, RefRead, RefTypeErased, WriteLocations, WrittenTo},
};
use bevy::{
    ecs::{
        entity::Entity,
        hierarchy::{ChildOf, Children},
        query::{Has, Or, QueryState, With, Without},
        resource::Resource,
        system::{Command, Query},
        world::{EntityMut, EntityWorldMut, World},
    },
    log::debug,
    platform::collections::HashMap,
    prelude::{Deref, DerefMut},
};
use caaqi_context::Scope;

use crate::action::node::{self, ActionNode};

pub struct ExecuteActionTrees;

impl Command for ExecuteActionTrees {
    type Out = ();

    fn apply(self, world: &mut World) -> Self::Out {
        let root_nodes = world
            .query_filtered::<Entity, (With<ActionNode>, Without<ChildOf>)>()
            .iter(world)
            .collect::<Vec<_>>();

        fn rec(
            node: Entity,
            world: &mut World,
            tree_query_state: &mut QueryState<&Children, With<ActionNode>>,
        ) {
            if world.get::<Stale>(node).is_some() && world.get::<ActionNode>(node).is_some() {
                world.entity_mut(node).remove::<Stale>();
                run_rewinds(world, node);
                run_action_node(world, node);
                flush_tracked_writes(world);
            } else {
                let children = tree_query_state
                    .query(world)
                    .get(node)
                    .map(|c| c.to_vec())
                    .unwrap_or_default();

                for child in children {
                    rec(child, world, tree_query_state);
                }
            }
        }

        let mut tree_query_state = world.query_filtered::<&Children, With<ActionNode>>();
        for root in root_nodes {
            rec(root, world, &mut tree_query_state);
        }

        let has_stale_nodes = world
            .query_filtered::<Entity, (With<Stale>, With<ActionNode>)>()
            .iter(world)
            .next()
            .is_some();

        if has_stale_nodes {
            world.commands().queue(ExecuteActionTrees);
        }
    }
}

pub struct FlushWrites;

impl Command for FlushWrites {
    type Out = ();

    fn apply(self, world: &mut World) -> Self::Out {
        flush_tracked_writes(world);
    }
}

fn run_rewinds(world: &mut World, node: Entity) {
    let mut rewinds = world.query_filtered::<(
        Entity,
        Option<&Children>,
        Has<ActionRewind>,
    ), Or<(With<ActionRewind>, With<ActionNode>)>>();
    let rewinds = rewinds.query(world);

    let mut rewinds_to_run = vec![];

    fn rec(
        node: Entity,
        rewinds: &Query<
            (Entity, Option<&Children>, Has<ActionRewind>),
            Or<(With<ActionRewind>, With<ActionNode>)>,
        >,
        rewinds_to_run: &mut Vec<Entity>,
    ) {
        let (_, node_children, _) = rewinds.get(node).unwrap();
        if let Some(children) = node_children {
            for child in children {
                let (child_entity, _, has_rewind) = rewinds.get(*child).unwrap();
                if has_rewind {
                    rewinds_to_run.push(child_entity);
                } else {
                    rec(child_entity, rewinds, rewinds_to_run);
                }
            }
        }
    }

    rec(node, &rewinds, &mut rewinds_to_run);

    rewinds_to_run.reverse();

    for rewind in rewinds_to_run {
        let mut entity_mut = world.entity_mut(rewind);
        let action_rewind = entity_mut.take::<ActionRewind>().unwrap();
        (action_rewind.0)(world);
    }
}

pub fn run_action_node(world: &mut World, node: Entity) {
    let mut entity_mut = world.entity_mut(node);
    entity_mut.despawn_children();

    let mut action_node = if let Some(action_node) = world.entity_mut(node).take::<ActionNode>() {
        action_node
    } else {
        panic!("entity {:?} is not an ActionNode", node);
    };

    let dependencies_scope = Scope::<RefRead>::new(&mut *world);
    let action_entities_scope = Scope::<ActionEntity>::new(&mut *world);

    (action_node.0)(world);

    let sub_actions_or_rewinds = action_entities_scope.collect(&mut *world);
    let depends_on = dependencies_scope.collect(&mut *world);

    debug!(
        "[execute_action_tree] Node {:?} at {} depends on {} Refs at:\n{}\n and has {} sub-actions or rewinds.",
        node,
        world
            .get::<ActionLocation>(node)
            .map(|l| l.to_string())
            .unwrap_or("Not Given".to_string()),
        depends_on.len(),
        depends_on
            .iter()
            .map(|r| format!("\t{}", r.1.to_string()))
            .collect::<Vec<_>>()
            .join("\n"),
        depends_on.len(),
    );

    let mut entity_mut = world.entity_mut(node);
    entity_mut.insert(action_node);
    entity_mut.add_children(
        &sub_actions_or_rewinds
            .into_iter()
            .map(|ae| *ae)
            .collect::<Vec<_>>(),
    );

    entity_mut.insert(Deps(depends_on.into_iter().map(|r| r.0).collect()));
}

pub fn flush_tracked_writes(world: &mut World) {
    let written_to = world
        .get_resource_mut::<WrittenTo>()
        .unwrap()
        .drain()
        .collect::<HashSet<_>>();

    debug!(
        "[execute_action_tree] Flushing tracked writes. Tracked writes to {} Refs.",
        written_to.len()
    );

    let mut affected_nodes = vec![];
    let mut action_nodes =
        world.query_filtered::<(Entity, &Deps, &ActionLocation), With<ActionNode>>();

    for (node, deps, location) in action_nodes.iter_mut(world) {
        let affected = !deps.is_disjoint(&written_to);

        if affected {
            affected_nodes.push((node, location.0));
        }
    }

    debug!(
        "[execute_action_tree] Found {} affected nodes:\n{}",
        affected_nodes.len(),
        affected_nodes
            .iter()
            .map(|(node, location)| format!("\tNode {:?} at {}", node, location))
            .collect::<Vec<_>>()
            .join("\n")
    );

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
