use smallvec::SmallVec;

use crate::context::{CaaqiCtx, DetachedNode, with_caaqi_ctx};
use crate::node_components::tree::CaaqiUiChildOf;
use super::{CreateElement, CanContainChildren};

pub struct Scope<Element>
where
    Element: CreateElement,
    Element::Kind: CanContainChildren<Element>,
{
    pub container: Element,
    pub children: SmallVec<[DetachedNode; 1]>,
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
    type Kind = super::Leaf;

    fn insert_element(self, ctx: &mut CaaqiCtx) -> DetachedNode {
        let container = self.container;
        let node = container.insert_element(ctx);
        let children = self.children;

        for child in &children {
            if node == *child {
                panic!("cannot attach a group to itself");
            }

            ctx.commands
                .entity(child.entity())
                .insert(CaaqiUiChildOf(node.entity()));
        }

        node
    }
}
