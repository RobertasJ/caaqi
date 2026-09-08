use bevy::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Component)]
pub struct Sizing {
    pub min_width: f64,
    pub min_height: f64,
    pub max_width: f64,
    pub max_height: f64,
    pub padding_top: f64,
    pub padding_bottom: f64,
    pub padding_left: f64,
    pub padding_right: f64,
    pub margin_top: f64,
    pub margin_bottom: f64,
    pub margin_left: f64,
    pub margin_right: f64,
    pub inner_width: Option<f64>,
    pub inner_height: Option<f64>,
}

impl Default for Sizing {
    fn default() -> Self {
        Self {
            min_width: 0.0,
            min_height: 0.0,
            max_width: f64::INFINITY,
            max_height: f64::INFINITY,
            padding_top: 0.0,
            padding_bottom: 0.0,
            padding_left: 0.0,
            padding_right: 0.0,
            margin_top: 0.0,
            margin_bottom: 0.0,
            margin_left: 0.0,
            margin_right: 0.0,
            inner_width: None,
            inner_height: None,
        }
    }
}

impl Sizing {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn outer_width(&self) -> f64 {
        self.inner_width.unwrap_or(0.0)
            + self.margin_left
            + self.margin_right
            + self.padding_left
            + self.padding_right
    }

    pub fn outer_height(&self) -> f64 {
        self.inner_height.unwrap_or(0.0)
            + self.margin_top
            + self.margin_bottom
            + self.padding_top
            + self.padding_bottom
    }

    pub fn visible_width(&self) -> f64 {
        self.inner_width.unwrap_or(0.0) + self.padding_left + self.padding_right
    }

    pub fn visible_height(&self) -> f64 {
        self.inner_height.unwrap_or(0.0) + self.padding_top + self.padding_bottom
    }

    pub fn inner_width(&self) -> f64 {
        self.inner_width.unwrap_or(0.0) - self.padding_left - self.padding_right
    }

    pub fn inner_height(&self) -> f64 {
        self.inner_height.unwrap_or(0.0) - self.padding_top - self.padding_bottom
    }
}
