mod scope;

use bevy::prelude::*;

use crate::element::CanHaveChildren;
use crate::element::Element;

pub use scope::{DetachedNode, attach_node};

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

pub fn detached_node_with<El: Element>(
    mut element: El,
    build: impl FnOnce(&mut El),
) -> DetachedNode {
    build(&mut element);

    WorldContext::with(|ctx| element.into_ui_node(ctx))
}

pub fn detached_node<El: Element + Default>(build: impl FnOnce(&mut El)) -> DetachedNode {
    detached_node_with(El::default(), build)
}

pub fn node_with<El: Element>(element: El, build: impl FnOnce(&mut El)) {
    attach_node(detached_node_with(element, build));
}

pub fn node<El: Element + Default>(build: impl FnOnce(&mut El)) {
    attach_node(detached_node(build));
}

pub fn detached_scope_with<El: CanHaveChildren>(
    mut element: El,
    build: impl FnOnce(&mut El),
) -> DetachedNode {
    let children = scope::scope(|| {
        build(&mut element);
    });

    WorldContext::with(|ctx| El::add_children(element.into_ui_node(ctx), ctx, children))
}

pub fn detached_scope<El: CanHaveChildren + Default>(build: impl FnOnce(&mut El)) -> DetachedNode {
    detached_scope_with(El::default(), build)
}

pub fn scope_with<El: CanHaveChildren>(element: El, build: impl FnOnce(&mut El)) {
    attach_node(detached_scope_with(element, build));
}

pub fn scope<El: CanHaveChildren + Default>(build: impl FnOnce(&mut El)) {
    attach_node(detached_scope(build));
}
