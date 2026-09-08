use bevy::prelude::*;

#[derive(Deref, DerefMut)]
pub struct WorldContext<'w>(&'w mut World);

scoped_thread_local::scoped_thread_local!(static CTX: for<>WorldContext<'_>);

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
}
