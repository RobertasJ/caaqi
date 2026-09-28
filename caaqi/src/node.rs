//! The ergonomic layer: the API app code uses, built only on
//! [`caaqi::raw`](crate::raw).
//!
//! An app is a tree of nodes, each running a closure. [`NodeExt::root`]
//! creates and runs a node on its own, and [`NodeExt::child`], called from a
//! running node's code, creates and runs a node under it. Inside a node's
//! code, [`NodeExt::current`] is that node.
//!
//! # Rules
//!
//! - Anything a node's code does that must be undone, such as adding
//!   something to shared state, goes in [`on_rewind`](NodeExt::on_rewind).
//! - Children made with [`child`](NodeExt::child) are cleaned up
//!   automatically when the parent is rewound or rerun: they're rewound, then
//!   deleted. Don't delete them yourself.
//! - To run a node again, [`rerun`](NodeExt::rerun) it: it's rewound first,
//!   so what its last run did doesn't pile up.
//! - Nodes created with [`root`](NodeExt::root) are cleaned up by whoever
//!   owns them, by rewinding and then deleting them with
//!   [`caaqi::raw`](crate::raw):
//!
//! ```
//! use caaqi::{prelude::*, raw::{rewind::RewindExt, trace::TraceExt}};
//!
//! let mut ctx = Context::new();
//! let root = ctx.root(|ctx| {
//!     ctx.child(|_| {});
//! });
//!
//! ctx.rewind(root)?;
//! ctx.node_mut(root)?.delete()?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use crate::{
    context::Context,
    raw::{
        rewind::RewindExt,
        trace::{NodeKey, RunnerInUse, TraceExt, UnknownNode},
    },
};

/// The current-node resource: the stack of nodes whose code is running,
/// innermost last.
///
/// Everything public goes through [`NodeExt`].
#[derive(Default)]
pub struct Current {
    stack: Vec<NodeKey>,
}

/// Why [`NodeExt::rerun`] failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RerunError {
    #[error(transparent)]
    UnknownNode(#[from] UnknownNode),
    #[error(transparent)]
    RunnerInUse(#[from] RunnerInUse),
}

/// Creates a node whose runner makes it the current node while `f` runs.
fn create(ctx: &mut Context, mut f: impl FnMut(&mut Context) + 'static) -> NodeKey {
    /// Pops the node off the current-node stack when dropped, so a panicking
    /// node is popped too.
    struct Pop<'a>(&'a mut Context);

    impl Drop for Pop<'_> {
        fn drop(&mut self) {
            // The code may have removed the resource; there's nothing left to
            // pop then.
            if let Some(current) = self.0.get_mut::<Current>() {
                current.stack.pop();
            }
        }
    }

    ctx.create_node(move |ctx: &mut Context, node: NodeKey| {
        ctx.get_or_insert_with(Current::default).stack.push(node);
        let guard = Pop(ctx);
        f(&mut *guard.0);
    })
    .id()
}

/// Creating, running and rewinding nodes. See the [module docs](self) for the
/// rules.
pub trait NodeExt {
    /// Creates a root node running `f`, runs it, and returns its key.
    ///
    /// Nothing cleans a root up for you: whoever owns it rewinds and then
    /// deletes it with [`caaqi::raw`](crate::raw) when it's no longer needed.
    fn root(&mut self, f: impl FnMut(&mut Context) + 'static) -> NodeKey;

    /// Creates a child of the current node running `f`, runs it, and returns
    /// its key. A rewind is registered on the current node that rewinds the
    /// child and then deletes it, so rerunning or rewinding the parent cleans
    /// the child up. Outside of any running node, it behaves like
    /// [`root`](Self::root).
    ///
    /// The rewind is registered before the child runs, so a child whose code
    /// panics is cleaned up too.
    fn child(&mut self, f: impl FnMut(&mut Context) + 'static) -> NodeKey;

    /// Registers `f` as a rewind of the current node: it runs when the node
    /// is rewound or rerun. Put everything the node's code does that must be
    /// undone here.
    ///
    /// Panics outside of a running node.
    fn on_rewind(&mut self, f: impl FnOnce(&mut Context) + 'static);

    /// Rewinds `node`, then runs it again.
    ///
    /// Fails if `node` isn't in the trace, or if it's running; nothing is
    /// rewound then.
    fn rerun(&mut self, node: NodeKey) -> Result<(), RerunError>;

    /// The node that is currently running: inside a node's code, that node.
    ///
    /// Panics outside of a running node; [`try_current`](Self::try_current)
    /// doesn't.
    fn current(&self) -> NodeKey;

    /// Like [`current`](Self::current), but returns `None` outside of a
    /// running node.
    fn try_current(&self) -> Option<NodeKey>;
}

impl NodeExt for Context {
    fn root(&mut self, f: impl FnMut(&mut Context) + 'static) -> NodeKey {
        let node = create(self, f);
        self.node_mut(node)
            .expect("the root was just created")
            .run()
            .expect("the root was just created, so it isn't running");
        node
    }

    fn child(&mut self, f: impl FnMut(&mut Context) + 'static) -> NodeKey {
        let Some(parent) = self.try_current() else {
            return self.root(f);
        };

        let child = create(self, f);
        self.node_mut(child)
            .expect("the child was just created")
            .set_parent(parent)
            .expect(
                "the child was just created, so it has no parent and no descendants, \
                 and the parent is running, so it's in the trace",
            );

        self.register_rewind(parent, move |ctx, _| {
            ctx.rewind(child)
                .expect("children are only deleted by their parent's rewind");

            ctx.node_mut(child)
                .expect("children are only deleted by their parent's rewind")
                .delete()
                .expect(
                    "the child has no children left after its rewind, and it isn't \
                     running during its parent's rewind",
                );
        })
        .expect("the parent is running, so it's in the trace");

        self.node_mut(child)
            .expect("the child was just created")
            .run()
            .expect("the child was just created, so it isn't running");

        child
    }

    fn on_rewind(&mut self, f: impl FnOnce(&mut Context) + 'static) {
        let node = self.current();
        self.register_rewind(node, move |ctx, _| f(ctx))
            .expect("the current node is running, so it's in the trace");
    }

    fn rerun(&mut self, node: NodeKey) -> Result<(), RerunError> {
        if self.node(node)?.is_running() {
            return Err(RunnerInUse(node).into());
        }

        self.rewind(node)?;
        self.node_mut(node)?.run()?;
        Ok(())
    }

    fn current(&self) -> NodeKey {
        self.try_current()
            .expect("`current` needs a running node; use `try_current` outside of one")
    }

    fn try_current(&self) -> Option<NodeKey> {
        self.get::<Current>()?.stack.last().copied()
    }
}

#[cfg(test)]
#[path = "node_tests.rs"]
mod tests;
