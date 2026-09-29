//! Rewindable, self-adjusting computation with fine-grained reactivity.
//!
//! Computation is modeled as a tree of nodes, the trace, where each node runs
//! a runner. Nodes can be rerun, and what a run did is undone first by
//! rewinding the node, without a framework hook system.
//!
//! ```
//! use caaqi::prelude::*;
//!
//! #[derive(Default)]
//! struct Count(u32);
//!
//! let mut ctx = Context::new();
//! let node = ctx
//!     .create_node(WithCurrent(|ctx: &mut Context| {
//!         ctx.get_or_insert_with(Count::default).0 += 1;
//!         let node = ctx.current_node().unwrap();
//!         ctx.register_rewind(
//!             node,
//!             WithCurrent(|ctx: &mut Context| ctx.get_mut::<Count>().unwrap().0 -= 1),
//!         )
//!         .unwrap();
//!     }))
//!     .id();
//! ctx.node_mut(node)?.run()?;
//! assert_eq!(ctx.get::<Count>().unwrap().0, 1);
//!
//! // Rewind before rerunning, so the run's effect doesn't pile up.
//! ctx.rewind(node)?;
//! ctx.node_mut(node)?.run()?;
//! assert_eq!(ctx.get::<Count>().unwrap().0, 1);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! # Layers
//!
//! The crate is built in layers, each depending only on the ones before it:
//!
//! 1. [`context`]: [`Context`](context::Context), a type-map of resources
//!    that holds all state, caaqi's own included.
//! 2. [`trace`]: the trace's structure and each node's runner, with
//!    [`trace_iter`] for walks over it.
//! 3. [`rewind`]: code registered on a node to undo what its run did.
//! 4. [`group`]: sets of nodes, built on the trace and rewinds.
//! 5. [`current`]: runners and rewinds that read their key from the context.
//!
//! Each layer is a resource in the context plus an extension trait
//! implemented for [`Context`](context::Context).

pub mod context;
pub mod current;
pub mod group;
pub mod id;
pub mod rewind;
pub mod trace;
pub mod trace_iter;

/// The common imports, in one: `use caaqi::prelude::*;`.
pub mod prelude {
    pub use crate::{
        context::Context,
        current::{CurrentExt, Rewind, Runner, WithCurrent},
        rewind::{RewindExt, RewindKey, RewindWithKey},
        trace::{NodeKey, RunnerWithNode, TraceExt},
    };
}
