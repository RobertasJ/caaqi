//! caaqi: a scoped wrapper over caaqi-core.
//!
//! [`scope`](machine::scope) puts a [`Machine`](machine::Machine) in a
//! scoped thread-local, and inside, code uses free functions and `Copy`
//! handles instead of passing the machine around: steps in [`trace`],
//! values in [`value`], and the timeline in [`time`].
//!
//! ```
//! use caaqi::prelude::*;
//!
//! let mut machine = Machine::new();
//! scope(&mut machine, || {
//!     let name = create_value(String::from("Ada"));
//!     let (greet, greeting) = run_step(|| format!("hello, {}", name.get()));
//!     name.set(String::from("Grace"));
//!
//!     assert_eq!(greeting, "hello, Ada");
//!     assert_eq!(greet.reads(), [Access { value: name.id(), version: 0 }]);
//! });
//! ```

pub mod machine;
pub mod time;
pub mod trace;
pub mod value;

/// The commonly needed items.
pub mod prelude {
    pub use crate::{
        machine::{Machine, scope},
        time::Time,
        trace::{Access, Step, current_step, run_step, steps},
        value::{Value, ValueId, create_value},
    };
}
