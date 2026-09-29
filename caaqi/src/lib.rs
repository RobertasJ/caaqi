//! Rewindable, self-adjusting computation with fine-grained reactivity.
//!
//! In caaqi, your computation is a tree of nodes called the trace. Each node
//! has a runner, the code it runs, and a node can create and run child nodes
//! while it runs. Any node can be run again later. Before it is, you undo
//! what its last run did by rewinding it: running the rewinds its last run
//! registered. There are no hooks: everything is an explicit method on a
//! [`Context`](context::Context).
//!
//! This example counts runs, and registers a rewind that takes the count
//! back down:
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
//! # Modules
//!
//! - [`context`]: [`Context`](context::Context), which holds all state,
//!   caaqi's and yours.
//! - [`trace`]: creating nodes, arranging them into a tree and running them.
//! - [`trace_iter`]: walking over the trace, such as through all of a node's
//!   descendants.
//! - [`rewind`]: registering code on a node that undoes what its run did.
//! - [`current`]: runners and rewinds that don't need to be given their own
//!   key, because they can ask the context for it.
//! - [`group`]: named sets of nodes, for example the nodes that depend on
//!   some piece of state.
//! - [`storage`]: values of any type kept in the context, reached through
//!   typed handles.
//!
//! Each module adds its methods to [`Context`](context::Context) through an
//! extension trait. [`prelude`] imports the common ones.
//!
//! Keys and handles, such as a node's [`NodeKey`](trace::NodeKey), only work
//! with the context that created them. Using one with another context is a
//! mistake that isn't detected: it may reach something unrelated.

pub mod context;
pub mod current;
pub mod group;
pub mod rewind;
pub mod storage;
pub mod trace;
pub mod trace_iter;

/// The most common imports, in one line: `use caaqi::prelude::*;`.
pub mod prelude {
    pub use crate::{
        context::Context,
        current::{CurrentExt, Rewind, Runner, WithCurrent},
        rewind::{RewindExt, RewindKey, RewindWithKey},
        trace::{NodeKey, RunnerWithNode, TraceExt},
    };
}
