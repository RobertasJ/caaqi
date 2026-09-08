use crate::context::WorldContext;
pub use crate::context::children_scope::DetachedNode;
use crate::element::CanHaveChildren;
use crate::element::Element;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct ElementTree;

#[track_caller]
pub fn detached_node_with<El: Element>(
    mut element: El,
    build: impl FnOnce(&mut El),
) -> DetachedNode {
    build(&mut element);

    WorldContext::with(|ctx| element.into_ui_node(ctx))
}

#[track_caller]
pub fn detached_node<El: Element + Default>(build: impl FnOnce(&mut El)) -> DetachedNode {
    detached_node_with(El::default(), build)
}

#[track_caller]
pub fn node_with<El: Element>(element: El, build: impl FnOnce(&mut El)) {
    let node = detached_node_with(element, build);

    WorldContext::with(|ctx| ctx.attach_node::<ElementTree>(node));
}

#[track_caller]
pub fn node<El: Element + Default>(build: impl FnOnce(&mut El)) {
    node_with(El::default(), build);
}

#[track_caller]
pub fn detached_scope_with<El: CanHaveChildren>(
    mut element: El,
    build: impl FnOnce(&mut El),
) -> DetachedNode {
    let children = WorldContext::children_scope::<ElementTree>(|| {
        build(&mut element);
    });

    WorldContext::with(|ctx| El::add_children(element.into_ui_node(ctx), ctx, children))
}

#[track_caller]
pub fn detached_scope<El: CanHaveChildren + Default>(build: impl FnOnce(&mut El)) -> DetachedNode {
    detached_scope_with(El::default(), build)
}

#[track_caller]
pub fn scope_with<El: CanHaveChildren>(element: El, build: impl FnOnce(&mut El)) {
    let node = detached_scope_with(element, build);
    WorldContext::with(|ctx| ctx.attach_node::<ElementTree>(node));
}

#[track_caller]
pub fn scope<El: CanHaveChildren + Default>(build: impl FnOnce(&mut El)) {
    scope_with(El::default(), build)
}
