use std::collections::HashSet;

use crate::{
    action::{
        context_builder::ActionEntity,
        node::{ActionLocation, ActionRewind, NeedsRun, Rewound, SubscribedTo, SyncedWith},
        sync::{SyncKey, SyncKeyToActions},
        tree_order::TreeOrder,
    },
    tracked_value::{RefInitLocation, RefNotify, RefSubscribe},
};
use bevy::{
    ecs::{
        entity::Entity,
        hierarchy::{ChildOf, Children},
        query::{QueryState, With, Without},
        resource::Resource,
        system::Command,
        world::World,
    },
    log::{debug, trace},
};
use caaqi_context::{Attached, Scope};

use crate::action::node::ActionNode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Resource)]
pub struct RunningAction(pub Entity);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Resource)]
pub struct TreeRoot(pub Entity);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Resource)]
pub struct ExecutionRoot(pub Entity);

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
                world.insert_resource(ExecutionRoot(node));
                if world.get::<Rewound>(node).is_none() {
                    rewind_action(world, node, root);
                } else {
                    debug!(
                        "[execute_action_tree] Node {:?} at {} already rewound, skipping rewind",
                        node,
                        world
                            .get::<ActionLocation>(node)
                            .map(|l| l.to_string())
                            .unwrap_or("Not Given".to_string()),
                    );
                }

                run_action_node(world, node);

                world.remove_resource::<ExecutionRoot>();
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
            world.insert_resource(TreeRoot(root));

            traverse_tree(root, root, world, &mut tree_query_state);

            world.remove_resource::<TreeRoot>();
        }

        let nodes_to_rerun = world
            .query_filtered::<Entity, (With<NeedsRun>, With<ActionNode>)>()
            .iter(world)
            .collect::<Vec<_>>();

        if !nodes_to_rerun.is_empty() {
            debug!(
                "[execute_action_tree] Nodes to re-run detected after executing action trees. Re-running. Locations:\n{}",
                nodes_to_rerun
                    .iter()
                    .map(|node| {
                        world
                            .get::<ActionLocation>(*node)
                            .map(|location| format!("\t{}", &**location))
                            .unwrap_or_else(|| format!("\tNode {:?} (location not given)", node))
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            );
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

pub fn rewind_action(world: &mut World, node: Entity, tree_root: Entity) {
    debug!(
        "[rewind_action] Rewinding node {:?} at {}",
        node,
        world
            .get::<ActionLocation>(node)
            .map(|l| l.to_string())
            .unwrap_or("Not Given".to_string()),
    );

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
        debug!(
            "[rewind_action] Adding node {:?} at {} to rewind set",
            node,
            world
                .get::<ActionLocation>(node)
                .map(|l| l.to_string())
                .unwrap_or("Not Given".to_string()),
        );
        if !to_rewind.insert(node) {
            debug!(
                "[rewind_action] Node {:?} at {} already in rewind set, skipping",
                node,
                world
                    .get::<ActionLocation>(node)
                    .map(|l| l.to_string())
                    .unwrap_or("Not Given".to_string()),
            );
            return;
        }

        let sync_keys = world.get::<SyncedWith>(node).unwrap();

        for sync_key in &**sync_keys {
            debug!(
                "[rewind_action] Finding synced nodes for node {:?} at {} with SyncKey {:?}",
                node,
                world
                    .get::<ActionLocation>(node)
                    .map(|l| l.to_string())
                    .unwrap_or("Not Given".to_string()),
                sync_key,
            );

            let actions = sync_key_to_actions
                .get(sync_key)
                .expect("SyncKey was destroyed with registered synced rewinds");
            for tree_node in tree_order.backward() {
                if tree_node == node {
                    debug!(
                        "[rewind_action] Reached node {:?} at {} while finding synced nodes for node {:?} at {} with SyncKey {:?}, stopping search",
                        tree_node,
                        world
                            .get::<ActionLocation>(tree_node)
                            .map(|l| l.to_string())
                            .unwrap_or("Not Given".to_string()),
                        node,
                        world
                            .get::<ActionLocation>(node)
                            .map(|l| l.to_string())
                            .unwrap_or("Not Given".to_string()),
                        sync_key,
                    );
                    break;
                }

                if actions.contains(&tree_node) {
                    debug!(
                        "[rewind_action] Found synced node {:?} at {} for node {:?} at {} with SyncKey {:?}, adding to rewind set",
                        tree_node,
                        world
                            .get::<ActionLocation>(tree_node)
                            .map(|l| l.to_string())
                            .unwrap_or("Not Given".to_string()),
                        node,
                        world
                            .get::<ActionLocation>(node)
                            .map(|l| l.to_string())
                            .unwrap_or("Not Given".to_string()),
                        sync_key,
                    );
                    rec(world, tree_node, to_rewind, sync_key_to_actions, tree_order);
                }
            }
        }
    }

    debug!(
        "[rewind_action] Finding synced nodes for rewind of node {:?} at {}",
        node,
        world
            .get::<ActionLocation>(node)
            .map(|l| l.to_string())
            .unwrap_or("Not Given".to_string())
    );
    rec(
        world,
        node,
        &mut to_rewind,
        sync_key_to_actions,
        &tree_order,
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
        entity_mut.insert(Rewound).insert(NeedsRun);
        entity_mut.remove::<SubscribedTo>();
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
                debug!(
                    "[rewind_action] Rewinding node {:?} at {}",
                    node,
                    world
                        .get::<ActionLocation>(node)
                        .map(|l| l.to_string())
                        .unwrap_or("Not Given".to_string()),
                );
                rewind_node(world, node);
            } else {
                debug!(
                    "[rewind_action] Node {:?} at {} already rewound, skipping",
                    node,
                    world
                        .get::<ActionLocation>(node)
                        .map(|l| l.to_string())
                        .unwrap_or("Not Given".to_string()),
                );
            }
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

    debug!(
        "[rewind_action] Rewinding nodes in reverse order for node {:?} at {}",
        node,
        world
            .get::<ActionLocation>(node)
            .map(|l| l.to_string())
            .unwrap_or("Not Given".to_string()),
    );
    traverse_actions_rev(world, tree_root, &to_rewind, &mut tree_query_state);
    flush_tracked_writes(world);
}

pub fn run_action_node(world: &mut World, node: Entity) {
    let parent_action = world.remove_resource::<RunningAction>();
    world.insert_resource(RunningAction(node));

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
        "[execute_action_tree] Ran node {:?} at Location {}\n\
         \tDependencies ({}):\n{}\
         \tSub-actions or rewinds: {}\n\
         \tSync keys attached: {}",
        node,
        world
            .get::<ActionLocation>(node)
            .map(|l| l.to_string())
            .unwrap_or("Not Given".to_string()),
        depends_on.len(),
        depends_on
            .iter()
            .map(|r| format!("\t{}\n", r.location.to_string()))
            .collect::<Vec<_>>()
            .join(""),
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

    entity_mut.remove::<NeedsRun>();
    entity_mut.remove::<Rewound>();

    flush_tracked_writes(world);

    world.entity_mut(node).add_children(
        &sub_actions_or_rewinds
            .into_iter()
            .map(|ae| *ae)
            .collect::<Vec<_>>(),
    );

    world.remove_resource::<RunningAction>();
    if let Some(parent) = parent_action {
        world.insert_resource(parent);
    }
}

pub fn flush_tracked_writes(world: &mut World) {
    let notifys = Attached::<RefNotify>::take(&mut *world);

    debug!(
        "[execute_action_tree] Flushing tracked writes for refs at:\n{}",
        notifys
            .iter()
            .map(|r| {
                let location = world
                    .get::<RefInitLocation>(*r.ref_)
                    .expect("the Ref has been deallocated");
                format!(
                    "\tRef {} at {} was written to at {}. ({})\n",
                    *r.ref_,
                    **location,
                    r.location,
                    if r.forward_only {
                        "Forward-only notify"
                    } else {
                        "Full notify"
                    }
                )
            })
            .collect::<Vec<_>>()
            .join("")
    );

    let forward_only_writes = notifys
        .iter()
        .filter(|rn| rn.forward_only)
        .map(|v| v.ref_)
        .collect::<HashSet<_>>();

    let full_writes = notifys
        .iter()
        .filter(|rn| !rn.forward_only)
        .map(|v| v.ref_)
        .collect::<HashSet<_>>();

    trace!(
        "[execute_action_tree] Full notifies for refs at:\n{}",
        full_writes
            .iter()
            .map(|r| {
                let location = world
                    .get::<RefInitLocation>(**r)
                    .expect("the Ref has been deallocated");
                format!("\tRef at {} was written to\n", **location)
            })
            .collect::<Vec<_>>()
            .join("")
    );

    trace!(
        "[execute_action_tree] Forward-only notifies for refs at:\n{}",
        forward_only_writes
            .iter()
            .map(|r| {
                let location = world
                    .get::<RefInitLocation>(**r)
                    .expect("the Ref has been deallocated");
                format!("\tRef at {} was written to\n", **location)
            })
            .collect::<Vec<_>>()
            .join("")
    );

    let mut affected_nodes = HashSet::new();

    let mut action_nodes =
        world.query_filtered::<(Entity, &SubscribedTo, &ActionLocation), With<ActionNode>>();

    if !full_writes.is_empty() {
        for (entity, subscribed_to, location) in action_nodes.iter(world) {
            if !subscribed_to.0.is_disjoint(&full_writes) {
                debug!(
                    "[execute_action_tree] Node {:?} at {} is affected by full notifications",
                    entity, &**location
                );
                affected_nodes.insert(entity);
            }
        }
    }

    if !forward_only_writes.is_empty() {
        let TreeRoot(tree_root) = *world.resource::<TreeRoot>();
        let ExecutionRoot(execution_root) = *world.resource::<ExecutionRoot>();
        let tree_nodes = TreeOrder::new(world, tree_root);
        let mut is_forward = false;

        for node in tree_nodes.forward() {
            if node == execution_root {
                is_forward = true;
                continue;
            }

            if !is_forward {
                continue;
            }

            let Ok((entity, subscribed_to, location)) = action_nodes.get(world, node) else {
                continue;
            };

            if !subscribed_to.0.is_disjoint(&forward_only_writes) {
                debug!(
                    "[execute_action_tree] Node {:?} at {} is affected by forward-only notifications",
                    entity, &**location
                );
                affected_nodes.insert(entity);
            }
        }
    }

    for affected in &affected_nodes {
        world.entity_mut(*affected).insert(NeedsRun);
    }
}
