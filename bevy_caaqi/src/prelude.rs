use bevy::ecs::system::Commands;
use caaqi_context::WORLD;

pub fn action(mut action: impl FnMut() + Send + Sync + 'static) {
    WORLD.with(|world| {
        crate::action::context_builder::action(world, move |world| WORLD.set(world, || action()));
    })
}

pub fn defer_action_eval(commands: Commands, mut action: impl FnMut() + Send + Sync + 'static) {
    crate::action::context_builder::defer_action_eval(commands, move |world| {
        WORLD.set(world, || action());
    });
}

pub fn rewind(undo: impl FnOnce() + Send + Sync + 'static) {
    WORLD.with(|world| {
        crate::action::context_builder::rewind(world, move |world| WORLD.set(world, undo));
    })
}

pub fn synced_rewind(
    keys: impl IntoIterator<Item = crate::action::sync::SyncKey>,
    undo: impl FnOnce() + Send + Sync + 'static,
) {
    WORLD.with(|world| {
        crate::action::context_builder::synced_rewind(world, keys, move |world| {
            WORLD.set(world, undo);
        });
    })
}

pub fn sync_point(keys: impl IntoIterator<Item = crate::action::sync::SyncKey>) {
    WORLD.with(|world| {
        crate::action::sync::sync_point(world, keys);
    })
}

pub fn ref_<T: Send + Sync + 'static>(value: T) -> crate::tracked_value::Ref<T> {
    WORLD.with(|world| crate::tracked_value::ref_(world, value))
}

pub fn create_ref<T: Send + Sync + 'static>(value: T) -> crate::tracked_value::Ref<T> {
    WORLD.with(|world| crate::tracked_value::create_ref(world, value))
}

pub fn drop_ref<T: Send + Sync + 'static>(ref_: crate::tracked_value::Ref<T>) {
    WORLD.with(|world| crate::tracked_value::drop_ref(world, ref_))
}

pub fn var<T: Clone + Send + Sync + 'static>(value: T) -> crate::var::Var<T> {
    WORLD.with(|world| crate::var::var(world, value))
}

pub fn create_var<T: Clone + Send + Sync + 'static>(value: T) -> crate::var::Var<T> {
    WORLD.with(|world| crate::var::create_var(world, value))
}

pub fn drop_var<T: Clone + Send + Sync + 'static>(var: crate::var::Var<T>) {
    WORLD.with(|world| crate::var::drop_var(world, var))
}

pub fn sync_key() -> crate::action::sync::SyncKey {
    WORLD.with(|world| crate::action::sync::sync_key(world))
}

pub fn create_sync_key() -> crate::action::sync::SyncKey {
    WORLD.with(|world| crate::action::sync::create_sync_key(world))
}

pub fn drop_sync_key(key: crate::action::sync::SyncKey) {
    WORLD.with(|world| crate::action::sync::drop_sync_key(world, key))
}

pub struct Ref<T: Send + Sync + 'static>(pub(crate) crate::tracked_value::Ref<T>);

impl<T: Send + Sync + 'static> Ref<T> {
    pub fn read(&self) -> crate::tracked_value::ReadRef<T> {
        WORLD.with(|world| self.0.read(world))
    }

    pub fn write(&mut self) -> crate::tracked_value::WriteRef<T> {
        WORLD.with(|world| self.0.write(world))
    }

    pub fn silent_write(&mut self) -> crate::tracked_value::WriteRef<T> {
        WORLD.with(|world| self.0.silent_write(world))
    }

    pub fn silent_read(&self) -> crate::tracked_value::ReadRef<T> {
        WORLD.with(|world| self.0.silent_read(world))
    }

    pub fn notify(&self) {
        WORLD.with(|world| self.0.notify(world))
    }

    pub fn subscribe(&self) {
        WORLD.with(|world| self.0.subscribe(world))
    }
}

pub struct Var<T: Clone + Send + Sync + 'static>(pub(crate) crate::var::Var<T>);

impl<T: Clone + Send + Sync + 'static> Var<T> {
    pub fn read(&self) -> crate::tracked_value::ReadRef<T> {
        WORLD.with(|world| self.0.read(world))
    }

    pub fn write(&mut self) -> crate::tracked_value::WriteRef<T> {
        WORLD.with(|world| self.0.write(world))
    }
}
