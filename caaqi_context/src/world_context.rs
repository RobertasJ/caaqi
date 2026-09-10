use bevy::{
    ecs::world::DeferredWorld,
    prelude::{Deref, DerefMut, World},
};

#[derive(Deref, DerefMut)]
pub struct WorldContext<'w>(&'w mut World);

scoped_thread_local::scoped_thread_local!(static CTX: WorldContext<'_>);

impl<'w> WorldContext<'w> {
    pub fn new(world: &'w mut World) -> Self {
        Self(world)
    }

    #[track_caller]
    pub fn enter<R>(&mut self, scope: impl FnOnce() -> R) -> R {
        CTX.set(self, || scope())
    }

    #[track_caller]
    pub fn with<R>(scope: impl FnOnce(&mut WorldContext) -> R) -> R {
        if CTX.is_set() {
            CTX.with(scope)
        } else {
            panic!("WorldContext is not set or is already borrowed.");
        }
    }

    pub fn is_set() -> bool {
        CTX.is_set()
    }
}

#[derive(Deref, DerefMut)]
pub struct DefferedWorldContext<'w>(DeferredWorld<'w>);

scoped_thread_local::scoped_thread_local!(static DEFERRED_CTX: DefferedWorldContext<'_>);

impl<'w> DefferedWorldContext<'w> {
    pub fn new(world: DeferredWorld<'w>) -> Self {
        Self(world)
    }

    #[track_caller]
    pub fn enter<R>(&mut self, scope: impl FnOnce() -> R) -> R {
        DEFERRED_CTX.set(self, || scope())
    }

    #[track_caller]
    pub fn with<R>(scope: impl FnOnce(&mut DefferedWorldContext) -> R) -> R {
        if DEFERRED_CTX.is_set() {
            DEFERRED_CTX.with(scope)
        } else {
            panic!("DefferedWorldContext is not set or is already borrowed.");
        }
    }

    pub fn is_set() -> bool {
        DEFERRED_CTX.is_set()
    }
}
