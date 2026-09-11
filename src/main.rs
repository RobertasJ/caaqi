use bevy::{ecs::world::DeferredWorld, prelude::*};

use bevy_caaqi::{
    CaaqiPlugin,
    action::context_builder::{action, action_root, detached_action, run_action_node},
    tracked_value::{ref_, ref_action},
};
use caaqi_context::WorldContext;

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, CaaqiPlugin))
        .add_systems(Startup, (setup_camera,))
        .run();
}

fn setup_camera(mut commands: Commands) {
    commands.spawn(Camera2d);

    commands.spawn(action_root(|| {
        let mut count = ref_(0);

        let cond = ref_(true);

        action(move || {
            if *cond.read() {
                action(move || {
                    *count.write() = 1;
                });
            }
        });

        action(move || {
            println!("Count: {}", *count.read());
        });
    }));
}
