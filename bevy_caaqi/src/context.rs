mod scope;

use bevy::ecs::system::Commands;
use bevy::prelude::*;
use smallvec::SmallVec;

use crate::element::Element;
use crate::{context::scope::SCOPING, element::CanHaveChildren};

pub use scope::{DetachedNode, attach_node};

pub struct CaaqiCtx<'w, 's> {
    pub(crate) commands: Commands<'w, 's>,
}

scoped_thread_local::scoped_thread_local!(static CTX: CaaqiCtx<'_, '_>);

impl<'w, 's> CaaqiCtx<'w, 's> {
    pub fn new(commands: Commands<'w, 's>) -> Self {
        Self { commands }
    }

    /// Create a node from a bundle.
    /// This is the primary method for node creation
    pub fn create_node<B>(&mut self, bundle: B) -> DetachedNode
    where
        B: Bundle,
    {
        DetachedNode::from_entity(
            self.commands
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
            self.commands.entity(child.entity()).insert(
                crate::node_components::tree::CaaqiUiChildOf(parent.entity()),
            );
        }
        parent
    }
}

pub fn enter_caaqi_ctx<R>(ctx: &mut CaaqiCtx, scope: impl FnOnce() -> R) -> R {
    CTX.set(ctx, || SCOPING.set(&mut scope::Tree::default(), scope))
}

pub fn with_caaqi_ctx<R>(scope: impl FnOnce(&mut CaaqiCtx) -> R) -> R {
    CTX.with(scope)
}

pub fn node_detached_with<El: Element>(
    mut element: El,
    build: impl FnOnce(&mut El),
) -> DetachedNode {
    build(&mut element);

    CTX.with(|ctx| element.into_ui_node(ctx))
}

pub fn node_detached<El: Element + Default>(build: impl FnOnce(&mut El)) -> DetachedNode {
    node_detached_with(El::default(), build)
}

pub fn node<El: Element + Default>(build: impl FnOnce(&mut El)) {
    attach_node(node_detached(build));
}

pub fn node_with<El: Element>(element: El, build: impl FnOnce(&mut El)) {
    attach_node(node_detached_with(element, build));
}

pub fn node_scoped_detached_with<El: CanHaveChildren>(
    mut element: El,
    build: impl FnOnce(&mut El),
) -> DetachedNode {
    let children = scope::scope(|| {
        build(&mut element);
    });

    CTX.with(|ctx| El::add_children(element.into_ui_node(ctx), ctx, children))
}

pub fn node_scoped_detached<El: CanHaveChildren + Default>(
    build: impl FnOnce(&mut El),
) -> DetachedNode {
    node_scoped_detached_with(El::default(), build)
}

pub fn node_scoped_with<El: CanHaveChildren>(element: El, build: impl FnOnce(&mut El)) {
    attach_node(node_scoped_detached_with(element, build));
}

pub fn node_scoped<El: CanHaveChildren + Default>(build: impl FnOnce(&mut El)) {
    attach_node(node_scoped_detached(build));
}
