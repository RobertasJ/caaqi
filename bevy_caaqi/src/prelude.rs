use std::panic::Location;

use bevy::ecs::system::Commands;
use bevy::ecs::world::{DeferredWorld, World};
use caaqi_context::{DEFERRED_WORLD, DeferredWorldContext, WORLD};

pub use crate::CaaqiPlugin;
pub use crate::action::sync::SyncKey;
use crate::value::{AtomicRefCellStorage, Storage, ValueReadError, ValueWriteError};

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

    pub fn with_deferred_world<R>(f: impl for<'a> FnOnce(DeferredWorld<'a>) -> R) -> R {
        if WORLD.is_set() {
            WORLD.with(|world| f(world.into()))
        } else if DEFERRED_WORLD.is_set() {
            DEFERRED_WORLD.with(|deferred_world| f(deferred_world.0))
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

pub fn var<T: Clone + Send + Sync + 'static>(value: T) -> Var<T> {
    Var(WorldContext::with_world(|world| {
        crate::var::var(world, value)
    }))
}

pub fn state<T: Send + Sync + 'static>(value: T) -> State<T> {
    State(WorldContext::with_world(|world| {
        crate::state::state(world, value)
    }))
}

pub fn sync_key() -> crate::action::sync::SyncKey {
    WorldContext::with_world(|world| crate::action::sync::sync_key(world))
}

pub struct State<T: Send + Sync + 'static, S: Storage = AtomicRefCellStorage<T>>(
    crate::state::State<T, S>,
);

impl<T: Send + Sync + 'static, S: Storage> Clone for State<T, S> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T: Send + Sync + 'static, S: Storage> Copy for State<T, S> {}

impl<T: Send + Sync + 'static, S: Storage> std::hash::Hash for State<T, S> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl<T: Send + Sync + 'static, S: Storage> PartialEq for State<T, S> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<T: Send + Sync + 'static, S: Storage> Eq for State<T, S> {}

impl<T: Send + Sync + 'static> State<T> {
    pub fn new_with_caller(caller: &'static Location<'static>, value: T) -> Self {
        WorldContext::with_world(|world| {
            Self(crate::state::State::new_with_caller(world, caller, value))
        })
    }

    #[track_caller]
    pub fn new(value: T) -> Self {
        Self::new_with_caller(Location::caller(), value)
    }
}

impl<T: Send + Sync + 'static, S: Storage<Value = T>> State<T, S> {
    pub fn new_with_storage_with_caller(caller: &'static Location<'static>, value: T) -> Self {
        WorldContext::with_world(|world| {
            Self(crate::state::State::new_with_storage_with_caller(
                world, caller, value,
            ))
        })
    }

    #[track_caller]
    pub fn new_with_storage(value: T) -> Self {
        Self::new_with_storage_with_caller(Location::caller(), value)
    }

    pub fn remove(self) {
        WorldContext::with_world(|world| {
            self.0.remove(world);
        })
    }

    pub fn subscribe_with_caller(&self, caller: &'static Location<'static>) {
        WorldContext::with_deferred_world(|world| self.0.subscribe_with_caller(world, caller));
    }

    #[track_caller]
    pub fn subscribe(&self) {
        self.subscribe_with_caller(Location::caller())
    }

    pub fn notify_with_caller(&self, caller: &'static Location<'static>) {
        WorldContext::with_deferred_world(|world| self.0.notify_with_caller(world, caller))
    }

    #[track_caller]
    pub fn notify(&self) {
        self.notify_with_caller(Location::caller())
    }

    pub fn try_read_silent(&self) -> Result<<S as Storage>::Ref, ValueReadError<S, T>> {
        WorldContext::with_deferred_world(|world| self.0.try_read_silent(world))
    }

    pub fn try_write_silent(&mut self) -> Result<<S as Storage>::RefMut, ValueWriteError<S, T>> {
        WorldContext::with_deferred_world(|world| self.0.try_write_silent(world))
    }

    pub fn read_silent(&self) -> <S as Storage>::Ref {
        WorldContext::with_deferred_world(|world| self.0.read_silent(world))
    }

    pub fn write_silent(&mut self) -> <S as Storage>::RefMut {
        WorldContext::with_deferred_world(|world| self.0.write_silent(world))
    }

    pub fn read_with_caller(&self, caller: &'static Location<'static>) -> <S as Storage>::Ref {
        WorldContext::with_deferred_world(|world| self.0.read_with_caller(world, caller))
    }

    pub fn write_with_caller(
        &mut self,
        caller: &'static Location<'static>,
    ) -> <S as Storage>::RefMut {
        WorldContext::with_deferred_world(|world| self.0.write_with_caller(world, caller))
    }

    #[track_caller]
    pub fn read(&self) -> <S as Storage>::Ref {
        self.read_with_caller(Location::caller())
    }

    #[track_caller]
    pub fn write(&mut self) -> <S as Storage>::RefMut {
        self.write_with_caller(Location::caller())
    }

    pub fn set_with_caller(&mut self, value: T, caller: &'static Location<'static>) {
        *self.write_with_caller(caller) = value;
    }

    #[track_caller]
    pub fn set(&mut self, value: T) {
        *self.write_with_caller(Location::caller()) = value;
    }
}

pub struct Var<T: Clone + Send + Sync + 'static>(crate::var::Var<T>);

impl<T: Clone + Send + Sync + 'static> Clone for Var<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T: Clone + Send + Sync + 'static> Copy for Var<T> {}

impl<T: Clone + Send + Sync + 'static> std::hash::Hash for Var<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl<T: Clone + Send + Sync + 'static> PartialEq for Var<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<T: Clone + Send + Sync + 'static> Eq for Var<T> {}

impl<T: Clone + Send + Sync + 'static> Var<T> {
    pub fn new_with_caller(caller: &'static Location<'static>, value: T) -> Self {
        WorldContext::with_world(|world| {
            Self(crate::var::Var::new_with_caller(world, caller, value))
        })
    }

    #[track_caller]
    pub fn new(value: T) -> Self {
        Self::new_with_caller(Location::caller(), value)
    }

    pub fn remove(self) {
        WorldContext::with_world(|world| {
            self.0.remove(world);
        })
    }

    pub fn read(&self) -> <AtomicRefCellStorage<T> as Storage>::Ref {
        WorldContext::with_world(|world| self.0.read(world))
    }

    pub fn write(&mut self) -> <AtomicRefCellStorage<T> as Storage>::RefMut {
        WorldContext::with_world(|world| self.0.write(world))
    }
}
