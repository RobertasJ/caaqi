use bevy::{camera::visibility::NoFrustumCulling, prelude::*};
use bevy_vello::{integrations::scene::VelloScene2d, render::VelloView};

use bevy_caaqi::{CaaqiPlugin, CaaqiUi};

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, CaaqiPlugin))
        .add_systems(Startup, (setup_camera))
        .run();
}

fn setup_camera(mut commands: Commands) {
    commands.spawn((Camera2d, VelloView));

    commands.spawn((VelloScene2d::default(), NoFrustumCulling, CaaqiUi));
}

// fn build_ui(mut commands: Commands, ui: Single<Entity, With<CaaqiUi>>) {
//     let tree = build_ui_tree(Node::new(), |_| {
//         node_scope::<Node>(|_| {
//             // regular items
//             node(Item);
//             node(Item);

//             for _ in 0..5 {
//                 node_scope::<Node>(|_| {
//                     node(Item);
//                     node(Item);

//                     for _ in 0..5 {
//                         node(Item);
//                     }
//                 });
//             }
//         });
//     });

//     commands.entity(*ui).insert(tree);
// }
