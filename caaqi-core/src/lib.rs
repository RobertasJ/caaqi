//! caaqi-core: the core of caaqi, with an explicit API taking `&mut Machine`.
//!
//! A [`Machine`](machine::Machine) records a run. Code runs in steps
//! ([`trace`]), and the state it reads and writes lives in values
//! ([`value`]), each keeping its history. Everything that happens takes a
//! point on the machine's timeline ([`time`]), so the past can be inspected:
//! what any value was at any point, and which step read or wrote it.
//!
//! ```
//! use caaqi_core::prelude::*;
//!
//! let mut machine = Machine::new();
//! let name = machine.create_value(String::from("Ada"));
//! let (greet, greeting) =
//!     machine.run_step_with_id(|machine| format!("hello, {}", machine.read(name).unwrap()));
//! machine.write(name, String::from("Grace")).unwrap();
//!
//! // The step's result is a value too.
//! assert_eq!(machine.read(greeting).unwrap(), "hello, Ada");
//! let read: Vec<_> = machine.step(greet).unwrap().reads().collect();
//! assert_eq!(read, [Access { value: name.id(), version: 0 }]);
//! ```

pub mod machine;
pub mod time;
pub mod trace;
pub mod value;

/// The commonly needed items.
pub mod prelude {
    pub use crate::{
        machine::Machine,
        time::Time,
        trace::{Access, Step, StepId},
        value::{Value, ValueId},
    };
}
