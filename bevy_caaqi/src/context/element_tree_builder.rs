use crate::context::WorldContext;
pub use crate::context::children_scope::DetachedNode;
use crate::element::CanHaveChildren;
use crate::element::Element;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct ElementTree;

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
    WorldContext::with(|ctx| ctx.attach_node::<ElementTree>(detached_node_with(element, build)));
}

pub fn node<El: Element + Default>(build: impl FnOnce(&mut El)) {
    node_with(El::default(), build);
}

pub fn detached_scope_with<El: CanHaveChildren>(
    mut element: El,
    build: impl FnOnce(&mut El),
) -> DetachedNode {
    let children = WorldContext::children_scope::<ElementTree>(|| {
        build(&mut element);
    });

    WorldContext::with(|ctx| El::add_children(element.into_ui_node(ctx), ctx, children))
}

pub fn detached_scope<El: CanHaveChildren + Default>(build: impl FnOnce(&mut El)) -> DetachedNode {
    detached_scope_with(El::default(), build)
}

pub fn scope_with<El: CanHaveChildren>(element: El, build: impl FnOnce(&mut El)) {
    WorldContext::with(|ctx| ctx.attach_node::<ElementTree>(detached_scope_with(element, build)));
}

pub fn scope<El: CanHaveChildren + Default>(build: impl FnOnce(&mut El)) {
    scope_with(El::default(), build)
}
