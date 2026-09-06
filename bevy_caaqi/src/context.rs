use bevy::prelude::*;
use bevy::ecs::system::Commands;
use smallvec::SmallVec;

use crate::element::CreateElement;

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
            self.commands
                .entity(child.entity())
                .insert(crate::node_components::tree::CaaqiUiChildOf(parent.entity()));
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

pub fn detached_node<Element: CreateElement>(element: Element) -> DetachedNode {
    with_caaqi_ctx(|ctx| element.insert_element(ctx))
}

pub fn attach_node(node: DetachedNode) {
    with_caaqi_ctx(|ctx| ctx.attached_nodes.push(node));
}

pub fn node<Element: CreateElement>(element: Element) {
    attach_node(detached_node(element));
}

pub fn scope<Element>(scope: impl FnOnce(&mut Element)) -> crate::element::Scope<Element>
where
    Element: CreateElement + Default,
    Element::Kind: crate::element::CanContainChildren<Element>,
{
    crate::element::Scope::new(scope)
}

pub fn detached_node_scope<Element>(scope: impl FnOnce(&mut Element)) -> DetachedNode
where
    Element: CreateElement + Default,
    Element::Kind: crate::element::CanContainChildren<Element>,
{
    detached_node(crate::element::Scope::new(scope))
}

pub fn node_scope<Element>(scope: impl FnOnce(&mut Element))
where
    Element: CreateElement + Default,
    Element::Kind: crate::element::CanContainChildren<Element>,
{
    node(crate::element::Scope::new(scope))
}

pub fn scope_with<Element>(element: Element, scope: impl FnOnce(&mut Element)) -> crate::element::Scope<Element>
where
    Element: CreateElement,
    Element::Kind: crate::element::CanContainChildren<Element>,
{
    crate::element::Scope::new_with(element, scope)
}

pub fn detached_node_scope_with<Element>(
    element: Element,
    scope: impl FnOnce(&mut Element),
) -> DetachedNode
where
    Element: CreateElement,
    Element::Kind: crate::element::CanContainChildren<Element>,
{
    detached_node(crate::element::Scope::new_with(element, scope))
}

pub fn node_scope_with<Element>(element: Element, scope: impl FnOnce(&mut Element))
where
    Element: CreateElement,
    Element::Kind: crate::element::CanContainChildren<Element>,
{
    node(crate::element::Scope::new_with(element, scope))
}
