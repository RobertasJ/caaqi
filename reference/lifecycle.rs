use std::{any::TypeId, collections::HashMap};

use crate::{context::Context, trace::TraceKey};

/// Reacts to action nodes entering or leaving the
/// [`Trace`](crate::trace::Trace), so resources holding
/// per-node data can keep it in sync.
///
/// Usually implemented by the resource type itself; registration is keyed by
/// the implementing type, so each subsystem is notified at most once.
pub trait NodeObserver: 'static {
    /// Called after `key` joins the trace.
    fn node_added(_ctx: &mut Context, _key: TraceKey) {}

    /// Called after `keys` leave the trace, bottom up.
    fn nodes_removed(_ctx: &mut Context, _keys: &[TraceKey]) {}
}

#[derive(Clone, Copy)]
struct Registration {
    node_added: fn(&mut Context, TraceKey),
    nodes_removed: fn(&mut Context, &[TraceKey]),
}

/// The lifecycle resource: every registered [`NodeObserver`], keyed by its
/// type. Notification order is unspecified.
#[derive(Default)]
pub struct NodeObservers(HashMap<TypeId, Registration>);

pub trait LifecycleExt {
    /// Registers `O` to be notified of trace changes. Returns `false` if it was
    /// already registered.
    fn observe_nodes<O: NodeObserver>(&mut self) -> bool;
}

impl LifecycleExt for Context {
    fn observe_nodes<O: NodeObserver>(&mut self) -> bool {
        let observers = &mut self.get_or_insert_with(NodeObservers::default).0;
        if observers.contains_key(&TypeId::of::<O>()) {
            return false;
        }
        observers.insert(
            TypeId::of::<O>(),
            Registration {
                node_added: O::node_added,
                nodes_removed: O::nodes_removed,
            },
        );
        true
    }
}

/// Copied out so observers can use the whole context, including registering
/// more observers, while being notified.
fn registrations(ctx: &Context) -> HashMap<TypeId, Registration> {
    ctx.get::<NodeObservers>()
        .map_or_else(HashMap::new, |observers| observers.0.clone())
}

pub(crate) fn notify_node_added(ctx: &mut Context, key: TraceKey) {
    for registration in registrations(ctx).into_values() {
        (registration.node_added)(ctx, key);
    }
}

pub(crate) fn notify_nodes_removed(ctx: &mut Context, keys: &[TraceKey]) {
    if keys.is_empty() {
        return;
    }
    for registration in registrations(ctx).into_values() {
        (registration.nodes_removed)(ctx, keys);
    }
}
