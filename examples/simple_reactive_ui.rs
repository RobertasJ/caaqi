use bevy::{ecs::world::DeferredWorld, prelude::*};

use bevy_caaqi::{
    CaaqiPlugin,
    action::{
        context_builder::{action, defer_action_eval, rewind, synced_rewind},
        execute::{ExecuteActionTrees, FlushWrites},
        sync::{SyncKey, sync_key},
    },
    tracked_value::{Ref, ref_},
    var::{Var, var},
};

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, CaaqiPlugin))
        .add_systems(Startup, (setup_ui,))
        .run();
}

fn setup_ui(mut commands: Commands) {
    commands.spawn(Camera2d);

    defer_action_eval(commands.reborrow(), |world| {
        let mut root_node = NodeMutator::new(world);

        root_node.observe(world, move |_ev: On<Pointer<Click>>, _world| {
            println!("Root node clicked!");
        });

        root_node.set_width(world, 500.0);
        root_node.set_height(world, 600.0);

        action(world, move |world| {
            if *root_node.is_hovered.read(&mut *world) {
                root_node.set_color(&mut *world, Color::hsl(0.0, 0.9, 0.8));
            } else {
                root_node.set_color(&mut *world, Color::hsl(0.0, 0.9, 0.5));
            }
        });

        action(world, move |world| {
            let mut child = NodeMutator::new(world);
            root_node.add_child(world, child);

            child.set_width(world, 100.0);
            child.set_height(world, 100.0);

            action(world, move |world| {
                if *child.is_hovered.read(&mut *world) {
                    child.set_color(&mut *world, Color::hsl(100.0, 0.9, 0.8));
                } else {
                    child.set_color(&mut *world, Color::hsl(100.0, 0.9, 0.5));
                }
            });
        });

        action(world, move |world| {
            if *root_node.is_hovered.read(&mut *world) {
                let mut child = NodeMutator::new(world);
                root_node.add_child(world, child);
                child.set_width(world, 100.0);
                child.set_height(world, 100.0);
                child.set_color(&mut *world, Color::hsl(200.0, 0.9, 0.5));
            } else {
                let mut long_child = NodeMutator::new(world);
                root_node.add_child(world, long_child);
                long_child.set_width(world, 400.0);
                long_child.set_height(world, 100.0);
                long_child.set_color(&mut *world, Color::hsl(300.0, 0.9, 0.5));
            }
        });

        for _ in 0..5 {
            action(world, move |world| {
                let mut child = NodeMutator::new(world);
                root_node.add_child(world, child);

                child.set_width(world, 100.0);
                child.set_height(world, 100.0);

                action(world, move |world| {
                    if *child.is_hovered.read(&mut *world) {
                        child.set_color(&mut *world, Color::hsl(100.0, 0.9, 0.8));
                    } else {
                        child.set_color(&mut *world, Color::hsl(100.0, 0.9, 0.5));
                    }
                });
            });
        }
    });
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeMutator {
    node_entity: Entity,
    sync_key: SyncKey,
    is_hovered: Ref<bool>,
}

impl NodeMutator {
    fn new(world: &mut World) -> Self {
        let node_entity = world
            .spawn(Node {
                ..Default::default()
            })
            .id();

        rewind(world, move |world| {
            world.entity_mut(node_entity).despawn();
        });

        let mut is_hovered = ref_(world, false);

        let mut self_ = Self {
            node_entity,
            sync_key: sync_key(world),
            is_hovered,
        };

        self_.observe(world, move |_ev: On<Pointer<Enter>>, world| {
            is_hovered.set(world, true);
        });

        self_.observe(world, move |_ev: On<Pointer<Leave>>, world| {
            is_hovered.set(world, false);
        });

        self_
    }

    fn set_color<'a>(&mut self, world: impl Into<DeferredWorld<'a>>, color: Color) {
        world
            .into()
            .entity_mut(self.node_entity)
            .get_mut::<BackgroundColor>()
            .unwrap()
            .0 = color;
    }

    fn set_width(&mut self, world: &mut World, width: f32) {
        world
            .entity_mut(self.node_entity)
            .get_mut::<Node>()
            .unwrap()
            .width = Val::Px(width);
    }

    fn set_height(&mut self, world: &mut World, height: f32) {
        world
            .entity_mut(self.node_entity)
            .get_mut::<Node>()
            .unwrap()
            .height = Val::Px(height);
    }

    fn add_child(&mut self, world: &mut World, child: NodeMutator) -> NodeMutator {
        world
            .entity_mut(self.node_entity)
            .add_child(child.node_entity);

        let self_ = *self;
        synced_rewind(world, [self.sync_key], move |world| {
            world
                .entity_mut(self_.node_entity)
                .detach_child(child.node_entity);
        });

        child
    }

    fn observe<E: EntityEvent + 'static>(
        &mut self,
        world: &mut World,
        mut callback: impl FnMut(On<E>, DeferredWorld) + Send + Sync + 'static,
    ) {
        world
            .entity_mut(self.node_entity)
            .observe(move |ev: On<E>, mut world: DeferredWorld| {
                callback(ev, world.reborrow());

                world.commands().queue(FlushWrites);
                world.commands().queue(ExecuteActionTrees);
            });
    }
}
