use bevy::{
    MinimalPlugins,
    app::{App, Startup},
    ecs::system::Commands,
    log::LogPlugin,
};
use bevy_caaqi::{
    CaaqiPlugin,
    action::context_builder::{action, defer_action_eval},
    tracked_value::ref_,
};

fn main() {
    App::new()
        .add_plugins((MinimalPlugins, LogPlugin::default(), CaaqiPlugin))
        .add_systems(Startup, queue_eval)
        .run();
}

fn queue_eval(commands: Commands) {
    defer_action_eval(commands, move |world| {
        let mut count = ref_(world, 0);

        // expect this to print 0, and then 10, the second print happens because of writing to count
        action(world, move |world| {
            println!("Count: {}", *count.read(&mut *world));
        });

        action(world, move |world| {
            for _ in 0..10 {
                *count.write(&mut *world) += 1;
            }
        });
    });
}
