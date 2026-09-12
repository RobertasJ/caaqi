use std::ops::Deref;

use crate::{
    action::context_builder::{ActionScope, run_action_node},
    tracked_value::{Notify, SubscribeScope},
};
use bevy::ecs::{
    entity::Entity,
    hierarchy::{ChildOf, Children},
    query::{QueryState, With},
    resource::Resource,
    system::Command,
    world::World,
};
use caaqi_context::collect_in_scope;

use crate::action::node::{self, ActionNode, NeedsRerun};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Resource)]
pub struct ShouldExecuteTree(pub bool);

impl Default for ShouldExecuteTree {
    fn default() -> Self {
        Self(false)
    }
}

pub struct ExecuteActionTree(pub Entity);

impl Command for ExecuteActionTree {
    type Out = ();

    fn apply(self, world: &mut World) -> Self::Out {
        let mut parent_query = world.query_filtered::<&ChildOf, With<ActionNode>>();
        let parents = parent_query.query(world);
        let root = parents.root_ancestor(self.0);

        world.init_resource::<ShouldExecuteTree>();

        fn rec(
            node: Entity,
            world: &mut World,
            tree_query_state: &mut QueryState<&Children, With<ActionNode>>,
        ) {
            if world.get::<NeedsRerun>(node).unwrap().0 {
                run_action_node(world, node);

                world.get_mut::<NeedsRerun>(node).unwrap().0 = false;
            } else {
                let children = tree_query_state
                    .query(world)
                    .get(node)
                    .unwrap()
                    .iter()
                    .copied()
                    .collect::<Vec<_>>();

                for child in children {
                    rec(child, world, tree_query_state);
                }
            }
        }

        let mut tree_query_state = world.query_filtered::<&Children, With<ActionNode>>();
        rec(root, world, &mut tree_query_state);

        let should_execute_tree = world.get_resource_mut::<ShouldExecuteTree>().unwrap().0;

        if should_execute_tree {
            world.commands().queue(ExecuteActionTree(root));
        }

        world.flush();
    }
}
