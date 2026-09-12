use std::{collections::HashSet, ops::Deref};

use crate::{
    action::{
        context_builder::{ActionScope, run_action_node},
        node::{Deps, Stale},
    },
    tracked_value::{RefTypeErased, SubscribeScope},
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
    prelude::{Deref, DerefMut},
};
use caaqi_context::collect_in_scope;

use crate::action::node::{self, ActionNode};

#[derive(Debug, Default, Clone, Resource, Deref, DerefMut)]
pub struct WrittenTo(HashSet<Entity>);

pub struct ExecuteActionTree(pub Entity);

impl Command for ExecuteActionTree {
    type Out = ();

    fn apply(self, world: &mut World) -> Self::Out {
        let mut parent_query = world.query_filtered::<&ChildOf, With<ActionNode>>();
        let parents = parent_query.query(world);
        let root = parents.root_ancestor(self.0);

        world.init_resource::<WrittenTo>();

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

        let written_to = world.remove_resource::<WrittenTo>().unwrap();

        let mut affected_nodes = vec![];
        let mut action_nodes = world.query_filtered::<(Entity, &Deps), With<ActionNode>>();
        for (node, deps) in action_nodes.iter_mut(world) {
            let affected = !deps.is_disjoint(&*written_to);

            if affected {
                affected_nodes.push(node);
            }
        }

        for affected in &affected_nodes {
            world.entity_mut(*affected).insert(Stale);
        }

        if affected_nodes.len() > 0 {
            world.commands().queue(ExecuteActionTree(root));
        }
    }
}
