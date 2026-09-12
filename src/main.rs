use bevy::prelude::*;

use bevy_caaqi::{
    CaaqiPlugin,
    action::context_builder::{
        action, action_root, defer_action_eval, detached_action, run_action_node,
    },
    tracked_value::{Ref, ref_, ref_action},
};

fn main() {
    App::new()
        .add_plugins(MinimalPlugins)
        .add_systems(Startup, (setup_camera,))
        .run();
}

fn setup_camera(mut commands: Commands) {
    commands.spawn(Camera2d);

    defer_action_eval(commands.reborrow(), |world| {
        let mut entity_mut = world.spawn((
            Node {
                width: Val::Px(100.0),
                height: Val::Px(100.0),
                ..Default::default()
            },
            BackgroundColor(Color::BLACK),
        ));
        let root_node = entity_mut.id();
        entity_mut.observe(|ev: On<Pointer<Over>>, commands: Commands<'_, '_>| {});

        action(world, move |world| {});
    });
}
