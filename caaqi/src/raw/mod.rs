//! The primitives the ergonomic layer is built on: the trace, runners and
//! rewinds. Use them to build your own abstractions or UI libraries. App code
//! shouldn't need them.
//!
//! - [`trace`]: the trace's structure and each node's runner.
//! - [`trace_iter`]: walks over the trace.
//! - [`rewind`]: code registered on a node to undo what its run did.
//! - [`group`]: sets of nodes, built on the trace and rewinds.

pub mod group;
pub mod rewind;
pub mod trace;
pub mod trace_iter;
