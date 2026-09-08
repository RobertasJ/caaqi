use bevy::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Component)]
pub struct Positioning {
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub main_axis: Direction,
}

impl Default for Positioning {
    fn default() -> Self {
        Self {
            x: None,
            y: None,
            main_axis: Default::default(),
        }
    }
}

#[derive(Debug, Default, Eq, Hash, PartialOrd, Ord, Component, Clone, Copy, PartialEq)]
pub enum Direction {
    Horizontal,
    #[default]
    Vertical,
}
