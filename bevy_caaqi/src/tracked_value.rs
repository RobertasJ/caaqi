use atomic_refcell::{AtomicRef, AtomicRefCell, AtomicRefMut};
use bevy::{
    ecs::{
        component::Component,
        entity::{Entity, EntityNotSpawnedError},
        world::{EntityRef, EntityWorldMut, Mut, World, error::EntityMutableFetchError},
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

use crate::action::{context_builder::action, execute::WrittenTo};
use caaqi_context::{DetachedNode, ScopeKind, attach_node};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SubscribeScope;

impl ScopeKind for SubscribeScope {}

#[derive(Debug, PartialEq, Eq)]
pub struct Ref<T: Send + Sync + 'static>(Entity, std::marker::PhantomData<T>);

impl<T: Send + Sync + 'static> Clone for Ref<T> {
    fn clone(&self) -> Self {
        Self(self.0, std::marker::PhantomData)
    }
}

impl<T: Send + Sync + 'static> Copy for Ref<T> {}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct RefTypeErased(Entity);

#[derive(Component)]
pub struct RefValue(Option<Arc<AtomicRefCell<Box<dyn Any + Send + Sync + 'static>>>>);

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

pub fn ref_<T: Send + Sync + 'static>(world: &mut World, value: T) -> Ref<T> {
    let ref_ = create_ref(world, value);

    // rewind(move || {
    //     drop_ref(ref_);
    // });

    ref_
}

pub fn ref_uninit<T: Send + Sync + 'static>(world: &mut World) -> Ref<T> {
    let ref_ = create_ref_uninit::<T>(world);

    // rewind(move || {
    //     drop_ref(ref_);
    // });

    ref_
}

pub fn ref_action<T: Send + Sync + 'static>(
    world: &mut World,
    mut computation: impl FnMut(&mut World) -> T + Send + Sync + 'static,
) -> Ref<T> {
    let mut ref_ = ref_uninit::<T>(world);

    action(world, move |world| {
        let value = computation(world);
        ref_.set_or_init(world, value);
    });

    ref_
}

pub fn create_ref<T: Any + Send + Sync + 'static>(world: &mut World, value: T) -> Ref<T> {
    let entity = world.spawn(RefValue::new(value)).id();
    Ref(entity, std::marker::PhantomData)
}

pub fn create_ref_uninit<T: Send + Sync + 'static>(world: &mut World) -> Ref<T> {
    let entity = world.spawn(RefValue::new_uninit()).id();
    Ref(entity, std::marker::PhantomData)
}

pub fn drop_ref<T: Any + Send + Sync + 'static>(world: &mut World, ref_: Ref<T>) {
    world.despawn(ref_.0);
}

impl<T: Any + Send + Sync + 'static> Ref<T> {
    pub fn set_or_init(&mut self, world: &mut World, value: T) {
        let mut entity_mut = self
            .entity_mut(world)
            .expect("the Ref has been deallocated");
        let mut ref_value = entity_mut
            .get_mut::<RefValue>()
            .expect("Whoops, someone created a Ref incorrectly");
        if let Some(data) = &ref_value.0 {
            *data.borrow_mut() = Box::new(value);
        } else {
            ref_value.init(value);
        }
    }

    pub fn subscribe(&self, world: &mut World) {
        attach_node(world, DetachedNode::<SubscribeScope>::from_entity(self.0));
    }

    pub fn notify(&self, world: &mut World) {
        world
            .get_resource_mut::<WrittenTo>()
            .unwrap()
            .insert(self.0);
    }

    pub fn set(&mut self, world: &mut World, value: T) {
        let ref_value = self.data(world).expect("the Ref is not initialized");

        *ref_value.borrow_mut() = Box::new(value);
    }

    pub fn read(&self, world: &mut World) -> ReadRef<T> {
        self.subscribe(world);

        self.silent_read(world)
    }

    fn silent_read(&self, world: &mut World) -> ReadRef<T> {
        let ref_value = self.data(world).expect("the Ref is not initialized");

        self.read_ref_value(ref_value)
    }

    pub fn write(&mut self, world: &mut World) -> WriteRef<T> {
        self.notify(world);

        self.silent_write(world)
    }

    fn silent_write(&mut self, world: &mut World) -> WriteRef<T> {
        let ref_value = self.data(world).expect("the Ref is not initialized");

        self.write_ref_value(ref_value)
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

    fn value<'a>(&self, ctx: &'a mut World) -> Option<&'a RefValue> {
        self.entity(ctx)
            .expect("the Ref has been deallocated")
            .get::<RefValue>()
    }

    fn data<'a>(
        &self,
        ctx: &'a mut World,
    ) -> Option<&'a Arc<AtomicRefCell<Box<dyn Any + Send + Sync + 'static>>>> {
        self.value(ctx).and_then(|ref_value| ref_value.0.as_ref())
    }

    fn entity<'a>(&self, ctx: &'a mut World) -> Result<EntityRef<'a>, EntityNotSpawnedError> {
        ctx.get_entity(self.0)
    }

    fn entity_mut<'a>(
        &self,
        ctx: &'a mut World,
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
