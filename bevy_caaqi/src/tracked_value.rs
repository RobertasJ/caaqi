use atomic_refcell::{AtomicRef, AtomicRefCell, AtomicRefMut};
use bevy::{
    ecs::{
        component::Component,
        entity::{Entity, EntityNotSpawnedError},
        world::{EntityRef, EntityWorldMut, Mut, error::EntityMutableFetchError},
    },
    prelude::{Deref, DerefMut},
};
use ouroboros::self_referencing;
use smallvec::SmallVec;
use std::{
    any::Any,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    sync::Arc,
};

use crate::action::{context_builder::action, node::NeedsRerun};
use caaqi_context::{DefferedWorldContext, DetachedNode, ScopeKind, WorldContext, attach_node};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SubscribeScope;

impl ScopeKind for SubscribeScope {}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct NotifyScope;

impl ScopeKind for NotifyScope {}

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
#[require(Notify)]
struct RefValue(Option<Arc<AtomicRefCell<Box<dyn Any + Send + Sync + 'static>>>>);

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Component, Deref, DerefMut)]
pub struct Notify(SmallVec<[Entity; 1]>);

impl RefValue {
    fn new<T: Any + Send + Sync + 'static>(value: T) -> Self {
        Self(Some(Arc::new(AtomicRefCell::from(
            Box::new(value) as Box<dyn Any + Send + Sync + 'static>
        ))))
    }

    fn new_uninit() -> Self {
        Self(None)
    }

    fn init<T: Any + Send + Sync + 'static>(&mut self, value: T) {
        self.0 = Some(Arc::new(AtomicRefCell::from(
            Box::new(value) as Box<dyn Any + Send + Sync + 'static>
        )));
    }
}

pub fn ref_<T: Send + Sync + 'static>(value: T) -> Ref<T> {
    let ref_ = create_ref(value);

    // rewind(move || {
    //     drop_ref(ref_);
    // });

    ref_
}

pub fn ref_uninit<T: Send + Sync + 'static>() -> Ref<T> {
    let ref_ = create_ref_uninit::<T>();

    // rewind(move || {
    //     drop_ref(ref_);
    // });

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
        let entity = ctx.spawn((RefValue::new_uninit())).id();
        Ref(entity, std::marker::PhantomData)
    })
}

pub fn drop_ref<T: Any + Send + Sync + 'static>(ref_: Ref<T>) {
    WorldContext::with(|ctx| {
        ctx.despawn(ref_.0);
    });
}

impl<T: Any + Send + Sync + 'static> Ref<T> {
    pub fn set_or_init(&mut self, value: T) {
        WorldContext::with(|ctx| {
            let mut entity_mut = self.entity_mut(ctx).expect("the Ref has been deallocated");
            let mut ref_value = entity_mut
                .get_mut::<RefValue>()
                .expect("Whoops, someone created a Ref incorrectly");
            if let Some(data) = &ref_value.0 {
                *data.borrow_mut() = Box::new(value);
            } else {
                ref_value.init(value);
            }
        });
    }

    pub fn subscribe(&self) {
        attach_node(DetachedNode::<SubscribeScope>::from_entity(self.0));
    }

    pub fn notify(&self) {
        WorldContext::with(|ctx| {
            let notify = ctx
                .get::<Notify>(self.0)
                .expect("the Ref is not initialized");

            for subscriber in notify.clone().iter() {
                ctx.entity_mut(*subscriber).insert(NeedsRerun);
            }
        });
    }

    pub fn set(&mut self, value: T) {
        WorldContext::with(|ctx| {
            let ref_value = self.data(ctx).expect("the Ref is not initialized");

            *ref_value.borrow_mut() = Box::new(value);
        });
    }

    pub fn read(&self) -> ReadRef<T> {
        self.subscribe();

        self.silent_read()
    }

    fn silent_read(&self) -> ReadRef<T> {
        WorldContext::with(|ctx| {
            let ref_value = self.data(ctx).expect("the Ref is not initialized");

            self.read_ref_value(ref_value)
        })
    }

    pub fn write(&mut self) -> WriteRef<T> {
        self.notify();

        self.silent_write()
    }

    fn silent_write(&mut self) -> WriteRef<T> {
        WorldContext::with(|ctx| {
            let ref_value = self.data(ctx).expect("the Ref is not initialized");

            self.write_ref_value(ref_value)
        })
    }

    fn write_ref_value(
        &self,
        ref_value: &Arc<AtomicRefCell<Box<dyn Any + Send + Sync + 'static>>>,
    ) -> WriteRef<T> {
        WriteRef::new(Arc::clone(ref_value), |arc| arc.borrow_mut(), PhantomData)
    }

    fn read_ref_value(
        &self,
        ref_value: &Arc<AtomicRefCell<Box<dyn Any + Send + Sync + 'static>>>,
    ) -> ReadRef<T> {
        ReadRef::new(Arc::clone(ref_value), |arc| arc.borrow(), PhantomData)
    }

    fn value<'a>(&self, ctx: &'a mut WorldContext) -> Option<&'a RefValue> {
        self.entity(ctx)
            .expect("the Ref has been deallocated")
            .get::<RefValue>()
    }

    fn data<'a>(
        &self,
        ctx: &'a mut WorldContext,
    ) -> Option<&'a Arc<AtomicRefCell<Box<dyn Any + Send + Sync + 'static>>>> {
        self.value(ctx).and_then(|ref_value| ref_value.0.as_ref())
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
    with_type: std::marker::PhantomData<T>,
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
    with_type: std::marker::PhantomData<T>,
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
