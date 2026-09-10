use std::str::FromStr;

use bevy::{camera::visibility::NoFrustumCulling, prelude::*};
use bevy_vello::{integrations::scene::VelloScene2d, render::VelloView};

use bevy_caaqi::{
    CaaqiPlugin, CaaqiUi, CaaqiUiRoot,
    action::context_builder::{action, action_rewind, detached_action, run_action_node},
    element::{
        Item,
        context_builder::{detached_scope, node, scope},
        node::positioning::Direction,
    },
    tracked_value::{Ref, create_ref, ref_, ref_action},
    world_context::WorldContext,
};

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, CaaqiPlugin))
        .add_systems(Startup, (setup_camera,))
        .run();
}

fn setup_camera(world: &mut World) {
    world.spawn((Camera2d, VelloView));

    let root_node = WorldContext::new(world).enter(|| {
        let heights = 100.0;

        detached_scope::<Item>(|_| {
            node(|item: &mut Item| {
                item.color(bevy_caaqi::Color::from_str("#3fefd2").unwrap());
                item.width(100.0);
                item.height(200.0);
            });

            scope(|item: &mut Item| {
                item.main_axis(Direction::Horizontal);

                node(|item: &mut Item| {
                    item.color(bevy_caaqi::Color::from_str("#943fef").unwrap());
                    item.width(80.0);
                    item.height(80.0);
                    item.margin_all(10.0);
                });

                node(|item: &mut Item| {
                    item.color(bevy_caaqi::Color::from_str("#3fef4b").unwrap());
                    item.width(100.0);
                    item.height(heights);
                });
            });
        })
    });

    let decision_root = WorldContext::new(world).enter(|| {
        let node = detached_action(|| {
            println!("Decision 1");

            action(|| {
                println!("Decision 2");
            });

            let num = ref_(5);
            let condition = ref_(true);

            let memo = ref_action(move || if *condition.read() { "hello" } else { "world" });

            action(move || {
                if *condition.read() {
                    action(move || {
                        println!("Decision 3 with num: {}", *num.read());
                        action(move || {
                            println!("Decision 4: {}", *memo.read());

                            action_rewind(|| {
                                // undo the println 4
                            });
                        });

                        action_rewind(|| {
                            // undo the println 3
                        });
                    });
                } else {
                    action(move || {
                        println!("Decision 5 with num * 2: {}", *num.read() * 2);

                        action_rewind(|| {
                            // undo the println 5
                        });
                    });
                }
            });

            action_rewind(|| {
                // undo the println 1
                // undo the println 2
            });
        });

        run_action_node(node);

        node
    });

    world.spawn((
        VelloScene2d::default(),
        NoFrustumCulling,
        CaaqiUi,
        CaaqiUiRoot(root_node),
    ));
}
