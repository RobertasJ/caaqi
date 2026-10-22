//! Steps, scoped: [`run_step`] runs code as a step of the machine in scope.
//!
//! ```
//! use caaqi::prelude::*;
//!
//! scope(&mut Machine::new(), || {
//!     let (outer, inner) = run_step_with_id(|| run_step_with_id(|| ()).0);
//!     // The step's result is a value.
//!     let inner = inner.get();
//!     assert_eq!(inner.parent(), Some(outer));
//!     assert_eq!(steps(), [outer, inner]);
//! });
//! ```

pub use caaqi_core::trace::{Access, UnknownStep};
use caaqi_core::trace::{Step as CoreStep, StepId};

use crate::{
    machine::{scope, with_machine},
    time::Time,
    value::Value,
};

/// A step of the machine's run. `Copy`.
///
/// A newtype over caaqi-core's [`StepId`], with `From` both ways. Errors
/// carry the core's id. A `Step` is only valid in the machine that ran it:
/// in another, the methods panic, or refer to an unrelated step.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Step(StepId);

impl From<StepId> for Step {
    fn from(step: StepId) -> Step {
        Step(step)
    }
}

impl From<Step> for StepId {
    fn from(step: Step) -> StepId {
        step.0
    }
}

/// Runs `f` as a new step, and returns a value holding what `f` returned.
/// The step writes the value itself, as the last thing in its section.
/// Inside `f`, the step is [`current_step`]; [`run_step_with_id`] also
/// returns it.
///
/// [`Machine::run_step`](caaqi_core::machine::Machine::run_step): the step
/// starts at the end of the timeline, inside the innermost running step,
/// and is current while `f` runs, so the values `f` reads and writes are
/// recorded on it. `f` runs with the machine in scope.
pub fn run_step<R: 'static>(f: impl FnOnce() -> R) -> Value<R> {
    run_step_with_id(f).1
}

/// Like [`run_step`], but also returns the step.
pub fn run_step_with_id<R: 'static>(f: impl FnOnce() -> R) -> (Step, Value<R>) {
    with_machine(|machine| {
        let (step, result) = machine.run_step_with_id(|machine| scope(machine, f));
        (Step(step), result.into())
    })
}

/// The innermost running step, or `None` outside any step.
pub fn current_step() -> Option<Step> {
    with_machine(|machine| machine.current_step().map(Step))
}

/// Every step, in the order they started, which is timeline order.
pub fn steps() -> Vec<Step> {
    with_machine(|machine| machine.steps().map(Step).collect())
}

impl Step {
    /// Calls `f` with what the trace recorded about the step.
    fn try_recorded<R>(self, f: impl FnOnce(&CoreStep) -> R) -> Result<R, UnknownStep> {
        with_machine(|machine| machine.step(self.0).map(f))
    }

    /// Whether the step is in the machine's trace.
    pub fn exists(self) -> bool {
        self.try_recorded(|_| ()).is_ok()
    }

    /// Like [`try_parent`](Self::try_parent), but panics on error.
    #[track_caller]
    pub fn parent(self) -> Option<Step> {
        self.try_parent().unwrap_or_else(|error| panic!("{error}"))
    }

    /// The step that was running when this one ran, or `None` for a step
    /// run outside any other.
    pub fn try_parent(self) -> Result<Option<Step>, UnknownStep> {
        self.try_recorded(|step| step.parent().map(Step))
    }

    /// Like [`try_start`](Self::try_start), but panics on error.
    #[track_caller]
    pub fn start(self) -> Time {
        self.try_start().unwrap_or_else(|error| panic!("{error}"))
    }

    /// Where the step's section starts: the point added when it started
    /// running.
    pub fn try_start(self) -> Result<Time, UnknownStep> {
        self.try_recorded(|step| step.start().into())
    }

    /// Like [`try_end`](Self::try_end), but panics on error.
    #[track_caller]
    pub fn end(self) -> Option<Time> {
        self.try_end().unwrap_or_else(|error| panic!("{error}"))
    }

    /// Where the step's section ends: the point added when it finished
    /// running, or `None` while it's still running.
    pub fn try_end(self) -> Result<Option<Time>, UnknownStep> {
        self.try_recorded(|step| step.end().map(Time::from))
    }

    /// Like [`try_is_running`](Self::try_is_running), but panics on error.
    #[track_caller]
    pub fn is_running(self) -> bool {
        self.try_is_running()
            .unwrap_or_else(|error| panic!("{error}"))
    }

    /// Whether the step is still running.
    pub fn try_is_running(self) -> Result<bool, UnknownStep> {
        self.try_recorded(CoreStep::is_running)
    }

    /// Like [`try_reads`](Self::try_reads), but panics on error.
    #[track_caller]
    pub fn reads(self) -> Vec<Access> {
        self.try_reads().unwrap_or_else(|error| panic!("{error}"))
    }

    /// The versions of values the step read itself, not counting the steps
    /// inside it. Each once, ordered by value, then version.
    pub fn try_reads(self) -> Result<Vec<Access>, UnknownStep> {
        self.try_recorded(|step| step.reads().collect())
    }

    /// Like [`try_writes`](Self::try_writes), but panics on error.
    #[track_caller]
    pub fn writes(self) -> Vec<Access> {
        self.try_writes().unwrap_or_else(|error| panic!("{error}"))
    }

    /// The versions of values the step wrote itself, not counting the steps
    /// inside it, ordered by value, then version. Creating a value counts as
    /// writing its first version, and the step's result is version 0 of its
    /// result value.
    pub fn try_writes(self) -> Result<Vec<Access>, UnknownStep> {
        self.try_recorded(|step| step.writes().collect())
    }
}

#[cfg(test)]
#[path = "trace_tests.rs"]
mod tests;
