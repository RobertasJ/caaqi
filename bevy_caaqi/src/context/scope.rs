use bevy::{
    ecs::{entity::Entity, resource::Resource},
    prelude::Deref,
};
use smallvec::SmallVec;

use crate::context::WorldContext;

#[derive(Debug, Default, PartialEq, Eq, Resource)]
struct AttachedNodes(pub(crate) SmallVec<[DetachedNode; 1]>);

#[derive(Debug, PartialEq, Eq, Hash, Deref)]
pub struct DetachedNode(pub(crate) Entity);

impl DetachedNode {
    pub(crate) fn from_entity(entity: Entity) -> Self {
        Self(entity)
    }
}

pub fn scope(scope: impl FnOnce()) -> SmallVec<[DetachedNode; 1]> {
    let mut curr_attached = WorldContext::with(|ctx| {
        std::mem::take(&mut ctx.world.get_resource_or_init::<AttachedNodes>().0)
    });

    scope();

    WorldContext::with(|ctx| {
        std::mem::swap(
            &mut curr_attached,
            &mut ctx.world.get_resource_or_init::<AttachedNodes>().0,
        )
    });

    curr_attached
}

pub fn attach_node(node: DetachedNode) {
    WorldContext::with(|ctx| {
        ctx.world
            .get_resource_or_init::<AttachedNodes>()
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

    #[test]
    fn test_scope() {
        let mut world = World::new();
        let mut ctx = WorldContext::new(&mut world);

        let nodes = ctx.enter(|| {
            scope(|| {
                attach_node(DetachedNode::from_entity(WorldContext::with(|ctx| {
                    ctx.world.spawn(Num(42)).id()
                })));
                attach_node(DetachedNode::from_entity(WorldContext::with(|ctx| {
                    ctx.world.spawn(Num(43)).id()
                })));
            })
        });

        assert_eq!(nodes.len(), 2);
    }
}
