use atomic_refcell::{AtomicRef, AtomicRefCell, AtomicRefMut};
use bevy::{
    ecs::{
        component::Component,
        entity::{Entity, EntityNotSpawnedError},
        world::{EntityRef, EntityWorldMut, World, error::EntityMutableFetchError},
    },
    platform::sync::atomic,
};
use ouroboros::self_referencing;
use std::{
    any::Any,
    io::Read,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    sync::{Arc, RwLock},
};

use crate::{
    action::context_builder::{action, action_rewind},
    context_tree_builder::ScopeKind,
    world_context::WorldContext,
};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RefScope;

impl ScopeKind for RefScope {
    fn with_scope_world<R>(world_scope: impl FnOnce(&mut World) -> R) -> R {
        WorldContext::with(|ctx| world_scope(ctx))
    }
}

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
struct RefValue(Arc<AtomicRefCell<Box<dyn Any + Send + Sync + 'static>>>);

impl RefValue {
    fn new<T: Any + Send + Sync + 'static>(value: T) -> Self {
        Self(Arc::new(AtomicRefCell::from(
            Box::new(value) as Box<dyn Any + Send + Sync + 'static>
        )))
    }
}

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
        ref_.set_or_init(computation());
    });

    ref_
}

pub fn create_ref<T: Any + Send + Sync + 'static>(value: T) -> Ref<T> {
    WorldContext::with(|ctx| {
        let entity = ctx.spawn(RefValue::new(value)).id();
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
    pub fn set_or_init(&mut self, value: T) {
        WorldContext::with(|ctx| {
            if let Some(ref_value) = self.value(ctx) {
                *ref_value.0.borrow_mut() = Box::new(value);
            } else {
                let mut entity = self.entity_mut(ctx).expect("This Ref has been deallocated");
                entity.insert(RefValue::new(value));
            }
        });
    }

    pub fn set(&mut self, value: T) {
        WorldContext::with(|ctx| {
            let ref_value = self.value(ctx).expect("the Ref is not initialized");

            *ref_value.0.borrow_mut() = Box::new(value);
        });
    }

    pub fn read(&self) -> ReadRef<T> {
        WorldContext::with(|ctx| {
            let ref_value = self.value(ctx).expect("the Ref is not initialized");

            Self::read_ref_value(ref_value)
        })
    }

    pub fn write(&mut self) -> WriteRef<T> {
        WorldContext::with(|ctx| {
            let ref_value = self.value(ctx).expect("the Ref is not initialized");

            Self::write_ref_value(ref_value)
        })
    }

    fn write_ref_value(ref_value: &RefValue) -> WriteRef<T> {
        WriteRef::new(
            Arc::clone(&ref_value.0),
            |arc| arc.borrow_mut(),
            PhantomData,
        )
    }

    fn read_ref_value(ref_value: &RefValue) -> ReadRef<T> {
        ReadRef::new(Arc::clone(&ref_value.0), |arc| arc.borrow(), PhantomData)
    }

    fn value<'a>(&self, ctx: &'a mut WorldContext) -> Option<&'a RefValue> {
        self.entity(ctx)
            .expect("the Ref has been deallocated")
            .get::<RefValue>()
    }

    fn entity<'a>(
        &self,
        ctx: &'a mut WorldContext,
    ) -> Result<EntityRef<'a>, EntityNotSpawnedError> {
        ctx.get_entity(self.0)
    }

    fn entity_mut<'a>(
        &self,
        ctx: &'a mut WorldContext,
    ) -> Result<EntityWorldMut<'a>, EntityMutableFetchError> {
        ctx.get_entity_mut(self.0)
    }

    pub fn into_erased(self) -> RefTypeErased {
        RefTypeErased(self.0)
    }
}

#[self_referencing]
pub struct ReadRef<T: Any + Send + Sync + 'static> {
    arc: Arc<AtomicRefCell<Box<dyn Any + Send + Sync + 'static>>>,
    #[borrows(arc)]
    #[covariant]
    borrow: AtomicRef<'this, Box<dyn Any + Send + Sync + 'static>>,
    _type: std::marker::PhantomData<T>,
}

impl<T: Any + Send + Sync + 'static> Deref for ReadRef<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.with_borrow(|borrow| {
            borrow
                .deref()
                .downcast_ref::<T>()
                .expect("the Ref is not initialized")
        })
    }
}

#[self_referencing]
pub struct WriteRef<T: Any + Send + Sync + 'static> {
    arc: Arc<AtomicRefCell<Box<dyn Any + Send + Sync + 'static>>>,
    #[borrows(mut arc)]
    #[covariant]
    borrow: AtomicRefMut<'this, Box<dyn Any + Send + Sync + 'static>>,
    _type: std::marker::PhantomData<T>,
}

impl<T: Any + Send + Sync + 'static> Deref for WriteRef<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.with_borrow(|borrow| {
            borrow
                .deref()
                .downcast_ref::<T>()
                .expect("the Ref is not initialized")
        })
    }
}

impl<T: Any + Send + Sync + 'static> DerefMut for WriteRef<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.with_borrow_mut(|borrow| borrow.deref_mut())
            .deref_mut()
            .downcast_mut::<T>()
            .expect("the Ref is not initialized")
    }
}

impl RefTypeErased {
    pub fn downcast<T: Any + Send + Sync + 'static>(self) -> Ref<T> {
        Ref(self.0, std::marker::PhantomData)
    }
}
