use bevy_vello::vello::peniko;

use super::{CanHaveChildren, Element};
use crate::context::{DetachedNode, NodeCreationCtx};
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

    pub fn color(&mut self, color: peniko::Color) {
        self.color = color;
    }

    pub fn width(&mut self, width: f64) {
        self.width = Some(width);
    }

    pub fn height(&mut self, height: f64) {
        self.height = Some(height);
    }

    pub fn main_axis(&mut self, main_axis: Direction) {
        self.main_axis = main_axis;
    }

    pub fn padding_all(&mut self, padding: f64) {
        self.padding_top = padding;
        self.padding_bottom = padding;
        self.padding_left = padding;
        self.padding_right = padding;
    }

    pub fn padding_top(&mut self, top: f64) {
        self.padding_top = top;
    }

    pub fn padding_bottom(&mut self, bottom: f64) {
        self.padding_bottom = bottom;
    }

    pub fn padding_left(&mut self, left: f64) {
        self.padding_left = left;
    }

    pub fn padding_right(&mut self, right: f64) {
        self.padding_right = right;
    }

    pub fn padding_horizontal(&mut self, left: f64, right: f64) {
        self.padding_left = left;
        self.padding_right = right;
    }

    pub fn padding_vertical(&mut self, top: f64, bottom: f64) {
        self.padding_top = top;
        self.padding_bottom = bottom;
    }

    pub fn margin_all(&mut self, margin: f64) {
        self.margin_top = margin;
        self.margin_bottom = margin;
        self.margin_left = margin;
        self.margin_right = margin;
    }

    pub fn margin_top(&mut self, top: f64) {
        self.margin_top = top;
    }

    pub fn margin_bottom(&mut self, bottom: f64) {
        self.margin_bottom = bottom;
    }

    pub fn margin_left(&mut self, left: f64) {
        self.margin_left = left;
    }

    pub fn margin_right(&mut self, right: f64) {
        self.margin_right = right;
    }

    pub fn margin_horizontal(&mut self, left: f64, right: f64) {
        self.margin_left = left;
        self.margin_right = right;
    }

    pub fn margin_vertical(&mut self, top: f64, bottom: f64) {
        self.margin_top = top;
        self.margin_bottom = bottom;
    }
}

impl Element for Item {
    fn into_ui_node(self, ctx: &mut NodeCreationCtx) -> DetachedNode {
        ctx.create_node((
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
