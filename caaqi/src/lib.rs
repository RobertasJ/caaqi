//! Rewindable, self-adjusting computation with fine-grained reactivity.
//!
//! Computation is modeled as a tree of nodes, the trace, where each node runs
//! a closure. Nodes can be rerun, and what a run did is undone first by
//! rewinding the node, without a framework hook system.
//!
//! App code uses the ergonomic layer, [`NodeExt`]: [`root`](NodeExt::root)
//! and [`child`](NodeExt::child) create and run nodes,
//! [`on_rewind`](NodeExt::on_rewind) registers what undoes a run, and
//! [`rerun`](NodeExt::rerun) rewinds a node and runs it again. Children are
//! cleaned up automatically when their parent is rewound or rerun.
//! [`prelude`] exports everything app code needs.
//!
//! ```
//! use caaqi::prelude::*;
//!
//! #[derive(Default)]
//! struct Count(u32);
//!
//! let mut ctx = Context::new();
//! let node = ctx.root(|ctx| {
//!     ctx.get_or_insert_with(Count::default).0 += 1;
//!     ctx.on_rewind(|ctx| ctx.get_mut::<Count>().unwrap().0 -= 1);
//! });
//! assert_eq!(ctx.get::<Count>().unwrap().0, 1);
//!
//! // The run's effect is rewound first, so it doesn't pile up.
//! ctx.rerun(node)?;
//! assert_eq!(ctx.get::<Count>().unwrap().0, 1);
//! # Ok::<(), RerunError>(())
//! ```
//!
//! # Layers
//!
//! The crate is built in layers, each depending only on the ones before it:
//!
//! 1. [`context`]: [`Context`](context::Context), a type-map of resources
//!    that holds all state, caaqi's own included.
//! 2. [`raw::trace`]: the trace's structure and each node's runner, with
//!    [`raw::trace_iter`] for walks over it.
//! 3. [`raw::rewind`]: code registered on a node to undo what its run did.
//! 4. [`node`]: the ergonomic layer, built only on [`raw`].
//!
//! Each layer is a resource in the context plus an extension trait
//! implemented for [`Context`](context::Context). [`raw`] is for building
//! your own abstractions; app code shouldn't need it.

pub mod context;
pub mod id;
pub mod node;
pub mod raw;

pub use node::{NodeExt, RerunError};

/// Everything app code needs, in one import: `use caaqi::prelude::*;`.
///
/// Low-level items are imported from [`caaqi::raw`](crate::raw).
pub mod prelude {
    pub use crate::{
        context::Context,
        node::{NodeExt, RerunError},
        raw::trace::NodeKey,
    };
}
