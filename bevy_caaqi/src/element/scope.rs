use smallvec::SmallVec;

use super::{CanHaveChildren, IntoNodeBundle};
use crate::context::{CaaqiCtx, DetachedNode, with_caaqi_ctx};

pub struct Scope<Element>
where
    Element: CanHaveChildren,
{
    pub container: Element,
    pub children: SmallVec<[DetachedNode; 1]>,
}

impl<Element> Scope<Element>
where
    Element: CanHaveChildren,
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

impl<Element> IntoNodeBundle for Scope<Element>
where
    Element: CanHaveChildren,
{
    fn into_node_bundle(self, ctx: &mut CaaqiCtx) -> impl bevy::prelude::Bundle {
        self.container.into_node_bundle(ctx)
    }
}

impl<Element> CanHaveChildren for Scope<Element> where Element: CanHaveChildren {}
