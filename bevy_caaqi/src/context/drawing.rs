use bevy::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Component)]
pub struct Drawing {
    pub color: bevy_vello::vello::peniko::Color,
}
