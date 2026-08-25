mod component;
mod ui_trees;

use bevy::ecs::component::Component;
pub use ui_trees::{
    CanContainChildren, Container, CreateElement, Item, Leaf, Node, NodeId, Scope, UiTree,
    attach_node, detached_node, detached_node_scope, detached_node_scope_with, node, node_scope,
    node_scope_with, scope, scope_with,
};

#[derive(Debug, Default, Component)]
pub struct CaaqiCtx {
    pub(crate) trees: UiTree,
}

scoped_thread_local::scoped_thread_local!(static CTX: CaaqiCtx);

impl CaaqiCtx {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn trees(&self) -> &UiTree {
        &self.trees
    }
}

pub fn with_caaqi_ctx<R>(ctx: &mut CaaqiCtx, scope: impl FnOnce() -> R) -> R {
    CTX.set(ctx, scope)
}
