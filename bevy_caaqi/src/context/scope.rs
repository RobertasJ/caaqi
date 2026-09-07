use bevy::ecs::entity::Entity;
use smallvec::SmallVec;

#[derive(Debug, Default)]
pub struct Tree {
    attached_nodes: SmallVec<[DetachedNode; 1]>,
}

scoped_thread_local::scoped_thread_local!(pub static SCOPING: Tree);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DetachedNode(pub(crate) Entity);

impl DetachedNode {
    pub fn into_entity(self) -> Entity {
        self.0
    }

    pub(crate) fn entity(&self) -> Entity {
        self.0
    }

    pub(crate) fn from_entity(entity: Entity) -> Self {
        Self(entity)
    }
}

pub fn scope(scope: impl FnOnce()) -> SmallVec<[DetachedNode; 1]> {
    let mut curr_attached = SCOPING.with(|tree| std::mem::take(&mut tree.attached_nodes));

    scope();

    SCOPING.with(|tree| std::mem::swap(&mut curr_attached, &mut tree.attached_nodes));

    curr_attached
}

pub fn attach_node(node: DetachedNode) {
    SCOPING.with(|tree| tree.attached_nodes.push(node));
}
