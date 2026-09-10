use bevy::{ecs::bundle::Bundle, prelude::World};

use crate::{
    context_tree_builder::{DetachedNode, ScopeKind, attach_node, collect_in_scope},
    element::{CanHaveChildren, Element, node::ElementNode},
    world_context::WorldContext,
};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ElementScope;

impl ScopeKind for ElementScope {
    fn with_scope_world<R>(world_scope: impl FnOnce(&mut World) -> R) -> R {
        WorldContext::with(|ctx| world_scope(ctx))
    }
}

/// Create a node from a bundle.
/// This is the primary method for node creation.
#[track_caller]
pub fn create_element_node(bundle: impl Bundle, world: &mut World) -> DetachedNode<ElementScope> {
    crate::context_tree_builder::spawn_node((ElementNode, bundle), world)
}

#[track_caller]
pub fn detached_node_with<El: Element>(
    mut element: El,
    build: impl FnOnce(&mut El),
) -> DetachedNode<ElementScope> {
    build(&mut element);

    WorldContext::with(|ctx| element.into_ui_node(ctx))
}

#[track_caller]
pub fn detached_node<El: Element + Default>(
    build: impl FnOnce(&mut El),
) -> DetachedNode<ElementScope> {
    detached_node_with(El::default(), build)
}

#[track_caller]
pub fn node_with<El: Element>(element: El, build: impl FnOnce(&mut El)) {
    let node = detached_node_with(element, build);

    attach_node::<ElementScope>(node);
}

#[track_caller]
pub fn node<El: Element + Default>(build: impl FnOnce(&mut El)) {
    node_with(El::default(), build);
}

#[track_caller]
pub fn detached_scope_with<El: CanHaveChildren>(
    mut element: El,
    build: impl FnOnce(&mut El),
) -> DetachedNode<ElementScope> {
    let (_, children) = collect_in_scope::<ElementScope, ()>(|| {
        build(&mut element);
    });

    WorldContext::with(|ctx| El::add_children(element.into_ui_node(ctx), ctx, children))
}

#[track_caller]
pub fn detached_scope<El: CanHaveChildren + Default>(
    build: impl FnOnce(&mut El),
) -> DetachedNode<ElementScope> {
    detached_scope_with(El::default(), build)
}

#[track_caller]
pub fn scope_with<El: CanHaveChildren>(element: El, build: impl FnOnce(&mut El)) {
    let node = detached_scope_with(element, build);
    attach_node::<ElementScope>(node);
}

#[track_caller]
pub fn scope<El: CanHaveChildren + Default>(build: impl FnOnce(&mut El)) {
    scope_with(El::default(), build)
}
