pub mod context;
pub mod element;
pub mod node_components;

use bevy::{camera::visibility::NoFrustumCulling, prelude::*};
use bevy_vello::{
    VelloPlugin,
    integrations::scene::VelloScene2d,
    vello::{kurbo, peniko},
};

pub use bevy_vello::vello::peniko::Color;

use crate::{
    context::DetachedNode,
    node_components::{
        Computed, Drawing, ElementNode, Positioning, Sizing, positioning::Direction,
    },
};

#[derive(Debug, Default, Component)]
pub struct CaaqiUi;

#[derive(Debug, Component)]
pub struct CaaqiUiRoot(pub DetachedNode);

/// SystemSet for Caaqi UI layout and rendering operations.
///
/// Each variant represents a stage in the UI processing pipeline.
/// External systems can order before or after specific stages using `in_set()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, SystemSet)]
pub enum CaaqiUiSystems {
    /// Size calculation pass (bottom-up traversal).
    Sizing,
    /// Position calculation pass (top-down traversal).
    Positioning,
    /// Render instruction generation.
    Drawing,
    /// Final transform updates and rendering.
    Rendering,
}

pub struct CaaqiPlugin;

impl Plugin for CaaqiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(VelloPlugin::default())
            .add_systems(
                Update,
                (
                    size_uis.in_set(CaaqiUiSystems::Sizing),
                    position_uis.in_set(CaaqiUiSystems::Positioning),
                    draw_uis.in_set(CaaqiUiSystems::Drawing),
                )
                    .chain()
                    .run_if(ui_changed),
            )
            .add_systems(
                Update,
                move_ui_to_origin
                    .in_set(CaaqiUiSystems::Rendering)
                    .run_if(window_changed),
            );
    }
}

fn ui_changed(nodes: Query<Entity, (With<ElementNode>, Without<Computed>)>) -> bool {
    !nodes.is_empty()
}

fn window_changed(windows: Query<Entity, Changed<Window>>) -> bool {
    !windows.is_empty()
}

fn size_uis(
    mut scenes: Query<&CaaqiUiRoot, With<CaaqiUi>>,
    mut tree: Query<(Option<&Children>, Option<&ChildOf>), With<ElementNode>>,
    mut sizings: Query<&mut Sizing, With<ElementNode>>,
    positionings: Query<&Positioning, With<ElementNode>>,
    window: Single<&Window>,
) {
    for CaaqiUiRoot(DetachedNode(root)) in &mut scenes {
        fn bottom_up_traverse(
            node: Entity,
            tree: &Query<(Option<&Children>, Option<&ChildOf>), With<ElementNode>>,
            sizings: &mut Query<&mut Sizing, With<ElementNode>>,
            positionings: &Query<&Positioning, With<ElementNode>>,
        ) -> Sizing {
            let positioning = positionings.get(node).unwrap();
            let sizing = sizings.get(node).unwrap();

            let mut inner_width: f64 = sizing.margin_left + sizing.margin_right;
            let mut inner_height: f64 = sizing.margin_top + sizing.margin_bottom;

            if let Some(children) = tree.get(node).unwrap().0 {
                for child in children.iter() {
                    let child_sizing = bottom_up_traverse(child, tree, sizings, positionings);

                    match positioning.main_axis {
                        Direction::Horizontal => {
                            inner_width += child_sizing.inner_width.unwrap();
                            inner_height = inner_height.max(child_sizing.inner_width.unwrap());
                        }
                        Direction::Vertical => {
                            inner_width = inner_width.max(child_sizing.inner_width.unwrap());
                            inner_height += child_sizing.inner_width.unwrap();
                        }
                    }
                }
            }

            let mut sizing = sizings.get_mut(node).unwrap();

            if sizing.inner_width.is_none() {
                sizing.inner_width = Some(inner_width);
            }
            if sizing.inner_width.is_none() {
                sizing.inner_width = Some(inner_height);
            }

            *sizing
        }

        bottom_up_traverse(*root, &tree, &mut sizings, &positionings);
    }
}

fn position_uis(
    mut scenes: Query<&CaaqiUiRoot, With<CaaqiUi>>,
    mut tree: Query<(Option<&Children>, Option<&ChildOf>), With<ElementNode>>,
    mut positionings: Query<&mut Positioning, With<ElementNode>>,
    sizings: Query<&Sizing, With<ElementNode>>,
    window: Single<&Window>,
) {
    for CaaqiUiRoot(DetachedNode(root)) in &mut scenes {
        fn top_down_traverse(
            node: Entity,
            tree: &Query<(Option<&Children>, Option<&ChildOf>), With<ElementNode>>,
            positionings: &mut Query<&mut Positioning, With<ElementNode>>,
            sizings: &Query<&Sizing, With<ElementNode>>,
            position_x: f64,
            position_y: f64,
        ) -> Sizing {
            let positioning = positionings.get(node).unwrap().clone();
            let sizing = sizings.get(node).unwrap();

            let mut child_x: f64 = position_x + sizing.padding_left;
            let mut child_y: f64 = position_y + sizing.padding_top;

            if let Some(children) = tree.get(node).unwrap().0 {
                for child in children.iter() {
                    let child_sizing =
                        top_down_traverse(child, tree, positionings, sizings, child_x, child_y);

                    match positioning.main_axis {
                        Direction::Horizontal => {
                            child_x += child_sizing.outer_width();
                        }
                        Direction::Vertical => {
                            child_y += child_sizing.outer_height();
                        }
                    }
                }
            }

            let mut positioning = positionings.get_mut(node).unwrap();

            if positioning.x.is_none() {
                positioning.x = Some(position_x);
            }
            if positioning.y.is_none() {
                positioning.y = Some(position_y);
            }

            *sizing
        }

        top_down_traverse(*root, &tree, &mut positionings, &sizings, 0.0, 0.0);
    }
}

fn move_ui_to_origin(
    mut scenes: Query<(&mut Transform, &CaaqiUiRoot), With<CaaqiUi>>,
    windows: Single<&Window>,
) {
    for (mut transform, CaaqiUiRoot(DetachedNode(root))) in &mut scenes {
        transform.translation = Vec3::new(windows.width() / -2.0, windows.height() / 2.0, 0.0);
    }
}

fn draw_uis(
    mut scenes: Query<(&mut Transform, &mut VelloScene2d, &CaaqiUiRoot), With<CaaqiUi>>,
    nodes: Query<(&Drawing, &Sizing, &Positioning, Option<&Children>), With<ElementNode>>,
    window: Single<&Window>,
) {
    for (mut transform, mut scene, CaaqiUiRoot(DetachedNode(root))) in &mut scenes {
        scene.reset();

        fn recursive_draw(
            node: Entity,
            nodes: &Query<(&Drawing, &Sizing, &Positioning, Option<&Children>), With<ElementNode>>,
            scene: &mut VelloScene2d,
        ) {
            let (drawing, sizing, positioning, children) = nodes.get(node).unwrap();

            let x = positioning.x.unwrap_or_else(|| {
                info!("Node {:?} has no x position, defaulting to 0.0", node);

                0.0
            });
            let y = positioning.y.unwrap_or_else(|| {
                info!("Node {:?} has no y position, defaulting to 0.0", node);

                0.0
            });

            let rect = kurbo::Rect::from_origin_size(
                (x + sizing.margin_left, y + sizing.margin_top),
                (sizing.visible_width(), sizing.visible_height()),
            );

            scene.fill(
                peniko::Fill::NonZero,
                Default::default(),
                drawing.color,
                None,
                &rect,
            );

            if let Some(children) = children {
                for child in children.iter() {
                    recursive_draw(child, nodes, scene);
                }
            }
        }

        recursive_draw(*root, &nodes, &mut scene);
    }
}
