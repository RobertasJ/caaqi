//! Runners and rewinds that don't need to be given their own key, like
//! [`caaqi::current`].
//!
//! Wrap a [`Runner`] or [`Rewind`] in [`WithCurrent`], and inside it,
//! [`current_node`] and [`current_rewind`] return the key it would have been
//! given.

use caaqi::{context::Context, current::CurrentExt};

use crate::{
    context::{scope, with_ctx},
    rewind::{RewindKey, RewindWithKey},
    trace::{RunnerWithNode, TraceNode},
};

/// The code that runs a node, when it doesn't need to be given the node's
/// key. Wrap it in [`WithCurrent`] to pass it to
/// [`create_node`](crate::trace::create_node).
///
/// Any `FnMut()` closure is a runner.
pub trait Runner: 'static {
    fn run(&mut self);
}

impl<F: FnMut() + 'static> Runner for F {
    fn run(&mut self) {
        self()
    }
}

/// A rewind that doesn't need to be given its own key. Wrap it in
/// [`WithCurrent`] to pass it to
/// [`register_rewind`](crate::rewind::register_rewind).
///
/// Any `FnOnce()` closure is a rewind.
pub trait Rewind: 'static {
    fn rewind(self);
}

impl<F: FnOnce() + 'static> Rewind for F {
    fn rewind(self) {
        self()
    }
}

/// Wraps a [`Runner`] or a [`Rewind`] so it can be passed to
/// [`create_node`](crate::trace::create_node) or
/// [`register_rewind`](crate::rewind::register_rewind). While it runs,
/// [`current_node`] or [`current_rewind`] returns its key.
pub struct WithCurrent<T>(pub T);

impl<R: Runner> RunnerWithNode for WithCurrent<R> {
    fn into_runner(self) -> impl caaqi::trace::RunnerWithNode {
        let mut inner = self.0;
        caaqi::current::WithCurrent(move |ctx: &mut Context| scope(ctx, || inner.run()))
    }
}

impl<R: Rewind> RewindWithKey for WithCurrent<R> {
    fn into_rewind(self) -> impl caaqi::rewind::RewindWithKey {
        let inner = self.0;
        caaqi::current::WithCurrent(move |ctx: &mut Context| scope(ctx, || inner.rewind()))
    }
}

/// [`CurrentExt::current_node`]: the innermost running node wrapped in
/// [`WithCurrent`], or `None`.
pub fn current_node() -> Option<TraceNode> {
    with_ctx(|ctx| ctx.current_node()).map(TraceNode::from)
}

/// [`CurrentExt::current_rewind`]: the key of the innermost running rewind
/// wrapped in [`WithCurrent`], or `None`.
pub fn current_rewind() -> Option<RewindKey> {
    with_ctx(|ctx| ctx.current_rewind())
}

#[cfg(test)]
#[path = "current_tests.rs"]
mod tests;
