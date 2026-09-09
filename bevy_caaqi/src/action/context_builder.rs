use std::marker::PhantomData;

use bevy::{
    ecs::world::World,
    prelude::{Deref, DerefMut},
};

use crate::{
    action::node::{ActionNode, ActionRewind},
    context_tree_builder::{DetachedNode, ScopeKind, attach_node, collect_in_scope, spawn_node},
    element::context_builder::{scope, scope_with},
    world_context::WorldContext,
};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ActionScope;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deref, DerefMut)]
pub struct DetachedAction<R: Clone + Send + Sync + 'static>(
    #[deref] DetachedNode<ActionScope>,
    PhantomData<R>,
);

impl<R: Clone + Send + Sync + 'static> DetachedAction<R> {
    pub fn from_node(node: DetachedNode<ActionScope>) -> Self {
        Self(node, PhantomData)
    }
}

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

pub fn detached_action<R: Clone + Send + Sync + 'static>(
    mut action: impl FnMut() -> R + Send + Sync + 'static,
) -> DetachedAction<R> {
    DetachedAction::from_node(WorldContext::with(|ctx| {
        spawn_node(ActionNode(Box::new(move || Box::new(action()))), ctx)
    }))
}

pub fn action<R: Clone + Send + Sync + 'static>(action: impl FnMut() -> R + Send + Sync + 'static) {
    let node = detached_action(action);
    attach_node::<ActionScope>(*node);
    run_action_node(node);
}

pub fn run_action_node<R: Clone + Send + Sync + 'static>(node: DetachedAction<R>) {
    let mut action_node = WorldContext::with(|ctx| {
        if let Some(mut action_node) = ctx.entity_mut(**node).take::<ActionNode>() {
            action_node
        } else {
            panic!("entity {:?} is not an ActionNode", *node);
        }
    });

    let (val, sub_actions) = collect_in_scope::<ActionScope, _>(|| (action_node.0)());

    WorldContext::with(|ctx| {
        ctx.entity_mut(**node).insert(action_node);
    });
}

pub fn action_rewind(undo: impl FnOnce() + Send + Sync + 'static) {
    attach_node::<ActionScope>(WorldContext::with(|ctx| {
        spawn_node(ActionRewind(Box::new(undo)), ctx)
    }));
}
