use bevy::ecs::system::Commands;
use bevy::ecs::world::{DeferredWorld, World};
use caaqi_context::{DEFERRED_WORLD, DeferredWorldContext, WORLD};

pub use crate::CaaqiPlugin;
pub use crate::action::sync::SyncKey;

pub struct WorldContext;

impl WorldContext {
    pub fn set_deferred_world<R>(world: DeferredWorld, f: impl FnOnce() -> R) -> R {
        DEFERRED_WORLD.set(DeferredWorldContext(world), f)
    }

    pub fn set_world<R>(world: &mut World, f: impl FnOnce() -> R) -> R {
        WORLD.set(world, f)
    }

    pub fn with_world<R>(f: impl FnOnce(&mut World) -> R) -> R {
        WORLD.with(f)
    }

    pub fn with_deferred_world<R>(f: impl for<'a> FnOnce(DeferredWorldContext<'a>) -> R) -> R {
        if WORLD.is_set() {
            WORLD.with(|world| f(DeferredWorldContext(world.into())))
        } else if DEFERRED_WORLD.is_set() {
            DEFERRED_WORLD.with(f)
        } else {
            panic!("with_deferred_world called outside of a world context");
        }
    }
}

pub fn action(mut action: impl FnMut() + Send + Sync + 'static) {
    WorldContext::with_world(|world| {
        crate::action::context_builder::action(world, move |world| {
            WorldContext::set_world(world, || action());
        });
    })
}

pub fn ref_action<T: Send + Sync + 'static>(
    mut action: impl FnMut() -> T + Send + Sync + 'static,
) -> Ref<T> {
    Ref(WorldContext::with_world(|world| {
        crate::tracked_value::ref_action(world, move |world| {
            WorldContext::set_world(world, || action())
        })
    }))
}

pub fn defer_action_eval(commands: Commands, mut action: impl FnMut() + Send + Sync + 'static) {
    crate::action::context_builder::defer_action_eval(commands, move |world| {
        WorldContext::set_world(world, || action());
    });
}

pub fn rewind(undo: impl FnOnce() + Send + Sync + 'static) {
    WorldContext::with_world(|world| {
        crate::action::context_builder::rewind(world, move |world| {
            WorldContext::set_world(world, undo);
        });
    })
}

pub fn synced_rewind(
    keys: impl IntoIterator<Item = crate::action::sync::SyncKey>,
    undo: impl FnOnce() + Send + Sync + 'static,
) {
    WorldContext::with_world(|world| {
        crate::action::context_builder::synced_rewind(world, keys, move |world| {
            WorldContext::set_world(world, undo);
        });
    })
}

pub fn sync_point(keys: impl IntoIterator<Item = crate::action::sync::SyncKey>) {
    WorldContext::with_world(|world| {
        crate::action::sync::sync_point(world, keys);
    })
}

pub fn ref_<T: Send + Sync + 'static>(value: T) -> Ref<T> {
    Ref(WorldContext::with_world(|world| {
        crate::tracked_value::ref_(world, value)
    }))
}

pub fn create_ref<T: Send + Sync + 'static>(value: T) -> Ref<T> {
    Ref(WorldContext::with_world(|world| {
        crate::tracked_value::create_ref(world, value)
    }))
}

pub fn drop_ref<T: Send + Sync + 'static>(ref_: Ref<T>) {
    WorldContext::with_world(|world| crate::tracked_value::drop_ref(world, ref_.0))
}

pub fn var<T: Clone + Send + Sync + 'static>(value: T) -> Var<T> {
    Var(WorldContext::with_world(|world| {
        crate::var::var(world, value)
    }))
}

pub fn create_var<T: Clone + Send + Sync + 'static>(value: T) -> Var<T> {
    Var(WorldContext::with_world(|world| {
        crate::var::create_var(world, value)
    }))
}

pub fn drop_var<T: Clone + Send + Sync + 'static>(var: Var<T>) {
    WorldContext::with_world(|world| crate::var::drop_var(world, var.0))
}

pub fn sync_key() -> crate::action::sync::SyncKey {
    WorldContext::with_world(|world| crate::action::sync::sync_key(world))
}

pub fn create_sync_key() -> crate::action::sync::SyncKey {
    WorldContext::with_world(|world| crate::action::sync::create_sync_key(world))
}

pub fn drop_sync_key(key: crate::action::sync::SyncKey) {
    WorldContext::with_world(|world| crate::action::sync::drop_sync_key(world, key))
}

#[derive(Debug, PartialEq, Eq)]
pub struct Ref<T: Send + Sync + 'static>(pub(crate) crate::tracked_value::Ref<T>);

impl<T: Send + Sync + 'static> Clone for Ref<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Send + Sync + 'static> Copy for Ref<T> {}

impl<T: Send + Sync + 'static> Ref<T> {
    pub fn read(&self) -> crate::tracked_value::ReadRef<T> {
        WorldContext::with_deferred_world(|mut world| self.0.read(world.reborrow()))
    }

    pub fn set(&mut self, value: T) {
        WorldContext::with_deferred_world(|mut world| self.0.set(world.reborrow(), value))
    }

    pub fn write(&mut self) -> crate::tracked_value::WriteRef<T> {
        WorldContext::with_deferred_world(|mut world| self.0.write(world.reborrow()))
    }

    pub fn silent_write(&mut self) -> crate::tracked_value::WriteRef<T> {
        WorldContext::with_deferred_world(|mut world| self.0.silent_write(world.reborrow()))
    }

    pub fn silent_read(&self) -> crate::tracked_value::ReadRef<T> {
        WorldContext::with_deferred_world(|mut world| self.0.silent_read(world.reborrow()))
    }

    pub fn notify(&self) {
        WorldContext::with_deferred_world(|mut world| self.0.notify(world.reborrow()))
    }

    pub fn notify_forward_only(&self) {
        WorldContext::with_deferred_world(|mut world| {
            self.0.notify_forward_only(world.reborrow())
        })
    }

    pub fn subscribe(&self) {
        WorldContext::with_deferred_world(|mut world| self.0.subscribe(world.reborrow()))
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Var<T: Clone + Send + Sync + 'static>(pub(crate) crate::var::Var<T>);

impl<T: Clone + Send + Sync + 'static> Clone for Var<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Clone + Send + Sync + 'static> Copy for Var<T> {}

impl<T: Clone + Send + Sync + 'static> Var<T> {
    pub fn read(&self) -> crate::tracked_value::ReadRef<T> {
        WorldContext::with_world(|world| self.0.read(world))
    }

    pub fn write(&mut self) -> crate::tracked_value::WriteRef<T> {
        WorldContext::with_world(|world| self.0.write(world))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::{App, Resource};

    #[test]
    fn handles_are_copy_without_copy_values() {
        struct NonClone;

        fn assert_copy<T: Copy>() {}

        assert_copy::<Ref<NonClone>>();
        assert_copy::<Ref<Vec<i32>>>();
        assert_copy::<Var<Vec<i32>>>();
    }

    #[test]
    fn context_setters_return_values_and_support_deferred_access() {
        let mut world = World::new();
        let mut value = WorldContext::set_world(&mut world, || create_ref(1));

        let observed = WorldContext::set_deferred_world((&mut world).into(), || {
            *value.silent_write() = 2;
            *value.silent_read()
        });

        assert_eq!(observed, 2);
        assert_eq!(
            WorldContext::set_world(&mut world, || *value.silent_read()),
            2,
        );
        WorldContext::set_world(&mut world, || drop_ref(value));
    }

    #[test]
    fn actions_computations_and_rewinds_reenter_context() {
        #[derive(Resource, Default)]
        struct Observed {
            values: Vec<i32>,
            rewinds: usize,
        }

        let mut app = App::new();
        app.add_plugins(CaaqiPlugin);
        app.init_resource::<Observed>();
        let world = app.world_mut();

        defer_action_eval(world.commands(), move || {
            let mut input = ref_(0);
            let doubled = ref_action(move || {
                rewind(move || {
                    WorldContext::with_world(|world| {
                        world.resource_mut::<Observed>().rewinds += 1;
                    });
                });
                *input.read() * 2
            });

            action(move || {
                let value = *doubled.read();
                WorldContext::with_world(|world| {
                    world.resource_mut::<Observed>().values.push(value);
                });
            });

            action(move || {
                input.set(1);
            });
        });

        world.flush();
        let observed = world.resource::<Observed>();
        assert_eq!(observed.values, vec![0, 2]);
        assert_eq!(observed.rewinds, 1);
    }
}
