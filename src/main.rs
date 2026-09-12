use bevy::{ecs::world::DeferredWorld, prelude::*};

use bevy_caaqi::{
    CaaqiPlugin,
    action::{
        context_builder::{
            action, action_root, defer_action_eval, detached_action, run_action_node,
        },
        execute::{ExecuteActionTrees, FlushWrites},
    },
    tracked_value::{Ref, ref_, ref_action},
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
        let mut color = ref_(world, Color::BLACK);
        let init_color = *color.silent_read(&mut *world);
        let mut entity_mut = world.spawn((
            Node {
                width: Val::Px(100.0),
                height: Val::Px(100.0),
                ..Default::default()
            },
            BackgroundColor(init_color),
        ));
        let root_node = entity_mut.id();
        entity_mut
            .observe(move |_ev: On<Pointer<Enter>>, mut world: DeferredWorld| {
                info!("Pointer entered node");
                color.set(world.reborrow(), Color::WHITE);

                world.commands().queue(FlushWrites);
                world.commands().queue(ExecuteActionTrees);
            })
            .observe(move |_ev: On<Pointer<Leave>>, mut world: DeferredWorld| {
                info!("Pointer left node");
                color.set(world.reborrow(), Color::BLACK);

                world.commands().queue(FlushWrites);
                world.commands().queue(ExecuteActionTrees);
            });

        action(world, move |world| {
            world
                .entity_mut(root_node)
                .get_mut::<BackgroundColor>()
                .unwrap()
                .0 = *color.read(&mut *world);
        });
    });
}
