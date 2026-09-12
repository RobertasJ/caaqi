use atomic_refcell::{AtomicRef, AtomicRefCell, AtomicRefMut};
use bevy::{
    ecs::{
        component::Component,
        entity::{Entity, EntityNotSpawnedError},
        resource::Resource,
        system::command,
        world::{EntityRef, EntityWorldMut, Mut, World, error::EntityMutableFetchError},
    },
    prelude::{Deref, DerefMut},
};
use ouroboros::self_referencing;
use smallvec::SmallVec;
use std::{
    any::Any,
    collections::{HashMap, HashSet},
    marker::PhantomData,
    ops::{Deref, DerefMut},
    panic::Location,
    sync::Arc,
};

use crate::action::context_builder::action;
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

#[derive(Component, Clone, Copy, Deref, DerefMut)]
pub struct RefInitLocation(&'static Location<'static>);

#[derive(Debug, Default, Clone, Resource, Deref, DerefMut)]
pub struct WrittenTo(HashSet<Entity>);

#[derive(Debug, Default, Clone, Resource, Deref, DerefMut)]
pub struct WriteLocations(HashMap<Entity, Vec<&'static std::panic::Location<'static>>>);

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

#[track_caller]
pub fn ref_<T: Send + Sync + 'static>(world: &mut World, value: T) -> Ref<T> {
    let ref_ = create_ref(world, value);

    // rewind(move || {
    //     drop_ref(ref_);
    // });

    ref_
}

#[track_caller]
pub fn ref_uninit<T: Send + Sync + 'static>(world: &mut World) -> Ref<T> {
    let ref_ = create_ref_uninit::<T>(world);

    // rewind(move || {
    //     drop_ref(ref_);
    // });

    ref_
}

#[track_caller]
pub fn ref_action<T: Send + Sync + 'static>(
    world: &mut World,
    mut computation: impl FnMut(&mut World) -> T + Send + Sync + 'static,
) -> Ref<T> {
    let mut ref_ = ref_uninit::<T>(world);
    let location = Location::caller();

    action(world, move |world| {
        let value = computation(world);
        ref_.set_or_init_with_caller(world, value, location);
    });

    ref_
}

#[track_caller]
pub fn create_ref<T: Any + Send + Sync + 'static>(world: &mut World, value: T) -> Ref<T> {
    let entity = world
        .spawn((RefValue::new(value), RefInitLocation(Location::caller())))
        .id();
    Ref(entity, std::marker::PhantomData)
}

#[track_caller]
pub fn create_ref_uninit<T: Send + Sync + 'static>(world: &mut World) -> Ref<T> {
    let entity = world
        .spawn((RefValue::new_uninit(), RefInitLocation(Location::caller())))
        .id();
    Ref(entity, std::marker::PhantomData)
}

pub fn drop_ref<T: Any + Send + Sync + 'static>(world: &mut World, ref_: Ref<T>) {
    world.despawn(ref_.0);
}

impl<T: Any + Send + Sync + 'static> Ref<T> {
    #[track_caller]
    pub fn set_or_init(&mut self, world: &mut World, value: T) {
        if self
            .value(world)
            .map(|v| v.0.as_deref())
            .flatten()
            .is_some()
        {
            self.set(world, value);
        } else {
            self.entity_mut(world)
                .expect("the Ref has been deallocated")
                .get_mut::<RefValue>()
                .expect("the Ref is not initialized")
                .init(value);
            self.notify(world);
        }
    }

    pub fn set_or_init_with_caller(
        &mut self,
        world: &mut World,
        value: T,
        caller: &'static Location<'static>,
    ) {
        if self
            .value(world)
            .map(|v| v.0.as_deref())
            .flatten()
            .is_some()
        {
            self.set_with_caller(world, value, caller);
        } else {
            self.entity_mut(world)
                .expect("the Ref has been deallocated")
                .get_mut::<RefValue>()
                .expect("the Ref is not initialized")
                .init(value);
            self.notify_with_caller(world, caller);
        }
    }

    #[track_caller]
    pub fn subscribe(&self, world: &mut World) {
        attach_node(world, DetachedNode::<SubscribeScope>::from_entity(self.0));
    }

    #[track_caller]
    pub fn notify(&self, world: &mut World) {
        world
            .get_resource_mut::<WrittenTo>()
            .unwrap()
            .insert(self.0);

        world
            .get_resource_mut::<WriteLocations>()
            .unwrap()
            .entry(self.0)
            .or_default()
            .push(Location::caller());
    }

    pub fn notify_with_caller(&self, world: &mut World, caller: &'static Location<'static>) {
        world
            .get_resource_mut::<WrittenTo>()
            .unwrap()
            .insert(self.0);

        world
            .get_resource_mut::<WriteLocations>()
            .unwrap()
            .entry(self.0)
            .or_default()
            .push(caller);
    }

    #[track_caller]
    pub fn set(&mut self, world: &mut World, value: T) {
        *self.write(world) = value;
    }

    pub fn set_with_caller(
        &mut self,
        world: &mut World,
        value: T,
        caller: &'static Location<'static>,
    ) {
        *self.write_with_caller(world, caller) = value;
    }

    #[track_caller]
    pub fn read(&self, world: &mut World) -> ReadRef<T> {
        self.subscribe(world);

        self.silent_read(world)
    }

    #[track_caller]
    fn silent_read(&self, world: &mut World) -> ReadRef<T> {
        let ref_value = self.data(world).expect("the Ref is not initialized");

        self.read_ref_value(ref_value)
    }

    #[track_caller]
    pub fn write(&mut self, world: &mut World) -> WriteRef<T> {
        self.notify(world);

        self.silent_write(world)
    }

    pub fn write_with_caller(
        &mut self,
        world: &mut World,
        caller: &'static Location<'static>,
    ) -> WriteRef<T> {
        self.notify_with_caller(world, caller);

        self.silent_write(world)
    }

    #[track_caller]
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

    fn value<'a>(&self, world: &'a mut World) -> Option<&'a RefValue> {
        self.entity(world)
            .expect("the Ref has been deallocated")
            .get::<RefValue>()
    }

    fn data<'a>(
        &self,
        world: &'a mut World,
    ) -> Option<&'a Arc<AtomicRefCell<Box<dyn Any + Send + Sync + 'static>>>> {
        self.value(world).and_then(|ref_value| ref_value.0.as_ref())
    }

    fn entity<'a>(&self, world: &'a mut World) -> Result<EntityRef<'a>, EntityNotSpawnedError> {
        world.get_entity(self.0)
    }

    fn entity_mut<'a>(
        &self,
        world: &'a mut World,
    ) -> Result<EntityWorldMut<'a>, EntityMutableFetchError> {
        world.get_entity_mut(self.0)
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
