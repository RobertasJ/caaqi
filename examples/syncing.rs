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
        let mut numbers = ref_(world, vec![]);
        let sync_key = sync_key(world);
        let mut count = ref_(world, 0);

        action(world, move |world| {
            numbers.silent_write(&mut *world).push(1);
            numbers.notify_forward_only(&mut *world);

            synced_rewind(world, [sync_key], move |world| {
                numbers.write(world).pop();
            });
        });

        action(world, move |world| {
            sync_point(world, [sync_key]);

            println!("state of numbers: {:?}", *numbers.read(world));
        });

        action(world, move |world| {
            let read = count.read(&mut *world);
            numbers.silent_write(&mut *world).push(*read);
            numbers.notify_forward_only(&mut *world);

            synced_rewind(world, [sync_key], move |world| {
                numbers.write(world).pop();
            });
        });

        action(world, move |world| {
            numbers.silent_write(&mut *world).push(69);
            numbers.notify_forward_only(&mut *world);

            synced_rewind(world, [sync_key], move |world| {
                numbers.write(world).pop();
            });
        });

        action(world, move |world| {
            if *count.read(&mut *world) < 3 {
                *count.write(world) += 1;
            }
        });
    });
}
