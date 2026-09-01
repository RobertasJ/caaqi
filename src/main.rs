use std::str::FromStr;

use bevy::{camera::visibility::NoFrustumCulling, prelude::*};
use bevy_vello::{integrations::scene::VelloScene2d, render::VelloView};

use bevy_caaqi::{
    CaaqiPlugin, CaaqiUi, CaaqiUiRoot,
    context::{
        CaaqiCtx, enter_caaqi_ctx,
        ui_node::{Group, Item, detached_node_scope, node},
    },
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
        detached_node_scope::<Group>(|_| {
            node(
                Item::new(bevy_caaqi::Color::from_str("#3fefd2").unwrap())
                    .width(100.0)
                    .height(100.0),
            );
        })
    });

    commands.spawn((
        VelloScene2d::default(),
        NoFrustumCulling,
        CaaqiUi,
        CaaqiUiRoot(root_node),
    ));
}
