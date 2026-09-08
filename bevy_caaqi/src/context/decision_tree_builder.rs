use bevy::ecs::component::Component;

// pub fn decision_node_with<El: Element>(
//     mut element: El,
//     mut build: impl FnMut(&mut El) + Send + Sync + 'static,
// ) {
//     WorldContext::with(|ctx| {
//         let node = ctx.create_element_node(DecisionNode(Box::new(move || build(&mut element))));
//     });
// }

// pub fn node_cleanup(cleanup: impl FnOnce() + Send + Sync + 'static) {
//     WorldContext::with(|ctx| {
//         let node = ctx.create_element_node(DecisionNode(Box::new(move || cleanup())));
//     });
// }

#[derive(Component)]
pub struct DecisionNode(pub Box<dyn FnOnce() + Send + Sync + 'static>);
