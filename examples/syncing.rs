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
        let mut numbers = ref_(vec![]);
        let sync_key = sync_key();
        let mut count = ref_(0);

        action(move || {
            numbers.silent_write().push(1);
            numbers.notify_forward_only();

            synced_rewind([sync_key], move || {
                numbers.silent_write().pop();
                numbers.notify_forward_only();
            });
        });

        action(move || {
            sync_point([sync_key]);

            println!("state of numbers: {:?}", *numbers.read());
        });

        action(move || {
            let read = count.read();
            numbers.silent_write().push(*read);
            numbers.notify_forward_only();

            synced_rewind([sync_key], move || {
                numbers.silent_write().pop();
                numbers.notify_forward_only();
            });
        });

        action(move || {
            numbers.silent_write().push(69);
            numbers.notify_forward_only();

            synced_rewind([sync_key], move || {
                numbers.silent_write().pop();
                numbers.notify_forward_only();
            });
        });

        action(move || {
            if *count.read() < 3 {
                *count.write() += 1;
            }
        });
    });
}
