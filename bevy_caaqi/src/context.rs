pub mod component;

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
