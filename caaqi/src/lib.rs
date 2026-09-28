//! Rewindable, self-adjusting computation with fine-grained reactivity.
//!
//! Computation is modeled as a tree of nodes, the trace, where each node has
//! a runner: the code that runs it. Runners can be rerun, and what a run did
//! can be undone by rewinding the node, without a framework hook system.
//!
//! The crate is built in layers, each depending only on the ones before it:
//!
//! 1. [`context`]: [`Context`](context::Context), a type-map of resources
//!    that holds all state, caaqi's own included.
//! 2. [`trace`]: the trace's structure and each node's runner, with
//!    [`trace_iter`] for walks over it.
//! 3. [`rewind`]: code registered on a node to undo what its run did.
//!
//! Each layer is a resource in the context plus an extension trait
//! implemented for [`Context`](context::Context). [`prelude`] exports all of
//! them.
//!
//! ```
//! use caaqi::prelude::*;
//!
//! #[derive(Default)]
//! struct Count(u32);
//!
//! let mut ctx = Context::new();
//! let node = ctx
//!     .create_node(|ctx: &mut Context, node: NodeKey| {
//!         ctx.get_or_insert_with(Count::default).0 += 1;
//!         ctx.register_rewind(node, |ctx, _| {
//!             ctx.get_mut::<Count>().unwrap().0 -= 1;
//!         })
//!         .unwrap();
//!     })
//!     .id();
//!
//! ctx.node_mut(node)?.run()?;
//! assert_eq!(ctx.get::<Count>().unwrap().0, 1);
//!
//! // Rewind before rerunning, so the run's effects don't pile up.
//! ctx.rewind(node)?;
//! assert_eq!(ctx.get::<Count>().unwrap().0, 0);
//! ctx.node_mut(node)?.run()?;
//! assert_eq!(ctx.get::<Count>().unwrap().0, 1);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

pub mod context;
pub mod id;
pub mod rewind;
pub mod trace;
pub mod trace_iter;

/// Everything users need, in one import: `use caaqi::prelude::*;`.
pub mod prelude {
    pub use crate::{
        context::Context,
        id::Id,
        rewind::{RewindExt, RewindKey, Rewinds},
        trace::{
            AddChildError, AlreadyParented, DeleteError, HasChildren, NodeKey, NodeMut, NodeRef,
            Runner, RunnerInUse, SelfParent, SetParentError, Trace, TraceExt, UnknownChild,
            UnknownNode, UnknownParent, WouldCycle,
        },
        trace_iter::{TopDownCursor, TopDownWalk, TraceIterExt},
    };
}
