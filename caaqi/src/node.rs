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
mod tests {
    use std::{
        cell::Cell,
        panic::{AssertUnwindSafe, catch_unwind},
        rc::Rc,
    };

    use googletest::prelude::*;

    use super::*;

    /// What nodes and rewinds did, in the order they did it.
    #[derive(Default)]
    struct Log(Vec<String>);

    fn log(ctx: &mut Context, entry: impl ToString) {
        ctx.get_or_insert_with(Log::default)
            .0
            .push(entry.to_string());
    }

    fn logged(ctx: &Context) -> Vec<String> {
        ctx.get::<Log>().map_or_else(Vec::new, |log| log.0.clone())
    }

    /// Every node key a test created.
    #[derive(Default)]
    struct Created(Vec<NodeKey>);

    fn created(ctx: &mut Context, node: NodeKey) {
        ctx.get_or_insert_with(Created::default).0.push(node);
    }

    /// How many of the nodes a test created are still in the trace.
    fn live(ctx: &Context) -> usize {
        ctx.get::<Created>().map_or(0, |created| {
            created
                .0
                .iter()
                .filter(|&&node| ctx.contains_node(node))
                .count()
        })
    }

    #[gtest]
    fn child_is_placed_under_the_current_node_and_run() {
        let mut ctx = Context::new();
        let child = Rc::new(Cell::new(None));
        let root = ctx.root({
            let child = child.clone();
            move |ctx| {
                log(ctx, "root");
                child.set(Some(ctx.child(|ctx| log(ctx, "child"))));
            }
        });
        let child = child.get().unwrap();

        expect_eq!(logged(&ctx), ["root", "child"]);
        expect_that!(ctx.node(child).unwrap().parent(), some(eq(root)));
        expect_eq!(ctx.node(root).unwrap().children(), &[child]);
    }

    #[gtest]
    fn child_outside_of_a_running_node_is_a_root() {
        let mut ctx = Context::new();
        let node = ctx.child(|ctx| log(ctx, "ran"));

        expect_eq!(logged(&ctx), ["ran"]);
        expect_that!(ctx.node(node).unwrap().parent(), none());
    }

    #[gtest]
    fn rerunning_the_parent_replaces_its_children() {
        let mut ctx = Context::new();
        let root = ctx.root(|ctx| {
            for _ in 0..3 {
                let child = ctx.child(|_| {});
                created(ctx, child);
            }
        });
        created(&mut ctx, root);
        expect_eq!(live(&ctx), 4);

        for _ in 0..10 {
            let old = ctx.node(root).unwrap().children().to_vec();
            ctx.rerun(root).unwrap();

            expect_eq!(live(&ctx), 4);
            let new = ctx.node(root).unwrap().children().to_vec();
            expect_that!(new.len(), eq(3));
            for child in old {
                expect_false!(ctx.contains_node(child));
                expect_false!(new.contains(&child));
            }
        }
    }

    #[gtest]
    fn nested_children_are_cleaned_up_by_rewinding_the_top() {
        let mut ctx = Context::new();
        let root = ctx.root(|ctx| {
            let a = ctx.child(|ctx| {
                let b = ctx.child(|ctx| {
                    let c = ctx.child(|ctx| ctx.on_rewind(|ctx| log(ctx, "c rewound")));
                    created(ctx, c);
                });
                created(ctx, b);
            });
            created(ctx, a);
        });

        expect_eq!(live(&ctx), 3);
        ctx.rewind(root).unwrap();

        expect_eq!(live(&ctx), 0);
        expect_that!(ctx.node(root).unwrap().children(), is_empty());
        expect_eq!(logged(&ctx), ["c rewound"]);
        expect_that!(ctx.rewind_keys(root).unwrap(), is_empty());
    }

    #[gtest]
    fn current_is_the_running_node() {
        /// What `current` was at each point.
        #[derive(Default)]
        struct Seen {
            root: Option<NodeKey>,
            in_child: Option<NodeKey>,
            child: Option<NodeKey>,
            after_child: Option<NodeKey>,
            after_panic: Option<NodeKey>,
        }

        let mut ctx = Context::new();
        let root = ctx.root(|ctx| {
            let root = ctx.current();
            let child = ctx.child(|ctx| {
                let in_child = ctx.current();
                ctx.get_or_insert_with(Seen::default).in_child = Some(in_child);
            });
            let after_child = ctx.current();
            let panicked = catch_unwind(AssertUnwindSafe(|| {
                ctx.child(|_| panic!("the child panics"));
            }));
            assert!(panicked.is_err());
            let after_panic = ctx.current();

            let seen = ctx.get_or_insert_with(Seen::default);
            seen.root = Some(root);
            seen.child = Some(child);
            seen.after_child = Some(after_child);
            seen.after_panic = Some(after_panic);
        });

        let seen = ctx.get::<Seen>().unwrap();
        expect_that!(seen.root, some(eq(root)));
        expect_eq!(seen.in_child, seen.child);
        expect_that!(seen.after_child, some(eq(root)));
        expect_that!(seen.after_panic, some(eq(root)));
        expect_that!(ctx.try_current(), none());
    }

    #[gtest]
    fn a_panicking_child_is_cleaned_up_with_its_parent() {
        let mut ctx = Context::new();
        let root = ctx.root(|ctx| {
            let _ = catch_unwind(AssertUnwindSafe(|| {
                ctx.child(|_| panic!("the child panics"));
            }));
        });
        expect_that!(ctx.node(root).unwrap().children().len(), eq(1));

        ctx.rewind(root).unwrap();
        expect_that!(ctx.node(root).unwrap().children(), is_empty());
    }

    #[gtest]
    fn on_rewind_runs_when_the_node_is_rewound() {
        let mut ctx = Context::new();
        let root = ctx.root(|ctx| {
            log(ctx, "ran");
            ctx.on_rewind(|ctx| log(ctx, "rewound"));
        });
        expect_eq!(logged(&ctx), ["ran"]);

        ctx.rewind(root).unwrap();
        expect_eq!(logged(&ctx), ["ran", "rewound"]);

        ctx.rerun(root).unwrap();
        ctx.rerun(root).unwrap();
        expect_eq!(logged(&ctx), ["ran", "rewound", "ran", "rewound", "ran"]);
    }

    #[gtest]
    fn rerun_on_a_deleted_node_is_unknown() {
        let mut ctx = Context::new();
        let root = ctx.root(|_| {});
        ctx.rewind(root).unwrap();
        ctx.node_mut(root).unwrap().delete().unwrap();

        expect_that!(
            ctx.rerun(root),
            err(eq(RerunError::UnknownNode(UnknownNode(root))))
        );
    }

    #[gtest]
    fn rerun_on_a_running_node_is_refused_without_rewinding_it() {
        let mut ctx = Context::new();
        let root = ctx.root(|ctx| {
            ctx.on_rewind(|ctx| log(ctx, "rewound"));
            let node = ctx.current();
            let result = ctx.rerun(node);
            ctx.insert(result);
        });

        expect_that!(
            ctx.get::<Result<(), RerunError>>(),
            some(eq(&Err(RerunError::RunnerInUse(RunnerInUse(root)))))
        );
        expect_that!(logged(&ctx), is_empty());
    }

    #[gtest]
    fn outside_of_a_running_node_there_is_no_current_node() {
        let mut ctx = Context::new();

        expect_that!(ctx.try_current(), none());
        expect_true!(catch_unwind(AssertUnwindSafe(|| ctx.current())).is_err());
        expect_true!(catch_unwind(AssertUnwindSafe(|| ctx.on_rewind(|_| {}))).is_err());

        ctx.root(|_| {});
        expect_that!(ctx.try_current(), none());
    }
}
