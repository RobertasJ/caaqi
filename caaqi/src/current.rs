//! The current node and rewind: code that doesn't take its node's or its
//! rewind's key reads it from the context instead.
//!
//! A [`Runner`] is a [`RunnerWithNode`] that isn't given its node's key, and
//! a [`Rewind`] is a [`RewindWithKey`] that isn't given its own key. Wrapped
//! in [`WithCurrent`], they're pushed onto the [`Current`] stacks while they
//! run, so [`current_node`](CurrentExt::current_node) and
//! [`current_rewind`](CurrentExt::current_rewind) return their keys:
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
//! Nodes and rewinds that aren't wrapped don't touch the stacks.

use crate::{
    context::Context,
    rewind::{RewindKey, RewindWithKey},
    trace::{NodeKey, RunnerWithNode},
};

/// The current resource: the stacks of running [`WithCurrent`] nodes and
/// rewinds, innermost last.
///
/// Everything public goes through [`CurrentExt`].
#[derive(Default)]
pub struct Current {
    nodes: Vec<NodeKey>,
    rewinds: Vec<RewindKey>,
}

/// The code that runs a node, without the node's key: inside it,
/// [`current_node`](CurrentExt::current_node) is the node. Wrap it in
/// [`WithCurrent`] to get a [`RunnerWithNode`]. Closures taking
/// `&mut Context` are runners.
pub trait Runner: 'static {
    fn run(&mut self, ctx: &mut Context);
}

impl<F: FnMut(&mut Context) + 'static> Runner for F {
    fn run(&mut self, ctx: &mut Context) {
        self(ctx)
    }
}

/// A rewind, without its own key: inside it,
/// [`current_rewind`](CurrentExt::current_rewind) is its key. Wrap it in
/// [`WithCurrent`] to get a [`RewindWithKey`]. Closures taking
/// `&mut Context` once are rewinds.
pub trait Rewind: 'static {
    fn rewind(self, ctx: &mut Context);
}

impl<F: FnOnce(&mut Context) + 'static> Rewind for F {
    fn rewind(self, ctx: &mut Context) {
        self(ctx)
    }
}

/// Turns a [`Runner`] into a [`RunnerWithNode`], and a [`Rewind`] into a
/// [`RewindWithKey`], that push their key onto the [`Current`] stacks while
/// they run. The key is popped again even if they panic.
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

/// Reading the current node and rewind. See the [module docs](self).
pub trait CurrentExt {
    /// The innermost running node whose runner is a [`WithCurrent`], or
    /// `None` outside of one.
    fn current_node(&self) -> Option<NodeKey>;

    /// The innermost running rewind that is a [`WithCurrent`], or `None`
    /// outside of one.
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
