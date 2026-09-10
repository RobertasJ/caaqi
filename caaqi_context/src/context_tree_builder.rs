use std::marker::PhantomData;

use bevy::{
    ecs::{bundle::Bundle, entity::Entity, resource::Resource},
    prelude::Deref,
};
use smallvec::SmallVec;

use crate::WorldContext;

#[derive(Debug, Default, PartialEq, Eq, Resource)]
struct AttachedNodes<S: ScopeKind>(SmallVec<[DetachedNode<S>; 1]>);

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
    scope: impl FnOnce() -> R,
) -> (R, SmallVec<[DetachedNode<S>; 1]>) {
    let parent_attached = WorldContext::with(|world| {
        world
            .get_resource_mut::<AttachedNodes<S>>()
            .map(|mut r| std::mem::take(&mut r.0))
            .or_else(|| {
                world.insert_resource(AttachedNodes::<S>::default());
                None
            })
    });

    let res = scope();

    let attached = WorldContext::with(|world| {
        if let Some(mut previously_attached) = parent_attached {
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
        }
    });

    (res, attached)
}

pub fn attach_node<S: ScopeKind>(node: DetachedNode<S>) {
    WorldContext::with(|world| {
        world
            .get_resource_mut::<AttachedNodes<S>>()
            .expect("no scope for node exists")
            .0
            .push(node)
    });
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
        let mut ctx = WorldContext::new(&mut world);

        let (_, nodes) = ctx.enter(|| {
            collect_in_scope::<TestScope, ()>(|| {
                let (entity1, entity2) = WorldContext::with(|ctx| {
                    let entity1 = ctx.spawn(Num(42)).id();
                    let entity2 = ctx.spawn(Num(43)).id();

                    (entity1, entity2)
                });

                attach_node::<TestScope>(DetachedNode::from_entity(entity1));
                attach_node::<TestScope>(DetachedNode::from_entity(entity2));
            })
        });

        assert_eq!(nodes.len(), 2);
    }
}
