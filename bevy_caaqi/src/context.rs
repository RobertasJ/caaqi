pub mod component;
pub mod drawing;
pub mod position;
pub mod sizing;
pub mod ui_node;

use bevy::ecs::system::Commands;
use smallvec::SmallVec;

use crate::context::ui_node::DetachedNode;

pub struct CaaqiCtx<'w, 's> {
    commands: Commands<'w, 's>,
    attached_nodes: SmallVec<[DetachedNode; 1]>,
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
