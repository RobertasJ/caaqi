use bevy::ecs::component::Component;

use crate::{context::WorldContext, element::Element};

pub fn decision_node_with<El: Element>(
    mut element: El,
    mut build: impl FnMut(&mut El) + Send + Sync + 'static,
) {
    WorldContext::with(|ctx| {
        let node = ctx.create_element_node(DecisionNode(Box::new(move || build(&mut element))));
    });
}

pub fn decision_node<El: Element + Default>(build: impl FnMut(&mut El) + Send + Sync + 'static) {
    decision_node_with(El::default(), build);
}

#[derive(Component)]
struct DecisionNode(Box<dyn FnMut() + Send + Sync + 'static>);
