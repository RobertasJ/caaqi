use bevy::{
    MinimalPlugins,
    app::{App, Startup},
    ecs::system::Commands,
    log::LogPlugin,
};
use bevy_caaqi::{
    CaaqiPlugin,
    action::{
        context_builder::{action, defer_action_eval, synced_rewind},
        sync::{sync_key, sync_point},
    },
    synced_value::synced_ref,
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
        let mut numbers = synced_ref(world, vec![]);

        action(world, move |world| {
            if *count.read(&mut *world) != 4 {
                action(world, move |world| {
                    numbers.write(world).push(1);
                });
            }
        });

        action(world, move |world| {
            println!("state of numbers: {:?}", *numbers.read(world));
        });

        action(world, move |world| {
            let read = count.read(&mut *world);
            numbers.write(world).push(*read);
        });

        action(world, move |world| {
            numbers.write(world).push(69);
            let read = numbers.read(world);
            println!("state of numbers end: {:?}", *read);
        });

        action(world, move |world| {
            if *count.read(&mut *world) < 5 {
                *count.write(world) += 1;
            }
        });
    });
}
