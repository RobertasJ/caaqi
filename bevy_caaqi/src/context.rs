use bevy::ecs::system::Commands;
use bevy::prelude::*;
use smallvec::SmallVec;

use crate::element::IntoNodeBundle;

/// Opaque handle to a spawned UI node entity.
/// Can only be constructed through `CaaqiCtx` methods.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct DetachedNode(pub(crate) Entity);

impl DetachedNode {
    pub(crate) fn entity(&self) -> Entity {
        self.0
    }

    pub fn into_inner(self) -> Entity {
        self.0
    }
}

pub struct CaaqiCtx<'w, 's> {
    pub(crate) commands: Commands<'w, 's>,
    pub(crate) attached_nodes: SmallVec<[DetachedNode; 1]>,
}

scoped_thread_local::scoped_thread_local!(static CTX: CaaqiCtx<'_, '_>);

impl<'w, 's> CaaqiCtx<'w, 's> {
    pub fn new(commands: Commands<'w, 's>) -> Self {
        Self {
            commands,
            attached_nodes: Default::default(),
        }
    }

    /// Create a node from an element that implements `IntoNodeBundle`.
    /// This is the primary method for node creation; it ensures exactly one
    /// entity is spawned with the marker component.
    pub fn create_node<E>(&mut self, element: E) -> DetachedNode
    where
        E: crate::element::IntoNodeBundle,
    {
        let bundle = element.into_node_bundle(self);
        DetachedNode(
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
    CTX.set(ctx, scope)
}

pub fn with_caaqi_ctx<R>(scope: impl FnOnce(&mut CaaqiCtx) -> R) -> R {
    CTX.with(scope)
}

pub fn detached_node<Element: IntoNodeBundle>(element: Element) -> DetachedNode {
    with_caaqi_ctx(|ctx| ctx.create_node(element))
}

pub fn attach_node(node: DetachedNode) {
    with_caaqi_ctx(|ctx| ctx.attached_nodes.push(node));
}

pub fn node<Element: IntoNodeBundle>(element: Element) {
    attach_node(detached_node(element));
}

pub fn scope<Element>(scope: impl FnOnce(&mut Element)) -> crate::element::Scope<Element>
where
    Element: crate::element::CanHaveChildren + Default,
{
    crate::element::Scope::new(scope)
}

pub fn detached_node_scope<Element>(scope: impl FnOnce(&mut Element)) -> DetachedNode
where
    Element: crate::element::CanHaveChildren + Default,
{
    let scope_elem = crate::element::Scope::new(scope);
    let children = scope_elem.children.clone();
    let parent = detached_node(scope_elem);
    with_caaqi_ctx(|ctx| {
        for child in children {
            ctx.commands.entity(child.entity()).insert(
                crate::node_components::tree::CaaqiUiChildOf(parent.entity()),
            );
        }
    });
    parent
}

pub fn node_scope<Element>(scope: impl FnOnce(&mut Element))
where
    Element: crate::element::CanHaveChildren + Default,
{
    attach_node(detached_node_scope(scope));
}

pub fn scope_with<Element>(
    element: Element,
    scope: impl FnOnce(&mut Element),
) -> crate::element::Scope<Element>
where
    Element: crate::element::CanHaveChildren,
{
    crate::element::Scope::new_with(element, scope)
}

pub fn detached_node_scope_with<Element>(
    element: Element,
    scope: impl FnOnce(&mut Element),
) -> DetachedNode
where
    Element: crate::element::CanHaveChildren,
{
    let scope_elem = crate::element::Scope::new_with(element, scope);
    let children = scope_elem.children.clone();
    let parent = detached_node(scope_elem);
    with_caaqi_ctx(|ctx| {
        for child in children {
            ctx.commands.entity(child.entity()).insert(
                crate::node_components::tree::CaaqiUiChildOf(parent.entity()),
            );
        }
    });
    parent
}

pub fn node_scope_with<Element>(element: Element, scope: impl FnOnce(&mut Element))
where
    Element: crate::element::CanHaveChildren,
{
    node(scope_with(element, scope));
}
