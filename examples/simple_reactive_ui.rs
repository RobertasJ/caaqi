use bevy::{ecs::world::DeferredWorld, prelude::*};

use bevy_caaqi::{
    action::execute::{ExecuteActionTrees, FlushWrites},
    prelude::{Ref, *},
};

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, CaaqiPlugin))
        .add_systems(Startup, (setup_ui,))
        .run();
}

fn setup_ui(mut commands: Commands) {
    commands.spawn(Camera2d);

    defer_action_eval(commands.reborrow(), move || {
        let mut root_node = NodeMutator::new();

        root_node.observe(move |_ev: On<Pointer<Click>>| {
            println!("Root node clicked!");
        });

        root_node.set_width(500.0);
        root_node.set_height(600.0);

        action(move || {
            if *root_node.is_hovered.read() {
                root_node.set_color(Color::hsl(0.0, 0.9, 0.8));
            } else {
                root_node.set_color(Color::hsl(0.0, 0.9, 0.5));
            }
        });

        action(move || {
            let child = NodeMutator::new();
            action(move || {
                root_node.add_child(child);
            });

            child.set_width(100.0);
            child.set_height(100.0);

            action(move || {
                if *child.is_hovered.read() {
                    child.set_color(Color::hsl(100.0, 0.9, 0.8));
                } else {
                    child.set_color(Color::hsl(100.0, 0.9, 0.5));
                }
            });
        });

        action(move || {
            if *root_node.is_hovered.read() {
                let child = NodeMutator::new();
                action(move || {
                    root_node.add_child(child);
                });
                child.set_width(100.0);
                child.set_height(100.0);
                child.set_color(Color::hsl(200.0, 0.9, 0.5));
            } else {
                let long_child = NodeMutator::new();
                action(move || {
                    root_node.add_child(long_child);
                });
                long_child.set_width(400.0);
                long_child.set_height(100.0);
                long_child.set_color(Color::hsl(300.0, 0.9, 0.5));
            }
        });

        for _ in 0..5 {
            action(move || {
                let child = NodeMutator::new();
                action(move || {
                    root_node.add_child(child);
                });

                child.set_width(100.0);
                child.set_height(100.0);

                action(move || {
                    if *child.is_hovered.read() {
                        child.set_color(Color::hsl(100.0, 0.9, 0.8));
                    } else {
                        child.set_color(Color::hsl(100.0, 0.9, 0.5));
                    }
                });
            });
        }
    });
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeMutator {
    node_entity: Entity,
    children_sync_key: SyncKey,
    state_sync_key: SyncKey,
    is_hovered: Ref<bool>,
}

impl NodeMutator {
    fn new() -> Self {
        let node_entity = WorldContext::with_world(|world| {
            world
                .spawn(Node {
                    ..Default::default()
                })
                .id()
        });

        rewind(move || {
            WorldContext::with_world(|world| {
                world.entity_mut(node_entity).despawn();
            });
        });

        let mut is_hovered = ref_(false);

        let mut self_ = Self {
            node_entity,
            children_sync_key: sync_key(),
            state_sync_key: sync_key(),
            is_hovered,
        };

        self_.observe(move |_ev: On<Pointer<Enter>>| {
            is_hovered.set(true);
        });

        self_.observe(move |_ev: On<Pointer<Leave>>| {
            is_hovered.set(false);
        });

        self_
    }

    fn set_color(self, color: Color) {
        let prev = WorldContext::with_deferred_world(|mut world| {
            let mut binding = world.entity_mut(self.node_entity);
            let mut background_color = binding.get_mut::<BackgroundColor>().unwrap();
            let prev = background_color.0;
            background_color.0 = color;
            prev
        });
        synced_rewind([self.state_sync_key], move || {
            WorldContext::with_deferred_world(|mut world| {
                let mut binding = world.entity_mut(self.node_entity);
                let mut background_color = binding.get_mut::<BackgroundColor>().unwrap();
                background_color.0 = prev;
            });
        });
    }

    fn set_width(self, width: f32) {
        let prev = WorldContext::with_deferred_world(|mut world| {
            let mut entity_mut = world.entity_mut(self.node_entity);
            let mut node = entity_mut.get_mut::<Node>().unwrap();
            let prev = node.width;
            node.width = Val::Px(width);

            prev
        });

        synced_rewind([self.state_sync_key], move || {
            WorldContext::with_deferred_world(|mut world| {
                let mut entity_mut = world.entity_mut(self.node_entity);
                let mut node = entity_mut.get_mut::<Node>().unwrap();
                node.width = prev;
            });
        });
    }

    fn set_height(self, height: f32) {
        let prev = WorldContext::with_deferred_world(|mut world| {
            let mut entity_mut = world.entity_mut(self.node_entity);
            let mut node = entity_mut.get_mut::<Node>().unwrap();
            let prev = node.height;
            node.height = Val::Px(height);
            prev
        });

        synced_rewind([self.state_sync_key], move || {
            WorldContext::with_deferred_world(|mut world| {
                let mut entity_mut = world.entity_mut(self.node_entity);
                let mut node = entity_mut.get_mut::<Node>().unwrap();
                node.height = prev;
            });
        });
    }

    fn add_child(&mut self, child: NodeMutator) -> NodeMutator {
        WorldContext::with_world(|world| {
            world
                .entity_mut(self.node_entity)
                .add_child(child.node_entity);
        });

        let self_ = *self;
        synced_rewind([self.children_sync_key], move || {
            WorldContext::with_world(|world| {
                world
                    .entity_mut(self_.node_entity)
                    .detach_child(child.node_entity);
            });
        });

        child
    }

    fn observe<E: EntityEvent + 'static>(
        self,
        mut callback: impl FnMut(On<E>) + Send + Sync + 'static,
    ) {
        let observer = WorldContext::with_world(|world| {
            let mut observer = Observer::new(move |ev: On<E>, mut world: DeferredWorld| {
                WorldContext::set_deferred_world(world.reborrow(), || {
                    callback(ev);
                });

                world.commands().queue(FlushWrites);
                world.commands().queue(ExecuteActionTrees);
            });

            observer.watch_entity(self.node_entity);

            world.spawn(observer).id()
        });

        synced_rewind([self.state_sync_key], move || {
            WorldContext::with_world(|world| {
                world.despawn(observer);
            })
        });
    }
}
