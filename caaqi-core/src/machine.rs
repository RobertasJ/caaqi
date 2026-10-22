//! [`Machine`], which holds everything a run records.

use crate::{time::Timeline, trace::Trace, value::Values};

/// Holds a run: its timeline, its trace (steps, their sections and the
/// values they read and wrote) and its values.
///
/// There's no place for anything else: state the machine doesn't track
/// would break the time travel. Keep state in [values](crate::value).
///
/// Every operation is a method taking the machine. They're defined next to
/// what they work on: steps in [`trace`](crate::trace), values in
/// [`value`](crate::value).
#[derive(Default)]
pub struct Machine {
    pub(crate) timeline: Timeline,
    pub(crate) trace: Trace,
    pub(crate) values: Values,
}

impl Machine {
    /// An empty machine: no steps, no values, and an empty timeline.
    pub fn new() -> Machine {
        Machine::default()
    }

    /// The timeline, read-only: only the machine adds points to it, as steps
    /// run and values are written.
    pub fn timeline(&self) -> &Timeline {
        &self.timeline
    }
}
