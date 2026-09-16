pub mod action;
pub mod prelude;
pub mod tracked_value;
pub mod value_storage;
pub mod var;

use std::panic::Location;

use bevy::prelude::*;
use caaqi_context::Attached;

use crate::{
    action::{
        context_builder::ActionEntity,
        sync::{SyncKey, SyncKeyToActions},
    },
    tracked_value::{RefNotify, RefSubscribe, RefTypeErased},
};

pub struct CaaqiPlugin;

impl Plugin for CaaqiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Attached<ActionEntity>>()
            .init_resource::<Attached<RefSubscribe>>()
            .init_resource::<Attached<RefNotify>>()
            .init_resource::<Attached<SyncKey>>()
            .init_resource::<SyncKeyToActions>();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Component)]
pub struct CreatedAt(&'static Location<'static>);

impl CreatedAt {
    pub fn new(location: &'static Location<'static>) -> Self {
        CreatedAt(location)
    }
}

#[cfg(test)]
mod tests;
