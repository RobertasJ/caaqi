use bevy::prelude::*;
use bevy_vello::vello::peniko;
use smallvec::SmallVec;

use crate::context::{CaaqiCtx, drawing::Drawing, sizing::Sizing, with_caaqi_ctx};

/// With this value you can access from the slotmap,
/// knowing you wont be missing the node and that it wont be already borrowed
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Component)]
pub struct DetachedNode(Entity);

impl DetachedNode {
    fn inner(&self) -> Entity {
        self.0
    }

    pub fn into_inner(self) -> Entity {
        self.0
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Component)]
struct CaaqiNode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
#[relationship(relationship_target = UiChildren)]
struct UiChildOf(Entity);

#[derive(Debug, Clone, PartialEq, Eq, Component)]
#[relationship_target(relationship = UiChildOf, linked_spawn)]
struct UiChildren(SmallVec<[Entity; 1]>);

pub struct Leaf;
pub struct Container;

pub trait CreateElement {
    type Kind;
    fn insert_element(self, ctx: &mut CaaqiCtx) -> DetachedNode;
}

#[diagnostic::on_unimplemented(
    message = "`{Element}` cannot be used as a container",
    label = "this element kind does not allow children",
    note = "expected an element of kind `Container`"
)]
pub trait CanContainChildren<Element> {}

impl<Element> CanContainChildren<Element> for Container {}

#[derive(Debug)]
pub struct Item {
    color: peniko::Color,
    width: Option<f32>,
    height: Option<f32>,
}

impl Item {
    pub fn new(color: peniko::Color) -> Self {
        Self {
            color,
            width: None,
            height: None,
        }
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }
}

impl CreateElement for Item {
    type Kind = Leaf;
    fn insert_element(self, ctx: &mut CaaqiCtx) -> DetachedNode {
        DetachedNode(
            ctx.commands
                .spawn((
                    CaaqiNode,
                    Sizing {
                        width: self.width,
                        height: self.height,
                        ..Default::default()
                    },
                    Drawing { color: self.color },
                ))
                .id(),
        )
    }
}

#[derive(Debug, Default)]
pub struct Group;

impl CreateElement for Group {
    type Kind = Container;

    fn insert_element(self, commands: &mut CaaqiCtx) -> DetachedNode {
        DetachedNode(commands.commands.spawn(CaaqiNode).id())
    }
}

pub struct Scope<Element>
where
    Element: CreateElement,
    Element::Kind: CanContainChildren<Element>,
{
    container: Element,
    children: SmallVec<[DetachedNode; 1]>,
}

impl<Element> Scope<Element>
where
    Element: CreateElement,
    Element::Kind: CanContainChildren<Element>,
{
    pub fn new_with<F>(mut element: Element, scope: F) -> Self
    where
        F: FnOnce(&mut Element),
    {
        let mut children = with_caaqi_ctx(|ctx| std::mem::take(&mut ctx.attached_nodes));

        scope(&mut element);

        with_caaqi_ctx(|ctx| std::mem::swap(&mut ctx.attached_nodes, &mut children));

        Self {
            children,
            container: element,
        }
    }
    pub fn new<F>(scope: F) -> Self
    where
        Element: Default,
        F: FnOnce(&mut Element),
    {
        Self::new_with(Element::default(), scope)
    }
}

impl<Element> CreateElement for Scope<Element>
where
    Element: CreateElement,
    Element::Kind: CanContainChildren<Element>,
{
    type Kind = Leaf;
    fn insert_element(self, ctx: &mut CaaqiCtx) -> DetachedNode {
        let group = self.container;
        let node = group.insert_element(ctx);

        let children = self.children;

        for child in &children {
            if node == *child {
                panic!("cannot attach a group to itself");
            }

            ctx.commands
                .entity(child.inner())
                .insert(UiChildOf(node.inner()));
        }

        node
    }
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

pub fn scope<Element>(scope: impl FnOnce(&mut Element)) -> Scope<Element>
where
    Element: CreateElement + Default,
    Element::Kind: CanContainChildren<Element>,
{
    Scope::new(scope)
}

pub fn detached_node_scope<Element>(scope: impl FnOnce(&mut Element)) -> DetachedNode
where
    Element: CreateElement + Default,
    Element::Kind: CanContainChildren<Element>,
{
    detached_node(Scope::new(scope))
}

pub fn node_scope<Element>(scope: impl FnOnce(&mut Element))
where
    Element: CreateElement + Default,
    Element::Kind: CanContainChildren<Element>,
{
    node(Scope::new(scope))
}

pub fn scope_with<Element>(element: Element, scope: impl FnOnce(&mut Element)) -> Scope<Element>
where
    Element: CreateElement,
    Element::Kind: CanContainChildren<Element>,
{
    Scope::new_with(element, scope)
}

pub fn detached_node_scope_with<Element>(
    element: Element,
    scope: impl FnOnce(&mut Element),
) -> DetachedNode
where
    Element: CreateElement,
    Element::Kind: CanContainChildren<Element>,
{
    detached_node(Scope::new_with(element, scope))
}

pub fn node_scope_with<Element>(element: Element, scope: impl FnOnce(&mut Element))
where
    Element: CreateElement,
    Element::Kind: CanContainChildren<Element>,
{
    node(Scope::new_with(element, scope))
}
