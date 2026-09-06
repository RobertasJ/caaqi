use bevy::prelude::*;
use smallvec::SmallVec;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component, Deref)]
#[relationship(relationship_target = CaaqiUiChildren)]
pub struct CaaqiUiChildOf(pub Entity);

#[derive(Debug, Clone, PartialEq, Eq, Component, Deref)]
#[relationship_target(relationship = CaaqiUiChildOf, linked_spawn)]
pub struct CaaqiUiChildren(SmallVec<[Entity; 1]>);
