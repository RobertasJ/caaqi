use bevy::{
    ecs::{bundle::Bundle, entity::Entity, hierarchy::ChildOf, resource::Resource, world::World},
    prelude::Deref,
};
use smallvec::SmallVec;

use crate::world_context::WorldContext;

#[derive(Debug, Default, PartialEq, Eq, Resource)]
struct AttachedNodes<TreeKind>(
    pub(crate) SmallVec<[DetachedNode; 1]>,
    pub(crate) std::marker::PhantomData<TreeKind>,
);

#[derive(Debug, PartialEq, Eq, Hash, Deref)]
pub struct DetachedNode(pub(crate) Entity);

impl DetachedNode {
    pub(crate) fn from_entity(entity: Entity) -> Self {
        Self(entity)
    }
}

pub fn children_scope<TreeKind: Send + Sync + Default + 'static>(
    scope: impl FnOnce(),
) -> SmallVec<[DetachedNode; 1]> {
    let mut curr_attached = WorldContext::with(|ctx| {
        std::mem::take(&mut ctx.get_resource_or_init::<AttachedNodes<TreeKind>>().0)
    });

    scope();

    WorldContext::with(|ctx| {
        std::mem::swap(
            &mut curr_attached,
            &mut ctx.get_resource_or_init::<AttachedNodes<TreeKind>>().0,
        )
    });

    curr_attached
}

pub fn attach_node<TreeKind: Send + Sync + Default + 'static>(node: DetachedNode) {
    WorldContext::with(|ctx| {
        ctx.get_resource_or_init::<AttachedNodes<TreeKind>>()
            .0
            .push(node)
    });
}

pub fn spawn_node(bundle: impl Bundle, world: &mut World) -> DetachedNode {
    DetachedNode::from_entity(world.spawn(bundle).id())
}

#[cfg(test)]
mod scope_tests {
    use super::*;
    use bevy::prelude::*;

    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Component)]
    struct Num(pub i32);

    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
    struct TestTree;

    #[test]
    fn test_scope() {
        let mut world = World::new();
        let mut ctx = WorldContext::new(&mut world);

        let nodes = ctx.enter(|| {
            children_scope::<TestTree>(|| {
                let (entity1, entity2) = WorldContext::with(|ctx| {
                    let entity1 = ctx.spawn(Num(42)).id();
                    let entity2 = ctx.spawn(Num(43)).id();

                    (entity1, entity2)
                });

                attach_node::<TestTree>(DetachedNode::from_entity(entity1));
                attach_node::<TestTree>(DetachedNode::from_entity(entity2));
            })
        });

        assert_eq!(nodes.len(), 2);
    }
}
