pub mod drawing;
pub mod positioning;
pub mod sizing;
pub mod tree;

use bevy::prelude::*;

pub use drawing::Drawing;
pub use positioning::{Direction, Positioning};
pub use sizing::Sizing;
pub use tree::{CaaqiUiChildOf, CaaqiUiChildren};

/// Marker component identifying an entity as part of the Caaqi UI system.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Component)]
#[require(Sizing, Drawing, Positioning)]
pub(crate) struct CaaqiNode;
