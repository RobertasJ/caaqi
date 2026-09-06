use bevy::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Component)]
pub struct Drawing {
    pub color: bevy_vello::vello::peniko::Color,
}

impl Default for Drawing {
    fn default() -> Self {
        Self {
            color: bevy_vello::vello::peniko::Color::TRANSPARENT,
        }
    }
}
