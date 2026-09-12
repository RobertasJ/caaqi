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

    defer_action_eval(commands.reborrow(), |world| {
        let mut count = ref_(world, 0);
        let mut doubled = ref_(world, 0);
        let mut done = ref_(world, false);

        action(world, move |world| {
            let count_value = *count.read(world);

            if count_value < 3 {
                *count.write(world) = count_value + 1;
            }
        });

        action(world, move |world| {
            let count_value = *count.read(world);
            *doubled.write(world) = count_value * 2;
        });

        action(world, move |world| {
            let doubled_value = *doubled.read(world);

            if doubled_value >= 6 {
                *done.write(world) = true;
            }
        });

        action(world, move |world| {
            let count_value = *count.read(world);
            let doubled_value = *doubled.read(world);
            let done_value = *done.read(world);

            // final observer
            println!(
                "Count: {}, Doubled: {}, Done: {}",
                count_value, doubled_value, done_value
            );
        });
    });
}
