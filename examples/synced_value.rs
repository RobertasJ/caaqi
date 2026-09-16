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
        let mut numbers = var(vec![]);

        action(move || {
            if *count.read() != 4 {
                action(move || {
                    numbers.write().push(1);
                });
            }
        });

        action(move || {
            println!("state of numbers: {:?}", *numbers.read());
        });

        action(move || {
            let read = count.read();
            numbers.write().push(*read);
        });

        action(move || {
            numbers.write().push(69);
            let read = numbers.read();
            println!("state of numbers end: {:?}", *read);
        });

        action(move || {
            if *count.read() < 5 {
                *count.write() += 1;
            }
        });
    });
}
