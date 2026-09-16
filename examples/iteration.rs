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
    defer_action_eval(commands, || {
        let mut elements = var(vec![100, 13, 035, 111111, 523]);
        let mut modularity_check = state(5);

        action(move || {
            println!("{:?}", *elements.read());
        });

        action(move || {
            let modulus = *modularity_check.read();
            let collect = elements
                .read()
                .iter()
                .filter(|num| **num % modulus == 0)
                .copied()
                .collect();
            *elements.write() = collect;
        });

        action(move || {
            println!("{:?}", *elements.read());
        });

        action(move || {
            let read = *modularity_check.read();
            match read {
                5 => modularity_check.set(3),
                3 => modularity_check.set(2),
                _ => {}
            }
        });
    });
}
