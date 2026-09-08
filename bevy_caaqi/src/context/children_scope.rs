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

impl WorldContext<'_> {
    pub fn children_scope(scope: impl FnOnce()) -> SmallVec<[DetachedNode; 1]> {
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

    pub fn attach_node(&mut self, node: DetachedNode) {
        self.world
            .get_resource_or_init::<AttachedNodes>()
            .0
            .push(node)
    }
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
            WorldContext::children_scope(|| {
                WorldContext::with(|ctx| {
                    let entity = ctx.world.spawn(Num(42)).id();
                    ctx.attach_node(DetachedNode::from_entity(entity));
                    let entity = ctx.world.spawn(Num(43)).id();
                    ctx.attach_node(DetachedNode::from_entity(entity));
                });
            })
        });

        assert_eq!(nodes.len(), 2);
    }
}
