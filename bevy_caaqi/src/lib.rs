pub mod action;
pub mod tracked_value;

use bevy::prelude::*;
use caaqi_context::Attached;

use crate::{
    action::{
        context_builder::ActionEntity,
        sync::{SyncKey, SyncKeyToActions},
    },
    tracked_value::{RefNotify, RefSubscribe, RefTypeErased},
};

/// SystemSet for Caaqi UI layout and rendering operations.
///
/// Each variant represents a stage in the UI processing pipeline.
/// External systems can order before or after specific stages using `in_set()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, SystemSet)]
pub enum CaaqiUiSystems {
    /// Size calculation pass (bottom-up traversal).
    Sizing,
    /// Position calculation pass (top-down traversal).
    Positioning,
    /// Render instruction generation.
    Drawing,
    /// Final transform updates and rendering.
    Rendering,
}

pub struct CaaqiPlugin;

impl Plugin for CaaqiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Attached<ActionEntity>>()
            .init_resource::<Attached<RefSubscribe>>()
            .init_resource::<Attached<RefNotify>>()
            .init_resource::<Attached<SyncKey>>()
            .init_resource::<SyncKeyToActions>();
    }
}
