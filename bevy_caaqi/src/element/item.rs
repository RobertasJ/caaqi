use bevy::ecs::entity::Entity;
use bevy_vello::vello::peniko;

use super::{CanHaveChildren, Element};
use crate::context_tree_builder::{DetachedNode, spawn_node};
use crate::element::ElementMutator;
use crate::element::context_builder::create_element_node;
use crate::element::node::positioning::Direction;
use crate::element::node::{Drawing, Positioning, Sizing};
use crate::world_context::WorldContext;

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
    fn into_ui_node(self, ctx: &mut WorldContext) -> DetachedNode {
        create_element_node(
            (
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
            ),
            ctx,
        )
    }
}

impl CanHaveChildren for Item {}

pub struct ItemMutator {
    entity: Entity,
}

impl ItemMutator {
    pub fn color(&mut self, color: peniko::Color) {
        WorldContext::with(|ctx| {
            ctx.entity_mut(self.entity)
                .get_components_mut::<&mut Drawing>()
                .unwrap()
                .color = color;
        });
    }

    pub fn width(&mut self, width: f64) {
        WorldContext::with(|ctx| {
            ctx.entity_mut(self.entity)
                .get_components_mut::<&mut Sizing>()
                .unwrap()
                .inner_width = Some(width);
        });
    }

    pub fn height(&mut self, height: f64) {
        WorldContext::with(|ctx| {
            ctx.entity_mut(self.entity)
                .get_components_mut::<&mut Sizing>()
                .unwrap()
                .inner_height = Some(height);
        });
    }

    pub fn main_axis(&mut self, main_axis: Direction) {
        WorldContext::with(|ctx| {
            ctx.entity_mut(self.entity)
                .get_components_mut::<&mut Positioning>()
                .unwrap()
                .main_axis = main_axis;
        });
    }

    pub fn padding_all(&mut self, padding: f64) {
        WorldContext::with(|ctx| {
            let mut binding = ctx.entity_mut(self.entity);
            let mut sizing = binding.get_components_mut::<&mut Sizing>().unwrap();
            sizing.padding_top = padding;
            sizing.padding_bottom = padding;
            sizing.padding_left = padding;
            sizing.padding_right = padding;
        });
    }

    pub fn padding_horizontal(&mut self, left: f64, right: f64) {
        WorldContext::with(|ctx| {
            let mut binding = ctx.entity_mut(self.entity);
            let mut sizing = binding.get_components_mut::<&mut Sizing>().unwrap();
            sizing.padding_left = left;
            sizing.padding_right = right;
        });
    }

    pub fn padding_vertical(&mut self, top: f64, bottom: f64) {
        WorldContext::with(|ctx| {
            let mut binding = ctx.entity_mut(self.entity);
            let mut sizing = binding.get_components_mut::<&mut Sizing>().unwrap();
            sizing.padding_top = top;
            sizing.padding_bottom = bottom;
        });
    }

    pub fn margin_all(&mut self, margin: f64) {
        WorldContext::with(|ctx| {
            let mut binding = ctx.entity_mut(self.entity);
            let mut sizing = binding.get_components_mut::<&mut Sizing>().unwrap();
            sizing.margin_top = margin;
            sizing.margin_bottom = margin;
            sizing.margin_left = margin;
            sizing.margin_right = margin;
        });
    }

    pub fn margin_horizontal(&mut self, left: f64, right: f64) {
        WorldContext::with(|ctx| {
            let mut binding = ctx.entity_mut(self.entity);
            let mut sizing = binding.get_components_mut::<&mut Sizing>().unwrap();
            sizing.margin_left = left;
            sizing.margin_right = right;
        });
    }

    pub fn margin_vertical(&mut self, top: f64, bottom: f64) {
        WorldContext::with(|ctx| {
            let mut binding = ctx.entity_mut(self.entity);
            let mut sizing = binding.get_components_mut::<&mut Sizing>().unwrap();
            sizing.margin_top = top;
            sizing.margin_bottom = bottom;
        });
    }

    fn padding_top(&mut self, top: f64) {
        WorldContext::with(|ctx| {
            ctx.entity_mut(self.entity)
                .get_components_mut::<&mut Sizing>()
                .unwrap()
                .padding_top = top;
        });
    }

    fn padding_bottom(&mut self, bottom: f64) {
        WorldContext::with(|ctx| {
            ctx.entity_mut(self.entity)
                .get_components_mut::<&mut Sizing>()
                .unwrap()
                .padding_bottom = bottom;
        });
    }

    fn padding_left(&mut self, left: f64) {
        WorldContext::with(|ctx| {
            ctx.entity_mut(self.entity)
                .get_components_mut::<&mut Sizing>()
                .unwrap()
                .padding_left = left;
        });
    }

    fn padding_right(&mut self, right: f64) {
        WorldContext::with(|ctx| {
            ctx.entity_mut(self.entity)
                .get_components_mut::<&mut Sizing>()
                .unwrap()
                .padding_right = right;
        });
    }

    fn margin_top(&mut self, top: f64) {
        WorldContext::with(|ctx| {
            ctx.entity_mut(self.entity)
                .get_components_mut::<&mut Sizing>()
                .unwrap()
                .margin_top = top;
        });
    }

    fn margin_bottom(&mut self, bottom: f64) {
        WorldContext::with(|ctx| {
            ctx.entity_mut(self.entity)
                .get_components_mut::<&mut Sizing>()
                .unwrap()
                .margin_bottom = bottom;
        });
    }

    fn margin_left(&mut self, left: f64) {
        WorldContext::with(|ctx| {
            ctx.entity_mut(self.entity)
                .get_components_mut::<&mut Sizing>()
                .unwrap()
                .margin_left = left;
        });
    }

    fn margin_right(&mut self, right: f64) {
        WorldContext::with(|ctx| {
            ctx.entity_mut(self.entity)
                .get_components_mut::<&mut Sizing>()
                .unwrap()
                .margin_right = right;
        });
    }
}

impl From<Entity> for ItemMutator {
    fn from(entity: Entity) -> Self {
        Self { entity }
    }
}

impl ElementMutator for ItemMutator {}
