use bevy::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Component)]
pub struct Position {
    pub x: Option<f32>,
    pub y: Option<f32>,
}

impl Default for Position {
    fn default() -> Self {
        Self { x: None, y: None }
    }
}
