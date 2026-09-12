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
        .add_systems(Startup, (setup_ui,))
        .run();
}

fn setup_ui(mut commands: Commands) {
    commands.spawn(Camera2d);

    // it took me 3 weeks to come up with this... and i've barely gotten started with the implementation.
    // without using Refs, action blocks do nothing, they just wrap code.
    // when you create a ref_ you give it an initial value, .write on a ref is the same as trigering a
    // re-run of the whole action tree, but as if ref_ had the initial value set to what you wrote to it
    // when calling .write.
    // when the rerun happens, it skips running action blocks that don't depend on the ref_ that was written to.
    // this exaple is also still pretty incomplete since i haven't
    // implemented rewind blocks yet which would clean up changes to controlled global state.

    // most convoluted way to keep ui up to date.
    // code is a bit messy, but it works. this is the simplest example without any abstractions so it's why it looks messy.
    defer_action_eval(commands.reborrow(), |world| {
        // this root action block
        let mut color = ref_(world, Color::BLACK);
        let init_color = *color.silent_read(&mut *world);
        // this never gets run again. the root scope doesn't depend on color
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

                // need to make the rewrites mark action blocks as stale
                world.commands().queue(FlushWrites);
                // queues the evaluation of the action tree. Make sure your action tree converges to a stable state!!
                world.commands().queue(ExecuteActionTrees);
            })
            .observe(move |_ev: On<Pointer<Leave>>, mut world: DeferredWorld| {
                info!("Pointer left node");
                color.set(world.reborrow(), Color::BLACK);

                world.commands().queue(FlushWrites);
                world.commands().queue(ExecuteActionTrees);
            });

        // gets rerun when the color is changed
        // action blocks can be run multiple times by the executor
        action(world, move |world| {
            world
                .entity_mut(root_node)
                .get_mut::<BackgroundColor>()
                .unwrap()
                .0 = *color.read(&mut *world);
        });
    });

    // run after the system exits
}
