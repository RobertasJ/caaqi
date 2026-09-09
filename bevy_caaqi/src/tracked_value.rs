use bevy::ecs::{component::Component, entity::Entity, world::World};
use std::any::Any;

use crate::{
    action::context_builder::{action, action_rewind},
    world_context::WorldContext,
};

#[derive(Debug, PartialEq, Eq)]
pub struct Ref<T: Send + Sync + 'static>(Entity, std::marker::PhantomData<T>);

impl<T: Send + Sync + 'static> Clone for Ref<T> {
    fn clone(&self) -> Self {
        Self(self.0, std::marker::PhantomData)
    }
}

impl<T: Send + Sync + 'static> Copy for Ref<T> {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefTypeErased(Entity);

#[derive(Component)]
struct RefValue(Box<dyn Any + Send + Sync + 'static>);

pub fn ref_<T: Send + Sync + 'static>(value: T) -> Ref<T> {
    let ref_ = create_ref(value);

    action_rewind(move || {
        drop_ref(ref_);
    });

    ref_
}

pub fn ref_uninit<T: Send + Sync + 'static>() -> Ref<T> {
    let ref_ = create_ref_uninit::<T>();

    action_rewind(move || {
        drop_ref(ref_);
    });

    ref_
}

pub fn ref_action<T: Send + Sync + 'static>(
    mut computation: impl FnMut() -> T + Send + Sync + 'static,
) -> Ref<T> {
    let mut ref_ = ref_uninit::<T>();

    action(move || {
        ref_.set(computation());
    });

    ref_
}

pub fn create_ref<T: Any + Send + Sync + 'static>(value: T) -> Ref<T> {
    WorldContext::with(|ctx| {
        let entity = ctx.spawn(RefValue(Box::new(value))).id();
        Ref(entity, std::marker::PhantomData)
    })
}

pub fn create_ref_uninit<T: Send + Sync + 'static>() -> Ref<T> {
    WorldContext::with(|ctx| {
        let entity = ctx.spawn(()).id();
        Ref(entity, std::marker::PhantomData)
    })
}

pub fn drop_ref<T: Any + Send + Sync + 'static>(ref_: Ref<T>) {
    WorldContext::with(|ctx| {
        ctx.despawn(ref_.0);
    });
}

pub fn untracked_scope(scope: impl FnOnce()) {
    scope();
}

impl<T: Any + Send + Sync + 'static> Ref<T> {
    pub fn set(&mut self, value: T) {
        WorldContext::with(|ctx| {
            if let Some(mut ref_value) = ctx
                .get_entity_mut(self.0)
                .expect("the Ref has been deallocated")
                .get_mut::<RefValue>()
            {
                let Some(ref_value) = ref_value.0.downcast_mut::<T>() else {
                    panic!(
                        "The value this Ref points to is not of the type parameter. Maybe you used RefTypeErased::downcast to get a Ref of the wrong type?"
                    );
                };
                *ref_value = value;
            } else {
                ctx.get_entity_mut(self.0)
                    .expect("the Ref has been deallocated")
                    .insert(RefValue(Box::new(value)));
            }
        });
    }

    pub fn into_erased(self) -> RefTypeErased {
        RefTypeErased(self.0)
    }
}

impl RefTypeErased {
    pub fn downcast<T: Any + Send + Sync + 'static>(self) -> Ref<T> {
        Ref(self.0, std::marker::PhantomData)
    }
}
