//! Rewinds, like [`caaqi::rewind`]: code that undoes what a node's run did.

pub use caaqi::rewind::{RewindKey, UnknownRewind};
use caaqi::{
    context::Context,
    rewind::{NodeRewindExt as _, RewindExt},
    trace::{NodeKey, TraceExt},
};

use crate::{
    context::{scope, with_ctx},
    trace::{TraceNode, UnknownNode},
};

/// Code that undoes what a node's run did, given its own key. Register it
/// with [`register_rewind`]. If it doesn't need its key, write a
/// [`Rewind`](crate::current::Rewind) and wrap it in
/// [`WithCurrent`](crate::current::WithCurrent).
///
/// Any `FnOnce(RewindKey)` closure is a rewind. Annotate its parameter type
/// (`|key: RewindKey| ...`).
pub trait RewindWithKey: Sized + 'static {
    /// caaqi's rewind for this one, which runs it with the context in scope.
    fn into_rewind(self) -> impl caaqi::rewind::RewindWithKey;
}

impl<F: FnOnce(RewindKey) + 'static> RewindWithKey for F {
    fn into_rewind(self) -> impl caaqi::rewind::RewindWithKey {
        move |ctx: &mut Context, key| scope(ctx, || self(key))
    }
}

/// [`RewindExt::register_rewind`]: registers `f` as a rewind of `node`.
/// When it runs, the context it runs in is in scope.
pub fn register_rewind(node: TraceNode, f: impl RewindWithKey) -> Result<RewindKey, UnknownNode> {
    with_ctx(|ctx| ctx.register_rewind(node.into(), f.into_rewind()))
}

/// [`RewindExt::rewind_keys`]: the keys of `node`'s rewinds. They're
/// collected first, so the trace can change while iterating.
pub fn rewind_keys(node: TraceNode) -> Result<impl Iterator<Item = RewindKey>, UnknownNode> {
    with_ctx(|ctx| {
        Ok(ctx
            .rewind_keys(node.into())?
            .collect::<Vec<_>>()
            .into_iter())
    })
}

/// [`RewindExt::run_rewind`]: runs one rewind now.
pub fn run_rewind(key: RewindKey) -> bool {
    with_ctx(|ctx| ctx.run_rewind(key))
}

/// [`RewindExt::rewind_node_of`]: the node the rewind was registered on.
pub fn rewind_node_of(key: RewindKey) -> Option<TraceNode> {
    with_ctx(|ctx| ctx.rewind_node_of(key)).map(TraceNode::from)
}

impl TraceNode {
    /// Runs all of the node's rewinds. See
    /// [`caaqi::rewind::NodeRewindExt::rewind`].
    ///
    /// # Panics
    ///
    /// If the node was deleted.
    pub fn rewind(self) {
        let key = NodeKey::from(self);
        with_ctx(|ctx| {
            ctx.node_mut(key)
                .unwrap_or_else(|_| panic!("node {key:?} was deleted"))
                .rewind()
        });
    }
}

#[cfg(test)]
#[path = "rewind_tests.rs"]
mod tests;
