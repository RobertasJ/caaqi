use bevy::prelude::*;
use bevy_vello::vello::peniko;
use smallvec::SmallVec;

use crate::context::{
    CaaqiCtx,
    drawing::Drawing,
    positioning::{Direction, Positioning},
    sizing::Sizing,
    with_caaqi_ctx,
};

/// With this value you can access from the slotmap,
/// knowing you wont be missing the node and that it wont be already borrowed
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Component)]
pub struct DetachedNode(pub(crate) Entity);

impl DetachedNode {
    fn inner(&self) -> Entity {
        self.0
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Component)]
#[require(Sizing, Drawing, Positioning)]
pub struct CaaqiNode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component, Deref)]
#[relationship(relationship_target = CaaqiUiChildren)]
pub struct CaaqiUiChildOf(Entity);

#[derive(Debug, Clone, PartialEq, Eq, Component, Deref)]
#[relationship_target(relationship = CaaqiUiChildOf, linked_spawn)]
pub struct CaaqiUiChildren(SmallVec<[Entity; 1]>);

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

#[derive(Debug, Clone)]
pub struct Item {
    color: peniko::Color,
    width: Option<f64>,
    height: Option<f64>,
    main_axis: Direction,
    padding_top: f64,
    padding_bottom: f64,
    padding_left: f64,
    padding_right: f64,
    margin_top: f64,
    margin_bottom: f64,
    margin_left: f64,
    margin_right: f64,
}

impl Default for Item {
    fn default() -> Self {
        Self {
            color: peniko::Color::TRANSPARENT,
            width: None,
            height: None,
            main_axis: Default::default(),
            padding_top: 0.0,
            padding_bottom: 0.0,
            padding_left: 0.0,
            padding_right: 0.0,
            margin_top: 0.0,
            margin_bottom: 0.0,
            margin_left: 0.0,
            margin_right: 0.0,
        }
    }
}

impl Item {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn color(mut self, color: peniko::Color) -> Self {
        self.color = color;
        self
    }

    pub fn width(mut self, width: f64) -> Self {
        self.width = Some(width);
        self
    }

    pub fn height(mut self, height: f64) -> Self {
        self.height = Some(height);
        self
    }

    pub fn main_axis(mut self, main_axis: Direction) -> Self {
        self.main_axis = main_axis;
        self
    }

    pub fn padding(mut self, top: f64, bottom: f64, left: f64, right: f64) -> Self {
        self.padding_top = top;
        self.padding_bottom = bottom;
        self.padding_left = left;
        self.padding_right = right;
        self
    }

    pub fn padding_top(mut self, top: f64) -> Self {
        self.padding_top = top;
        self
    }

    pub fn padding_bottom(mut self, bottom: f64) -> Self {
        self.padding_bottom = bottom;
        self
    }

    pub fn padding_left(mut self, left: f64) -> Self {
        self.padding_left = left;
        self
    }

    pub fn padding_right(mut self, right: f64) -> Self {
        self.padding_right = right;
        self
    }

    pub fn padding_horizontal(mut self, left: f64, right: f64) -> Self {
        self.padding_left = left;
        self.padding_right = right;
        self
    }

    pub fn padding_vertical(mut self, top: f64, bottom: f64) -> Self {
        self.padding_top = top;
        self.padding_bottom = bottom;
        self
    }

    pub fn margin(mut self, top: f64, bottom: f64, left: f64, right: f64) -> Self {
        self.margin_top = top;
        self.margin_bottom = bottom;
        self.margin_left = left;
        self.margin_right = right;
        self
    }

    pub fn margin_top(mut self, top: f64) -> Self {
        self.margin_top = top;
        self
    }

    pub fn margin_bottom(mut self, bottom: f64) -> Self {
        self.margin_bottom = bottom;
        self
    }

    pub fn margin_left(mut self, left: f64) -> Self {
        self.margin_left = left;
        self
    }

    pub fn margin_right(mut self, right: f64) -> Self {
        self.margin_right = right;
        self
    }

    pub fn margin_horizontal(mut self, left: f64, right: f64) -> Self {
        self.margin_left = left;
        self.margin_right = right;
        self
    }

    pub fn margin_vertical(mut self, top: f64, bottom: f64) -> Self {
        self.margin_top = top;
        self.margin_bottom = bottom;
        self
    }
}

impl CreateElement for Item {
    type Kind = Container;
    fn insert_element(self, ctx: &mut CaaqiCtx) -> DetachedNode {
        DetachedNode(
            ctx.commands
                .spawn((
                    CaaqiNode,
                    Sizing {
                        inner_width: self.width,
                        inner_height: self.height,
                        margin_bottom: self.margin_bottom,
                        margin_left: self.margin_left,
                        margin_right: self.margin_right,
                        margin_top: self.margin_top,
                        padding_bottom: self.padding_bottom,
                        padding_left: self.padding_left,
                        padding_right: self.padding_right,
                        padding_top: self.padding_top,
                        ..Default::default()
                    },
                    Positioning {
                        main_axis: self.main_axis,
                        ..Default::default()
                    },
                    Drawing { color: self.color },
                ))
                .id(),
        )
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
                .insert(CaaqiUiChildOf(node.inner()));
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
