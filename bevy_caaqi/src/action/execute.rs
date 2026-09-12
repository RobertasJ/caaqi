use std::{collections::HashSet, ops::Deref};

use crate::{
    action::{
        context_builder::{ActionScope, flush_tracked_writes, run_action_node},
        node::{ActionLocation, Deps, Stale},
    },
    tracked_value::{RefInitLocation, RefTypeErased, SubscribeScope},
};
use bevy::{
    ecs::{
        entity::Entity,
        hierarchy::{ChildOf, Children},
        query::{QueryState, With},
        resource::Resource,
        system::Command,
        world::{EntityMut, EntityWorldMut, World},
    },
    platform::collections::HashMap,
    prelude::{Deref, DerefMut},
};
use caaqi_context::collect_in_scope;

use crate::action::node::{self, ActionNode};

pub struct ExecuteActionTree(pub Entity);

impl Command for ExecuteActionTree {
    type Out = ();

    fn apply(self, world: &mut World) -> Self::Out {
        let mut parent_query = world.query_filtered::<&ChildOf, With<ActionNode>>();
        let parents = parent_query.query(world);
        let root = parents.root_ancestor(self.0);

        fn rec(
            node: Entity,
            world: &mut World,
            tree_query_state: &mut QueryState<&Children, With<ActionNode>>,
        ) {
            if world.get::<Stale>(node).is_some() {
                world.entity_mut(node).remove::<Stale>();
                run_action_node(world, node);
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
        rec(root, world, &mut tree_query_state);

        let has_stale_nodes = world
            .query_filtered::<Entity, (With<Stale>, With<ActionNode>)>()
            .iter(world)
            .next()
            .is_some();

        if has_stale_nodes {
            world.commands().queue(ExecuteActionTree(root));
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
