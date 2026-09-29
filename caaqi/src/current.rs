//! Runners and rewinds that don't need to be given their own key.
//!
//! A [`RunnerWithNode`] is given its node's key, and a [`RewindWithKey`] its
//! own key. Often it's more convenient to write code that takes only the
//! context: a [`Runner`] or a [`Rewind`]. Wrap it in [`WithCurrent`], and
//! inside it, [`current_node`](CurrentExt::current_node) and
//! [`current_rewind`](CurrentExt::current_rewind) return the key it would
//! have been given:
//!
//! ```
//! use caaqi::prelude::*;
//!
//! let mut ctx = Context::new();
//! let node = ctx
//!     .create_node(WithCurrent(|ctx: &mut Context| {
//!         let node = ctx.current_node().unwrap();
//!         ctx.register_rewind(
//!             node,
//!             WithCurrent(|ctx: &mut Context| {
//!                 assert!(ctx.current_rewind().is_some());
//!             }),
//!         )
//!         .unwrap();
//!     }))
//!     .id();
//!
//! ctx.node_mut(node)?.run()?;
//! assert_eq!(ctx.current_node(), None);
//! ctx.rewind(node)?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! Only code wrapped in `WithCurrent` is tracked. Inside a runner or rewind
//! that isn't wrapped, `current_node` and `current_rewind` return the
//! innermost wrapped one around it, if any.

use crate::{
    context::Context,
    rewind::{RewindKey, RewindWithKey},
    trace::{NodeKey, RunnerWithNode},
};

/// The current resource: the keys of the running [`WithCurrent`] nodes and
/// rewinds, innermost last. Everything public goes through [`CurrentExt`].
#[derive(Default)]
struct Current {
    nodes: Vec<NodeKey>,
    rewinds: Vec<RewindKey>,
}

/// The code that runs a node, when it doesn't need to be given the node's
/// key. Wrap it in [`WithCurrent`] to pass it to
/// [`create_node`](crate::trace::TraceExt::create_node); inside it,
/// [`current_node`](CurrentExt::current_node) returns the node's key.
///
/// Any `FnMut(&mut Context)` closure is a runner.
pub trait Runner: 'static {
    fn run(&mut self, ctx: &mut Context);
}

impl<F: FnMut(&mut Context) + 'static> Runner for F {
    fn run(&mut self, ctx: &mut Context) {
        self(ctx)
    }
}

/// A rewind that doesn't need to be given its own key. Wrap it in
/// [`WithCurrent`] to pass it to
/// [`register_rewind`](crate::rewind::RewindExt::register_rewind); inside
/// it, [`current_rewind`](CurrentExt::current_rewind) returns its key.
///
/// Any `FnOnce(&mut Context)` closure is a rewind.
pub trait Rewind: 'static {
    fn rewind(self, ctx: &mut Context);
}

impl<F: FnOnce(&mut Context) + 'static> Rewind for F {
    fn rewind(self, ctx: &mut Context) {
        self(ctx)
    }
}

/// Wraps a [`Runner`] or a [`Rewind`] so it can be passed to
/// [`create_node`](crate::trace::TraceExt::create_node) or
/// [`register_rewind`](crate::rewind::RewindExt::register_rewind). While it
/// runs, [`CurrentExt`] returns its key.
pub struct WithCurrent<T>(pub T);

/// Pops the last key off one of the [`Current`] stacks when dropped, so a
/// panicking runner or rewind is popped too.
struct Pop<'a> {
    ctx: &'a mut Context,
    pop: fn(&mut Current),
}

impl Drop for Pop<'_> {
    fn drop(&mut self) {
        // The code may have removed the resource; there's nothing left to
        // pop then.
        if let Some(current) = self.ctx.get_mut::<Current>() {
            (self.pop)(current);
        }
    }
}

impl<R: Runner> RunnerWithNode for WithCurrent<R> {
    fn run(&mut self, ctx: &mut Context, node: NodeKey) {
        ctx.get_or_insert_with(Current::default).nodes.push(node);
        let guard = Pop {
            ctx,
            pop: |current| {
                current.nodes.pop();
            },
        };
        self.0.run(&mut *guard.ctx);
    }
}

impl<R: Rewind> RewindWithKey for WithCurrent<R> {
    fn rewind(self: Box<Self>, ctx: &mut Context, key: RewindKey) {
        ctx.get_or_insert_with(Current::default).rewinds.push(key);
        let guard = Pop {
            ctx,
            pop: |current| {
                current.rewinds.pop();
            },
        };
        self.0.rewind(&mut *guard.ctx);
    }
}

/// Getting the key of the running node or rewind. See the
/// [module docs](self) for an example.
pub trait CurrentExt {
    /// The key of the node whose [`WithCurrent`] runner is running. When
    /// several are running, one inside another, it's the innermost. Returns
    /// `None` when none is running.
    fn current_node(&self) -> Option<NodeKey>;

    /// The key of the [`WithCurrent`] rewind that is running. When several
    /// are running, one inside another, it's the innermost. Returns `None`
    /// when none is running.
    ///
    /// This is independent of [`current_node`](Self::current_node): a
    /// rewind that runs inside a node's run sees both.
    fn current_rewind(&self) -> Option<RewindKey>;
}

impl CurrentExt for Context {
    fn current_node(&self) -> Option<NodeKey> {
        self.get::<Current>()?.nodes.last().copied()
    }

    fn current_rewind(&self) -> Option<RewindKey> {
        self.get::<Current>()?.rewinds.last().copied()
    }
}

#[cfg(test)]
#[path = "current_tests.rs"]
mod tests;
