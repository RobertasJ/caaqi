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
        let mut count = ref_(world, 0);

        let doubled = ref_action(world, move |world| {
            println!("[doubled] read count");
            let count_value = *count.read(world);

            println!("[doubled] write doubled = {}", count_value * 2);
            count_value * 2
        });

        let done = ref_action(world, move |world| {
            println!("[done] read count");
            let count_value = *count.read(world);

            println!("[done] write done = {}", count_value == 10);
            count_value == 10
        });

        action(world, move |world| {
            println!("[print] read done");
            let done_value = *done.read(world);

            if done_value {
                println!("[print] read count");
                let count_value = *count.read(world);

                println!("Done! Count: {}", count_value);
            } else {
                println!("[print] read count");
                let count_value = *count.read(world);

                println!("[print] read doubled");
                let doubled_value = *doubled.read(world);

                println!("Count: {}, Doubled: {}", count_value, doubled_value);
            }
        });

        action(world, move |world| {
            println!("[increment] read done");
            let done_value = *done.read(world);

            if !done_value {
                println!("[increment] write count");
                *count.write(world) += 1;

                println!("[increment] finished count write");
            } else {
                println!("[increment] no write");
            }
        });
    });
}

struct SizeData {
    width: Ref<f32>,
    height: Ref<f32>,
    inner_width: Ref<f32>,
    inner_height: Ref<f32>,
}

struct LayoutNode {
    size: SizeData,
    children: Vec<LayoutNode>,
}

impl LayoutNode {
    fn new(world: &mut World, width: f32, height: f32) -> Self {
        let size = SizeData {
            width: ref_(world, width),
            height: ref_(world, height),
            inner_width: ref_(world, width),
            inner_height: ref_(world, height),
        };

        Self {
            size,
            children: Vec::new(),
        }
    }

    fn add_child(&mut self, child: LayoutNode) {
        self.children.push(child);
    }

    // fn calculate_size(&self, world: &mut World) {
    //     let width = *self.size.width.get(world);
    //     let height = *self.size.height.get(world);

    //     self.size.inner_width.init(world, width);
    //     self.size.inner_height.init(world, height);

    //     for child in &self.children {
    //         child.calculate_size(world);
    //     }
    // }
}
