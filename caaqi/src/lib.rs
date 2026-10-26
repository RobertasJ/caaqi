//! caaqi: a scoped wrapper over caaqi-core.
//!
//! [`new_machine`](machine::new_machine) makes a machine for a program, a
//! closure with no arguments: while it runs, the machine is in a scoped
//! thread-local, and the program uses free functions and `Copy` handles
//! instead of passing the machine around: values in [`value`], the timeline
//! and the current point in [`time`]. Outside a run, [`scope`](machine::scope)
//! puts the machine in scope to read results.
//!
//! ```
//! use caaqi::prelude::*;
//!
//! // A label centered in a 200-wide box, using the width its child
//! // measures later in the program.
//! let (mut machine, position) = new_machine(|| {
//!     let child_width = create_value(None);
//!     let parent = now();
//!     let position = match child_width.get() {
//!         Some(width) => (200.0 - width) / 2.0,
//!         None => 0.0,
//!     };
//!     let measured = 50.0;
//!     if child_width.get() != Some(measured) {
//!         child_width.write_at(parent, Some(measured));
//!     }
//!     position
//! });
//!
//! // The first run stops at the write into the past; the second settles.
//! assert_eq!(machine.run(), Ok(2));
//! assert_eq!(scope(&mut machine, || position.get()), 75.0);
//! ```

pub mod machine;
pub mod time;
pub mod value;

/// The commonly needed items.
pub mod prelude {
    pub use crate::{
        machine::{Machine, new_machine, scope},
        time::{Time, now},
        value::{Value, ValueId, create_value},
    };
}
