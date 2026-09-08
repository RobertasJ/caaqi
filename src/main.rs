use std::str::FromStr;

use bevy::{camera::visibility::NoFrustumCulling, prelude::*};
use bevy_vello::{integrations::scene::VelloScene2d, render::VelloView};

use bevy_caaqi::context::{detached_scope, node, scope};
use bevy_caaqi::{
    CaaqiPlugin, CaaqiUi, CaaqiUiRoot, context::WorldContext, element::Item,
    element::node::positioning::Direction,
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

    world.spawn((
        VelloScene2d::default(),
        NoFrustumCulling,
        CaaqiUi,
        CaaqiUiRoot(root_node),
    ));
}
