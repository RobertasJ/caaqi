use bevy::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Component)]
pub struct Sizing {
    pub min_width: f32,
    pub min_height: f32,
    pub max_width: f32,
    pub max_height: f32,
    pub preferred_width: f32,
    pub preferred_height: f32,
    pub width: Option<f32>,
    pub height: Option<f32>,
}

impl Default for Sizing {
    fn default() -> Self {
        Self {
            min_width: 0.0,
            min_height: 0.0,
            max_width: f32::INFINITY,
            max_height: f32::INFINITY,
            preferred_width: 0.0,
            preferred_height: 0.0,
            width: None,
            height: None,
        }
    }
}
