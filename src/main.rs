use std::str::FromStr;

use bevy::{camera::visibility::NoFrustumCulling, prelude::*};
use bevy_vello::{integrations::scene::VelloScene2d, render::VelloView};

use bevy_caaqi::context::{node, node_scoped, node_scoped_detached};
use bevy_caaqi::{
    CaaqiPlugin, CaaqiUi, CaaqiUiRoot,
    context::{CaaqiCtx, enter_caaqi_ctx},
    element::Item,
    node_components::positioning::Direction,
};

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, CaaqiPlugin))
        .add_systems(Startup, (setup_camera,))
        .run();
}

fn setup_camera(mut commands: Commands) {
    commands.spawn((Camera2d, VelloView));

    let root_node = enter_caaqi_ctx(&mut CaaqiCtx::new(commands.reborrow()), || {
        let heights = 100.0;

        node_scoped_detached::<Item>(|_| {
            node(|item: &mut Item| {
                item.color(bevy_caaqi::Color::from_str("#3fefd2").unwrap())
                    .width(100.0)
                    .height(200.0);
            });

            node_scoped(|item: &mut Item| {
                item.main_axis(Direction::Horizontal);
                node(|item: &mut Item| {
                    item.color(bevy_caaqi::Color::from_str("#943fef").unwrap())
                        .width(80.0)
                        .height(80.0)
                        .margin(10.0, 10.0, 10.0, 10.0);
                });

                node(|item: &mut Item| {
                    item.color(bevy_caaqi::Color::from_str("#3fef4b").unwrap())
                        .width(100.0)
                        .height(heights);
                });
            });
        })
    });

    commands.spawn((
        VelloScene2d::default(),
        NoFrustumCulling,
        CaaqiUi,
        CaaqiUiRoot(root_node),
    ));
}
