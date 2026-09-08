use bevy::ecs::component::Component;

use crate::{context::WorldContext, element::Element};

pub fn node_init<El: Element + Send + Sync + 'static>(
    mut init: impl Fn() -> El + Send + Sync + 'static,
    build: impl Fn(&mut El) + Send + Sync + 'static,
) {
    WorldContext::with(|ctx| {
        let node = ctx.create_node(DecisionNode(Box::new(move || build(&mut init()))));
    });
}

#[derive(Component)]
struct DecisionNode(Box<dyn Fn() + Send + Sync + 'static>);
