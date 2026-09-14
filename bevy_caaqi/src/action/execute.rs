use std::{collections::HashSet, ops::Deref};

use crate::{
    action::{
        self,
        context_builder::ActionEntity,
        node::{
            ActionLocation, ActionRewind, NeedsRun, Rewound, SubscribedTo, SyncedWith, TreeNode,
        },
        sync::{SyncKey, SyncKeyToActions},
        tree_order::{self, TreeOrder},
    },
    tracked_value::{RefInitLocation, RefNotify, RefSubscribe, RefTypeErased},
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
use caaqi_context::{Attached, Scope};

use crate::action::node::{self, ActionNode};

pub struct ExecuteActionTrees;

impl Command for ExecuteActionTrees {
    type Out = ();

    fn apply(self, world: &mut World) -> Self::Out {
        let root_nodes = world
            .query_filtered::<Entity, (With<ActionNode>, Without<ChildOf>)>()
            .iter(world)
            .collect::<Vec<_>>();

        fn traverse_tree(
            root: Entity,
            node: Entity,
            world: &mut World,
            tree_query_state: &mut QueryState<&Children, With<ActionNode>>,
        ) {
            if world.get::<NeedsRun>(node).is_some() && world.get::<ActionNode>(node).is_some() {
                if world.get::<Rewound>(node).is_some() {
                    world.entity_mut(node).remove::<Rewound>();
                } else {
                    rewind_action(world, node, root);
                }
                world.entity_mut(node).remove::<NeedsRun>();
                run_action_node(world, node);
            } else {
                let children = tree_query_state
                    .query(world)
                    .get(node)
                    .map(|c| c.to_vec())
                    .unwrap_or_default();

                for child in children {
                    traverse_tree(root, child, world, tree_query_state);
                }
            }
        }

        let mut tree_query_state = world.query_filtered::<&Children, With<ActionNode>>();
        for root in root_nodes {
            traverse_tree(root, root, world, &mut tree_query_state);
        }

        let has_stale_nodes = world
            .query_filtered::<Entity, (With<NeedsRun>, With<ActionNode>)>()
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

fn rewind_action(world: &mut World, node: Entity, tree_root: Entity) {
    let mut to_rewind = HashSet::<Entity>::new();
    let sync_key_to_actions = world.resource::<SyncKeyToActions>();
    let tree_order = TreeOrder::new(world, tree_root);

    fn rec(
        world: &World,
        node: Entity,
        to_rewind: &mut HashSet<Entity>,
        sync_key_to_actions: &SyncKeyToActions,
        tree_order: &TreeOrder,
    ) {
        if !to_rewind.insert(node) {
            return;
        }

        let sync_keys = world.get::<SyncedWith>(node).unwrap();

        for sync_key in &**sync_keys {
            let actions = sync_key_to_actions
                .get(sync_key)
                .expect("SyncKey was destroyed with registered synced rewinds");
            for tree_node in tree_order.backward() {
                if tree_node == node {
                    break;
                }

                if actions.contains(&tree_node) {
                    rec(world, tree_node, to_rewind, sync_key_to_actions, tree_order);
                }
            }
        }
    }

    rec(
        world,
        node,
        &mut to_rewind,
        sync_key_to_actions,
        &tree_order,
    );

    debug!(
        "[rewind_action] Rewinding {} nodes for node {:?} at {}",
        to_rewind.len(),
        node,
        world
            .get::<ActionLocation>(node)
            .map(|l| l.to_string())
            .unwrap_or("Not Given".to_string()),
    );

    fn rewind_node(world: &mut World, node: Entity) {
        let order = TreeOrder::new(world, node);

        for entity in order.forward() {
            if world.get::<SyncedWith>(entity).is_some() {
                world.entity_mut(entity).remove::<SyncedWith>();
            }
        }

        for entity in order.backward() {
            if world.get::<ActionRewind>(entity).is_some() {
                ActionRewind::run(world, entity);
            }
        }

        let mut entity_mut = world.entity_mut(node);
        entity_mut.despawn_children();
    }

    fn traverse_actions_rev(
        world: &mut World,
        node: Entity,
        to_rewind: &HashSet<Entity>,
        tree_query_state: &mut QueryState<&Children, With<ActionNode>>,
    ) {
        if to_rewind.contains(&node) {
            if world.get::<Rewound>(node).is_none() {
                rewind_node(world, node);
            }

            world.entity_mut(node).insert(Rewound).insert(NeedsRun);
        } else {
            let children = tree_query_state
                .query(world)
                .get(node)
                .map(|c| c.to_vec())
                .unwrap_or_default();

            for child in children.iter().rev() {
                traverse_actions_rev(world, *child, to_rewind, tree_query_state);
            }
        }
    }

    let mut tree_query_state = world.query_filtered::<&Children, With<ActionNode>>();
    traverse_actions_rev(world, tree_root, &to_rewind, &mut tree_query_state);
}

pub fn run_action_node(world: &mut World, node: Entity) {
    let mut action_node = if let Some(action_node) = world.entity_mut(node).take::<ActionNode>() {
        action_node
    } else {
        panic!("entity {:?} is not an ActionNode", node);
    };

    let dependencies_scope = Scope::<RefSubscribe>::new(&mut *world);
    let action_entities_scope = Scope::<ActionEntity>::new(&mut *world);
    let sync_keys_scope = Scope::<SyncKey>::new(&mut *world);

    (action_node.0)(world);

    let sub_actions_or_rewinds = action_entities_scope.collect(&mut *world);
    let depends_on = dependencies_scope.collect(&mut *world);
    let sync_keys = sync_keys_scope.collect(&mut *world);

    debug!(
        "[execute_action_tree] Node {:?} at {} depends on {} Refs at:\n{}\n and has {} sub-actions or rewinds and {} sync keys.",
        node,
        world
            .get::<ActionLocation>(node)
            .map(|l| l.to_string())
            .unwrap_or("Not Given".to_string()),
        depends_on.len(),
        depends_on
            .iter()
            .map(|r| format!("\t{}", r.location.to_string()))
            .collect::<Vec<_>>()
            .join("\n"),
        sub_actions_or_rewinds.len(),
        sync_keys.len(),
    );

    for sync_key in &sync_keys {
        let mut sync_key_to_actions = world.resource_mut::<SyncKeyToActions>();
        let actions = sync_key_to_actions
            .get_mut(sync_key)
            .expect("SyncKey was destroyed");
        actions.insert(node);
    }

    let mut entity_mut = world.entity_mut(node);
    entity_mut.insert(action_node);
    entity_mut.insert(SyncedWith(sync_keys.into_iter().collect()));
    entity_mut.insert(SubscribedTo(
        depends_on.into_iter().map(|r| r.ref_).collect(),
    ));
    entity_mut.add_children(
        &sub_actions_or_rewinds
            .into_iter()
            .map(|ae| *ae)
            .collect::<Vec<_>>(),
    );
}

pub fn flush_tracked_writes(world: &mut World) {
    let writes = Attached::<RefNotify>::take(&mut *world);

    let written_to = writes.iter().map(|rw| rw.ref_).collect::<HashSet<_>>();

    debug!(
        "[execute_action_tree] Flushing tracked writes. Tracked writes to {} Refs.",
        written_to.len()
    );

    let mut affected_nodes = vec![];

    // dont check for a node being part of the tree
    let mut action_nodes =
        world.query_filtered::<(Entity, &SubscribedTo, &ActionLocation), With<ActionNode>>();

    for (node, subscribed_to, location) in action_nodes.iter_mut(world) {
        let affected = !subscribed_to.is_disjoint(&written_to);

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
        world.entity_mut(*affected).insert(NeedsRun);
    }

    for RefNotify { ref_, location } in writes {
        let ref_location = *world
            .get::<RefInitLocation>(*ref_)
            .expect("the Ref has been deallocated");

        debug!(
            "[execute_action_tree] Ref at location {} was written to at {}",
            *ref_location, location
        );
    }
}
