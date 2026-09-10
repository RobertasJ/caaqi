use std::marker::PhantomData;

use bevy::{
    ecs::world::World,
    prelude::{Deref, DerefMut},
};

use crate::{
    action::node::{ActionNode, ActionRewind},
    context_tree_builder::{DetachedNode, ScopeKind, attach_node, collect_in_scope, spawn_node},
    world_context::WorldContext,
};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ActionScope;

impl ScopeKind for ActionScope {
    fn with_scope_world<R>(world_scope: impl FnOnce(&mut World) -> R) -> R {
        WorldContext::with(|ctx| world_scope(ctx))
    }
}

impl Clone for DetachedNode<ActionScope> {
    fn clone(&self) -> Self {
        Self::from_entity(**self)
    }
}

impl Copy for DetachedNode<ActionScope> {}

pub fn detached_action(action: impl FnMut() + Send + Sync + 'static) -> DetachedNode<ActionScope> {
    WorldContext::with(|ctx| spawn_node(ActionNode(Box::new(action)), ctx))
}

pub fn action(action: impl FnMut() + Send + Sync + 'static) {
    let node = detached_action(action);
    attach_node::<ActionScope>(node);
    run_action_node(node);
}

pub fn run_action_node(node: DetachedNode<ActionScope>) {
    let mut action_node = WorldContext::with(|ctx| {
        if let Some(mut action_node) = ctx.entity_mut(*node).take::<ActionNode>() {
            action_node
        } else {
            panic!("entity {:?} is not an ActionNode", *node);
        }
    });

    let (_, sub_actions) = collect_in_scope::<ActionScope, _>(|| {
        (action_node.0)();
    });

    WorldContext::with(|ctx| {
        let mut entity_mut = ctx.entity_mut(*node);
        entity_mut.insert(action_node);

        for sub_action in sub_actions {
            entity_mut.add_child(*sub_action);
        }
    });
}

pub fn action_rewind(undo: impl FnOnce() + Send + Sync + 'static) {
    attach_node::<ActionScope>(WorldContext::with(|ctx| {
        spawn_node(ActionRewind(Box::new(undo)), ctx)
    }));
}
