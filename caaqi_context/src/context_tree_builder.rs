use std::marker::PhantomData;

use bevy::{
    ecs::{bundle::Bundle, entity::Entity, resource::Resource, world::World},
    log::warn,
    prelude::{Deref, DerefMut},
};
use smallvec::SmallVec;

// use crate::WORLD;

#[derive(Debug, Default, PartialEq, Eq, Resource, Deref, DerefMut)]
struct AttachedNodes<S: ScopeKind>(#[deref] SmallVec<[Entity; 1]>, PhantomData<S>);

#[derive(Debug, PartialEq, Eq, Hash, Deref)]
pub struct DetachedNode<S: ScopeKind>(#[deref] Entity, PhantomData<S>);

pub trait ScopeKind: Default + Send + Sync + 'static {}

impl<S: ScopeKind> Clone for DetachedNode<S> {
    fn clone(&self) -> Self {
        Self::from_entity(**self)
    }
}

impl<S: ScopeKind> Copy for DetachedNode<S> {}

impl<S: ScopeKind> DetachedNode<S> {
    pub fn from_entity(entity: Entity) -> Self {
        Self(entity, PhantomData)
    }
}

pub fn collect_in_scope<S: ScopeKind, R>(
    world: &mut World,
    scope: impl FnOnce(&mut World) -> R,
) -> (R, SmallVec<[Entity; 1]>) {
    let parent_attached = world
        .get_resource_mut::<AttachedNodes<S>>()
        .map(|mut r| std::mem::take(&mut r.0))
        .or_else(|| {
            world.insert_resource(AttachedNodes::<S>::default());
            None
        });

    let res = scope(world);

    let attached = if let Some(mut previously_attached) = parent_attached {
        std::mem::swap(
            &mut previously_attached,
            &mut world
                .get_resource_mut::<AttachedNodes<S>>()
                .expect("no scope for node exists")
                .0,
        );

        previously_attached
    } else {
        let nodes = world.remove_resource::<AttachedNodes<S>>();
        nodes.unwrap().0
    };

    (res, attached)
}

pub fn attach_node<S: ScopeKind>(world: &mut World, node: DetachedNode<S>) {
    if let Some(mut attached_nodes) = world.get_resource_mut::<AttachedNodes<S>>() {
        attached_nodes.push(*node);
    } else {
        warn!(
            "Attempted to attach node {:?} to scope {:?}, but no scope exists. Node will be dropped.",
            *node,
            std::any::type_name::<S>()
        );
    }
}

#[cfg(test)]
mod scope_tests {
    use super::*;
    use bevy::prelude::*;

    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Component)]
    struct Num(pub i32);

    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
    struct TestScope;

    impl ScopeKind for TestScope {}

    #[test]
    fn test_scope() {
        let mut world = World::new();

        let (_, nodes) = collect_in_scope::<TestScope, ()>(&mut world, |world| {
            let entity1 = world.spawn(Num(42)).id();
            let entity2 = world.spawn(Num(43)).id();

            attach_node::<TestScope>(world, DetachedNode::from_entity(entity1));
            attach_node::<TestScope>(world, DetachedNode::from_entity(entity2));
        });

        assert_eq!(nodes.len(), 2);
    }
}
