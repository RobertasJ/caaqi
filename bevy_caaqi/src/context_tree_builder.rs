use std::marker::PhantomData;

use bevy::{
    ecs::{bundle::Bundle, entity::Entity, hierarchy::ChildOf, resource::Resource, world::World},
    prelude::Deref,
};
use smallvec::SmallVec;

use crate::world_context::WorldContext;

#[derive(Debug, Default, PartialEq, Eq, Resource)]
struct AttachedNodes<S: ScopeKind>(pub(crate) SmallVec<[DetachedNode<S>; 1]>);

#[derive(Debug, PartialEq, Eq, Hash, Deref)]
pub struct DetachedNode<S: ScopeKind>(#[deref] Entity, PhantomData<S>);

pub trait ScopeKind: Send + Sync + Default + 'static {
    fn with_scope_world<R>(world_scope: impl FnOnce(&mut World) -> R) -> R;
}

impl<S: ScopeKind> DetachedNode<S> {
    pub(crate) fn from_entity(entity: Entity) -> Self {
        Self(entity, PhantomData)
    }
}

pub fn collect_in_scope<S: ScopeKind, R>(
    scope: impl FnOnce() -> R,
) -> (R, SmallVec<[DetachedNode<S>; 1]>) {
    let mut curr_attached = S::with_scope_world(|world| {
        std::mem::take(&mut world.get_resource_or_init::<AttachedNodes<S>>().0)
    });

    let res = scope();

    S::with_scope_world(|world| {
        std::mem::swap(
            &mut curr_attached,
            &mut world.get_resource_or_init::<AttachedNodes<S>>().0,
        )
    });

    (res, curr_attached)
}

pub fn attach_node<S: ScopeKind>(node: DetachedNode<S>) {
    S::with_scope_world(|world| {
        world
            .get_resource_or_init::<AttachedNodes<S>>()
            .0
            .push(node)
    });
}

pub fn spawn_node<S: ScopeKind>(bundle: impl Bundle, world: &mut World) -> DetachedNode<S> {
    DetachedNode::from_entity(world.spawn(bundle).id())
}

#[cfg(test)]
mod scope_tests {
    use super::*;
    use bevy::prelude::*;

    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Component)]
    struct Num(pub i32);

    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
    struct TestScope;

    impl ScopeKind for TestScope {
        fn with_scope_world<R>(world_scope: impl FnOnce(&mut World) -> R) -> R {
            WorldContext::with(|ctx| world_scope(ctx))
        }
    }

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
