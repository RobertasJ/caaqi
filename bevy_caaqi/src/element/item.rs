use bevy_vello::vello::peniko;

use super::{CanHaveChildren, IntoUiNode};
use crate::node_components::positioning::Direction;
use crate::node_components::{Drawing, Positioning, Sizing};

#[derive(Debug, Clone)]
pub struct Item {
    color: peniko::Color,
    width: Option<f64>,
    height: Option<f64>,
    main_axis: Direction,
    padding_top: f64,
    padding_bottom: f64,
    padding_left: f64,
    padding_right: f64,
    margin_top: f64,
    margin_bottom: f64,
    margin_left: f64,
    margin_right: f64,
}

impl Default for Item {
    fn default() -> Self {
        Self {
            color: peniko::Color::TRANSPARENT,
            width: None,
            height: None,
            main_axis: Default::default(),
            padding_top: 0.0,
            padding_bottom: 0.0,
            padding_left: 0.0,
            padding_right: 0.0,
            margin_top: 0.0,
            margin_bottom: 0.0,
            margin_left: 0.0,
            margin_right: 0.0,
        }
    }
}

impl Item {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn color(mut self, color: peniko::Color) -> Self {
        self.color = color;
        self
    }

    pub fn width(mut self, width: f64) -> Self {
        self.width = Some(width);
        self
    }

    pub fn height(mut self, height: f64) -> Self {
        self.height = Some(height);
        self
    }

    pub fn main_axis(mut self, main_axis: Direction) -> Self {
        self.main_axis = main_axis;
        self
    }

    pub fn padding(mut self, top: f64, bottom: f64, left: f64, right: f64) -> Self {
        self.padding_top = top;
        self.padding_bottom = bottom;
        self.padding_left = left;
        self.padding_right = right;
        self
    }

    pub fn padding_top(mut self, top: f64) -> Self {
        self.padding_top = top;
        self
    }

    pub fn padding_bottom(mut self, bottom: f64) -> Self {
        self.padding_bottom = bottom;
        self
    }

    pub fn padding_left(mut self, left: f64) -> Self {
        self.padding_left = left;
        self
    }

    pub fn padding_right(mut self, right: f64) -> Self {
        self.padding_right = right;
        self
    }

    pub fn padding_horizontal(mut self, left: f64, right: f64) -> Self {
        self.padding_left = left;
        self.padding_right = right;
        self
    }

    pub fn padding_vertical(mut self, top: f64, bottom: f64) -> Self {
        self.padding_top = top;
        self.padding_bottom = bottom;
        self
    }

    pub fn margin(mut self, top: f64, bottom: f64, left: f64, right: f64) -> Self {
        self.margin_top = top;
        self.margin_bottom = bottom;
        self.margin_left = left;
        self.margin_right = right;
        self
    }

    pub fn margin_top(mut self, top: f64) -> Self {
        self.margin_top = top;
        self
    }

    pub fn margin_bottom(mut self, bottom: f64) -> Self {
        self.margin_bottom = bottom;
        self
    }

    pub fn margin_left(mut self, left: f64) -> Self {
        self.margin_left = left;
        self
    }

    pub fn margin_right(mut self, right: f64) -> Self {
        self.margin_right = right;
        self
    }

    pub fn margin_horizontal(mut self, left: f64, right: f64) -> Self {
        self.margin_left = left;
        self.margin_right = right;
        self
    }

    pub fn margin_vertical(mut self, top: f64, bottom: f64) -> Self {
        self.margin_top = top;
        self.margin_bottom = bottom;
        self
    }
}

impl IntoUiNode for Item {
    fn into_ui_node(self, _ctx: &mut crate::context::CaaqiCtx) -> crate::context::DetachedNode {
        _ctx.create_node((
            Sizing {
                inner_width: self.width,
                inner_height: self.height,
                margin_bottom: self.margin_bottom,
                margin_left: self.margin_left,
                margin_right: self.margin_right,
                margin_top: self.margin_top,
                padding_bottom: self.padding_bottom,
                padding_left: self.padding_left,
                padding_right: self.padding_right,
                padding_top: self.padding_top,
                ..Default::default()
            },
            Drawing { color: self.color },
            Positioning {
                main_axis: self.main_axis,
                ..Default::default()
            },
        ))
    }
}

impl CanHaveChildren for Item {}
