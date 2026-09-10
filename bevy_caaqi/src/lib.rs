pub mod action;
pub mod tracked_value;

use bevy::prelude::*;

use crate::action::context_builder::ActionScope;
pub use caaqi_context::DetachedNode;

#[derive(Debug, Component)]
pub struct CaaqiActionRoot(pub DetachedNode<ActionScope>);

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
        app.add_systems(PostUpdate, (run_decision_tree,).chain());
    }
}

fn run_decision_tree(mut world: &mut World) {}
