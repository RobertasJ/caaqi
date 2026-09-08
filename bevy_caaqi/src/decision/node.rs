use bevy::ecs::component::Component;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Component)]
pub struct DecisionNode;

#[derive(Component)]
pub struct Decision(pub Box<dyn FnMut() + Send + Sync + 'static>);

#[derive(Component)]
pub struct UndoOperation(pub Box<dyn FnOnce() + Send + Sync + 'static>);
