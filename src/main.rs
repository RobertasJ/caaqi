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

    defer_action_eval(commands.reborrow(), || {
        let mut count = ref_(0);
        let mut doubled = ref_(0);
        let mut done = ref_(false);

        action(move || {
            let count_value = *count.read();

            if count_value < 3 {
                *count.write() = count_value + 1;
            }
        });

        action(move || {
            let count_value = *count.read();
            *doubled.write() = count_value * 2;
        });

        action(move || {
            let doubled_value = *doubled.read();

            if doubled_value >= 6 {
                *done.write() = true;
            }
        });

        action(move || {
            let count_value = *count.read();
            let doubled_value = *doubled.read();
            let done_value = *done.read();

            // final observer
            println!(
                "Count: {}, Doubled: {}, Done: {}",
                count_value, doubled_value, done_value
            );
        });
    });
}
