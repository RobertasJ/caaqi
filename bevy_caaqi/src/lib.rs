pub mod context;

use bevy::{camera::visibility::NoFrustumCulling, prelude::*};
use bevy_vello::{
    VelloPlugin,
    integrations::scene::VelloScene2d,
    vello::{kurbo, peniko},
};
use std::str::FromStr;

pub use bevy_vello::vello::peniko::Color;

use crate::context::ui_node::DetachedNode;

#[derive(Debug, Default, Component)]
pub struct CaaqiUi;

#[derive(Debug, Component)]
pub struct CaaqiUiRoot(pub DetachedNode);

pub struct CaaqiPlugin;

impl Plugin for CaaqiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(VelloPlugin::default())
            .add_systems(Update, (draw_uis).chain());
    }
}

pub fn draw_uis(
    mut scenes: Query<(&mut Transform, &mut VelloScene2d), With<CaaqiUi>>,
    window: Single<&Window>,
) {
    for (mut transform, mut scene) in &mut scenes {
        transform.translation = Vec3::new(window.width() / -2.0, window.height() / 2.0, 0.0);

        scene.reset();

        scene.fill(
            peniko::Fill::NonZero,
            kurbo::Affine::default(),
            peniko::Color::from_str("#111111").unwrap(),
            None,
            &kurbo::Rect::new(0.0, 0.0, 50.0, 50.0),
        );
    }
}

pub fn spawn_ui_scene(commands: &mut Commands) {
    commands.spawn((VelloScene2d::default(), NoFrustumCulling, CaaqiUi));
}
