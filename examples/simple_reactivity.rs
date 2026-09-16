use bevy::{
    MinimalPlugins,
    app::{App, Startup},
    ecs::system::Commands,
    log::LogPlugin,
};
use bevy_caaqi::prelude::*;

fn main() {
    App::new()
        .add_plugins((MinimalPlugins, LogPlugin::default(), CaaqiPlugin))
        .add_systems(Startup, queue_eval)
        .run();
}

fn queue_eval(commands: Commands) {
    defer_action_eval(commands, move || {
        let mut count = ref_(0);

        // expect this to print 0, and then 10, the second print happens because of writing to count
        action(move || {
            println!("Count: {}", *count.read());
        });

        action(move || {
            for _ in 0..10 {
                *count.write() += 1;
            }
        });
    });
}
