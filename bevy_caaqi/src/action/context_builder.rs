use caaqi_context::{DetachedNode, ScopeKind, WorldContext, attach_node, collect_in_scope};

use crate::{
    action::node::{ActionNode, ActionRewind},
    tracked_value::{Notify, SubscribeScope},
};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ActionScope;

impl ScopeKind for ActionScope {}

pub fn detached_action(action: impl FnMut() + Send + Sync + 'static) -> DetachedNode<ActionScope> {
    DetachedNode::from_entity(WorldContext::with(|ctx| {
        ctx.spawn(ActionNode(Box::new(action))).id()
    }))
    .into()
}

pub fn action(action: impl FnMut() + Send + Sync + 'static) {
    let node = detached_action(action);
    attach_node::<ActionScope>(node);
    run_action_node(node);
}

pub fn run_action_node(node: DetachedNode<ActionScope>) {
    let mut action_node = WorldContext::with(|ctx| {
        if let Some(action_node) = ctx.entity_mut(*node).take::<ActionNode>() {
            action_node
        } else {
            panic!("entity {:?} is not an ActionNode", *node);
        }
    });

    let ((_, depends_on), sub_actions) = collect_in_scope::<SubscribeScope, _>(|| {
        collect_in_scope::<ActionScope, _>(|| {
            (action_node.0)();
        })
    });

    WorldContext::with(|ctx| {
        let mut entity_mut = ctx.entity_mut(*node);
        entity_mut.insert(action_node);

        for sub_action in sub_actions {
            entity_mut.add_child(*sub_action);
        }

        for dep in depends_on {
            ctx.get_mut::<Notify>(*dep).unwrap().push(*node);
        }
    });
}

pub fn action_root(action_root: impl FnMut() + Send + Sync + 'static) -> ActionNode {
    ActionNode(Box::new(action_root))
}

pub fn rewind(undo: impl FnOnce() + Send + Sync + 'static) {
    attach_node::<ActionScope>(DetachedNode::from_entity(WorldContext::with(|ctx| {
        ctx.spawn(ActionRewind(Box::new(undo))).id()
    })));
}
