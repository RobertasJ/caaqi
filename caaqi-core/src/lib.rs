//! caaqi-core: the core of caaqi, with an explicit API taking `&mut Machine`.
//!
//! A [`Machine`](machine::Machine) runs a program, a closure, from the
//! start. Every call the program makes on the machine during a run is an
//! [event] at its own point on the [timeline](time), with an undo:
//! creating, deleting, reading and writing [values](value), and asking for
//! the current point. A program can write into the past
//! ([`write_at`](machine::Machine::write_at)): that wipes everything after
//! the write, and the machine runs the program again, with the write back
//! at its point, until a run settles.
//!
//! ```
//! use caaqi_core::prelude::*;
//!
//! // A label centered in a 200-wide box, using the width its child
//! // measures later in the program.
//! let (mut machine, position) = Machine::new(|machine| {
//!     let child_width = machine.create_value(None).unwrap();
//!     let parent = machine.now().unwrap();
//!     let position = match *machine.read(child_width).unwrap() {
//!         Some(width) => (200.0 - width) / 2.0,
//!         None => 0.0,
//!     };
//!     let measured = 50.0;
//!     if *machine.read(child_width).unwrap() != Some(measured) {
//!         machine.write_at(child_width, parent, Some(measured)).unwrap();
//!     }
//!     position
//! });
//!
//! // The first run stops at the write into the past; the second settles.
//! assert_eq!(machine.run(), Ok(2));
//! assert_eq!(machine.read(position), Ok(&75.0));
//! ```

pub mod event;
pub mod machine;
pub mod time;
pub mod value;

/// The commonly needed items.
pub mod prelude {
    pub use crate::{
        machine::Machine,
        time::Time,
        value::{Value, ValueId},
    };
}
