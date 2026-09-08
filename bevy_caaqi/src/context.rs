mod children_scope;
mod decision_tree_builder;
pub mod element_tree_builder;

pub use element_tree_builder::*;

use bevy::prelude::*;

use crate::node_components::ElementNode;

pub struct WorldContext<'w> {
    pub(crate) world: &'w mut World,
}

scoped_thread_local::scoped_thread_local!(static CTX: WorldContext<'_>);

impl<'w> WorldContext<'w> {
    pub fn new(world: &'w mut World) -> Self {
        Self { world }
    }

    /// Create a node from a bundle.
    /// This is the primary method for node creation.
    pub fn create_element_node<B>(&mut self, bundle: B) -> DetachedNode
    where
        B: Bundle,
    {
        DetachedNode::from_entity(self.world.spawn((ElementNode, bundle)).id())
    }

    pub fn enter<R>(&mut self, scope: impl FnOnce() -> R) -> R {
        CTX.set(self, || scope())
    }

    pub fn with<R>(scope: impl FnOnce(&mut WorldContext) -> R) -> R {
        CTX.with(scope)
    }
}
