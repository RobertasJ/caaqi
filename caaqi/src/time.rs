//! The machine's timeline, scoped: points that compare in O(1).
//!
//! Only the machine adds points, as steps run and values are written, so
//! this module only reads the timeline. A [`Time`] is `PartialOrd` by
//! timeline order:
//!
//! ```
//! use caaqi::prelude::*;
//!
//! scope(&mut Machine::new(), || {
//!     let (step, _) = run_step_with_id(|| ());
//!     assert!(step.start() < step.end().unwrap());
//! });
//! ```

use std::cmp::Ordering;

use caaqi_core::time::Time as CoreTime;
pub use caaqi_core::time::UnknownTime;

use crate::machine::with_machine;

/// A point on the machine's timeline. `Copy`, and cheap to compare.
///
/// A newtype over caaqi-core's [`Time`](caaqi_core::time::Time), with `From`
/// both ways. Errors carry the core's handle.
///
/// It's `PartialOrd` by timeline order: `a < b` if `a` comes first.
/// Comparing panics if either point isn't on the timeline;
/// [`try_time_cmp`](Self::try_time_cmp) returns the error instead. A `Time`
/// is only valid in the machine that created it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Time(CoreTime);

impl From<CoreTime> for Time {
    fn from(time: CoreTime) -> Time {
        Time(time)
    }
}

impl From<Time> for CoreTime {
    fn from(time: Time) -> CoreTime {
        time.0
    }
}

impl PartialOrd for Time {
    fn partial_cmp(&self, other: &Time) -> Option<Ordering> {
        Some(self.time_cmp(*other))
    }
}

/// The first point, or `None` if the timeline is empty.
pub fn first() -> Option<Time> {
    with_machine(|machine| machine.timeline().first().map(Time))
}

/// The last point, or `None` if the timeline is empty.
pub fn last() -> Option<Time> {
    with_machine(|machine| machine.timeline().last().map(Time))
}

/// How many points the timeline has.
pub fn len() -> usize {
    with_machine(|machine| machine.timeline().len())
}

/// Whether the timeline has no points.
pub fn is_empty() -> bool {
    with_machine(|machine| machine.timeline().is_empty())
}

/// Every point, in timeline order.
pub fn times() -> Vec<Time> {
    with_machine(|machine| machine.timeline().times().map(Time).collect())
}

impl Time {
    /// Whether the point is on the timeline. When it isn't, the other
    /// methods panic.
    pub fn exists(self) -> bool {
        with_machine(|machine| machine.timeline().contains(self.0))
    }

    /// Like [`try_time_cmp`](Self::try_time_cmp), but panics on error.
    #[track_caller]
    pub fn time_cmp(self, other: Time) -> Ordering {
        self.try_time_cmp(other)
            .unwrap_or_else(|error| panic!("{error}"))
    }

    /// Compares this point with `other` by timeline order: `Less` when this
    /// one comes first. O(1).
    ///
    /// Fails with whichever point isn't on the timeline.
    pub fn try_time_cmp(self, other: Time) -> Result<Ordering, UnknownTime> {
        with_machine(|machine| machine.timeline().time_cmp(self.0, other.0))
    }

    /// Like [`try_next`](Self::try_next), but panics on error.
    #[track_caller]
    pub fn next(self) -> Option<Time> {
        self.try_next().unwrap_or_else(|error| panic!("{error}"))
    }

    /// The point right after this one, or `None` if this is the last.
    pub fn try_next(self) -> Result<Option<Time>, UnknownTime> {
        with_machine(|machine| Ok(machine.timeline().next(self.0)?.map(Time)))
    }

    /// Like [`try_prev`](Self::try_prev), but panics on error.
    #[track_caller]
    pub fn prev(self) -> Option<Time> {
        self.try_prev().unwrap_or_else(|error| panic!("{error}"))
    }

    /// The point right before this one, or `None` if this is the first.
    pub fn try_prev(self) -> Result<Option<Time>, UnknownTime> {
        with_machine(|machine| Ok(machine.timeline().prev(self.0)?.map(Time)))
    }
}

#[cfg(test)]
#[path = "time_tests.rs"]
mod tests;
