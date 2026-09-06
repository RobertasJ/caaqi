use smallvec::SmallVec;

use super::{CanHaveChildren, IntoUiNode};
use crate::context::{CaaqiCtx, DetachedNode, with_caaqi_ctx};

pub struct Scope<Element>
where
    Element: IntoUiNode + CanHaveChildren,
{
    pub container: Element,
    pub children: SmallVec<[DetachedNode; 1]>,
}

impl<Element> Scope<Element>
where
    Element: IntoUiNode + CanHaveChildren,
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

impl<Element> IntoUiNode for Scope<Element>
where
    Element: IntoUiNode + CanHaveChildren,
{
    fn into_ui_node(self, ctx: &mut CaaqiCtx) -> DetachedNode {
        let parent = self.container.into_ui_node(ctx);
        let children = self.children;

        ctx.attach_children(parent, children)
    }
}

impl<Element: IntoUiNode + CanHaveChildren> CanHaveChildren for Scope<Element> {}
