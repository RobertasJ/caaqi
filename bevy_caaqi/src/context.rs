mod children_scope;
mod decision_tree_builder;
pub mod tree_builder;

pub use tree_builder::*;

use bevy::prelude::*;

pub struct WorldContext<'w> {
    pub(crate) world: &'w mut World,
}

scoped_thread_local::scoped_thread_local!(static CTX: WorldContext<'_>);

impl<'w> WorldContext<'w> {
    pub fn new(world: &'w mut World) -> Self {
        Self { world }
    }

    /// Create a node from a bundle.
    /// This is the primary method for node creation
    pub fn create_node<B>(&mut self, bundle: B) -> DetachedNode
    where
        B: Bundle,
    {
        DetachedNode::from_entity(
            self.world
                .spawn((crate::node_components::CaaqiNode, bundle))
                .id(),
        )
    }

    /// Attach child nodes to a parent node, establishing parent-child relationships.
    /// This method consumes both the parent and all children, preventing reuse
    /// and ensuring each child is attached exactly once.
    pub fn attach_children(
        &mut self,
        parent: DetachedNode,
        children: impl IntoIterator<Item = DetachedNode>,
    ) -> DetachedNode {
        for child in children {
            self.world.entity_mut(*child).insert(ChildOf(*parent));
        }
        parent
    }

    pub fn enter<R>(&mut self, scope: impl FnOnce() -> R) -> R {
        CTX.set(self, || scope())
    }

    pub fn with<R>(scope: impl FnOnce(&mut WorldContext) -> R) -> R {
        CTX.with(scope)
    }
}
