//! caaqi-core with a scoped context: code doesn't pass the
//! [`Context`](context::Context) around.
//!
//! [`scope`](context::scope) puts a context in a thread-local for the length
//! of a closure, and is the only function that takes one.
//! [`with_ctx`](context::with_ctx) borrows it back, for building more
//! functions like this crate's. Each module
//! mirrors the `caaqi_core` module of the same name: every method its extension
//! traits add to `Context` is a function here, [`trace::TraceNode`] (a newtype
//! over `NodeKey`) has the node's operations as methods, and
//! runners and rewinds aren't given the context either.
//!
//! ```
//! use caaqi_core_scoped::{current, prelude::*, trace};
//!
//! let mut ctx = Context::new();
//! scope(&mut ctx, || {
//!     let root = trace::create_node(WithCurrent(|| {}));
//!     let child = trace::create_node(WithCurrent(|| {
//!         assert!(current::current_node().is_some());
//!     }));
//!     child.set_parent(root).unwrap();
//!     child.run().unwrap();
//!     assert_eq!(root.children(), [child]);
//! });
//! ```

pub mod context;
pub mod current;
pub mod group;
pub mod rewind;
pub mod storage;
pub mod trace;
pub mod trace_iter;

/// The most common imports, in one line: `use caaqi_core_scoped::prelude::*;`.
///
/// The modules' functions aren't in it, since names like `node` and
/// `remove` would clash: call them through their module, as in
/// `trace::create_node`.
pub mod prelude {
    pub use crate::{
        context::{Context, scope},
        current::{Rewind, Runner, WithCurrent},
        rewind::{RewindKey, RewindWithKey},
        trace::{RunnerWithNode, TraceNode},
    };
}
