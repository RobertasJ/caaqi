use bevy::{
    ecs::world::DeferredWorld,
    prelude::{Deref, DerefMut, World},
};
use scoped_tls_hkt::ReborrowMut;

scoped_tls_hkt::scoped_thread_local!(pub static mut WORLD: World);

#[derive(Deref, DerefMut)]
pub struct DeferredWorldContext<'w>(pub DeferredWorld<'w>);

impl<'a, 'b> ReborrowMut<'b> for DeferredWorldContext<'a> {
    type Result = DeferredWorldContext<'b>;

    fn reborrow_mut(&'b mut self) -> Self::Result {
        DeferredWorldContext(self.0.reborrow())
    }
}

scoped_tls_hkt::scoped_thread_local!(pub static mut DEFERRED_WORLD: for<'a> DeferredWorldContext<'a>);
