use bevy::prelude::*;

use bevy_caaqi::{
    CaaqiPlugin,
    action::context_builder::{
        action, action_root, defer_action_eval, detached_action, run_action_node,
    },
    tracked_value::{ref_, ref_action},
};

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, CaaqiPlugin))
        .add_systems(Startup, (setup_camera,))
        .run();
}

fn setup_camera(mut commands: Commands) {
    commands.spawn(Camera2d);

    defer_action_eval(commands.reborrow(), |world| {});
}
