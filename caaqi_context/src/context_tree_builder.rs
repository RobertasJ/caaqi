use std::marker::PhantomData;

use bevy::{
    ecs::{
        bundle::Bundle,
        entity::Entity,
        resource::Resource,
        world::{DeferredWorld, World},
    },
    prelude::{Deref, DerefMut},
};
use smallvec::SmallVec;

// use crate::WORLD;

#[derive(Debug, PartialEq, Eq, Resource, Deref, DerefMut)]
pub struct Attached<T: Send + Sync + 'static>(#[deref] SmallVec<[T; 1]>);

impl<T: Send + Sync + 'static> Default for Attached<T> {
    fn default() -> Self {
        Self(SmallVec::new())
    }
}

impl<T: Send + Sync + 'static> Attached<T> {
    pub fn attach<'a>(world: impl Into<DeferredWorld<'a>>, node: T) {
        let world = &mut world.into();

        if let Some(mut attached_nodes) = world.get_resource_mut::<Attached<T>>() {
            attached_nodes.push(node);
        } else {
            panic!(
                "No scope for node exists. Did you init Attached<{}> in the world before calling Attached::<{}>::attach?",
                std::any::type_name::<T>(),
                std::any::type_name::<T>()
            );
        }
    }

    pub fn take<'a>(world: impl Into<DeferredWorld<'a>>) -> SmallVec<[T; 1]> {
        let world = &mut world.into();

        if let Some(mut attached_nodes) = world.get_resource_mut::<Attached<T>>() {
            std::mem::take(&mut *attached_nodes)
        } else {
            panic!(
                "No scope for node exists. Did you init Attached<{}> in the world before calling Attached::<{}>::take?",
                std::any::type_name::<T>(),
                std::any::type_name::<T>()
            );
        }
    }
}

pub struct Scope<T: Send + Sync + 'static> {
    prev_attached: Attached<T>,
}

impl<T: Send + Sync + 'static> Scope<T> {
    pub fn new<'a>(world: impl Into<DeferredWorld<'a>>) -> Self {
        let world = &mut world.into();
        let msg = &format!(
            "Did you init Attached<{}> in the world before calling Scope::<{}>::new()?",
            std::any::type_name::<T>(),
            std::any::type_name::<T>()
        );

        let prev_attached =
            std::mem::take(&mut *world.get_resource_mut::<Attached<T>>().expect(msg));

        Self { prev_attached }
    }

    pub fn collect<'a>(mut self, world: impl Into<DeferredWorld<'a>>) -> SmallVec<[T; 1]> {
        let world = &mut world.into();

        let msg = &format!(
            "Did you init Attached<{}> in the world before calling Scope::<{}>::new()?",
            std::any::type_name::<T>(),
            std::any::type_name::<T>()
        );

        std::mem::swap(
            &mut self.prev_attached,
            &mut *world.get_resource_mut::<Attached<T>>().expect(&msg),
        );

        self.prev_attached.0
    }
}

#[cfg(test)]
mod scope_tests {
    use super::*;
    use bevy::prelude::*;

    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Component)]
    struct Num(pub i32);

    #[test]
    fn test_scope() {
        let mut world = World::new();
        world.init_resource::<Attached<Entity>>();

        let previous_entity = world.spawn(Num(41)).id();
        Attached::attach(&mut world, previous_entity);

        let scope = Scope::<Entity>::new(&mut world);

        let outer_entity = world.spawn(Num(42)).id();
        Attached::attach(&mut world, outer_entity);

        let nested_scope = Scope::<Entity>::new(&mut world);

        let nested_entity = world.spawn(Num(43)).id();
        Attached::attach(&mut world, nested_entity);

        let nested_nodes = nested_scope.collect(&mut world);

        assert_eq!(nested_nodes.len(), 1);
        assert!(nested_nodes.contains(&nested_entity));
        assert_eq!(world.resource::<Attached<Entity>>().len(), 1);
        assert!(world.resource::<Attached<Entity>>().contains(&outer_entity));

        let nodes = scope.collect(&mut world);

        assert_eq!(nodes.len(), 1);
        assert!(nodes.contains(&outer_entity));
        assert_eq!(world.resource::<Attached<Entity>>().len(), 1);
        assert!(
            world
                .resource::<Attached<Entity>>()
                .contains(&previous_entity)
        );
    }
}
