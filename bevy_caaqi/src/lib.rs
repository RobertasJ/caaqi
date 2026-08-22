pub mod layout;
pub mod tree;

use bevy::{camera::visibility::NoFrustumCulling, prelude::*};
use bevy_vello::{
    VelloPlugin,
    integrations::scene::VelloScene2d,
    vello::{kurbo, peniko},
};
use std::str::FromStr;

use crate::tree::UiTree;

#[derive(Debug, Default, Component)]
pub struct CaaqiUi;

pub struct CaaqiPlugin;

impl Plugin for CaaqiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(VelloPlugin::default())
            .add_systems(Update, (size_uis, draw_uis).chain());
    }
}

pub fn size_uis(mut ui_trees: Query<&mut UiTree, With<CaaqiUi>>, window: Single<&Window>) {
    for tree in &mut ui_trees {
        fn rec(tree: &mut UiTree) {
            // Text
        }
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
