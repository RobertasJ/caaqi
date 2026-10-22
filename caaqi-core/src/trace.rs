//! The trace: the steps of a run, in the order they ran.
//!
//! Running a step with [`Machine::run_step`] appends a point for it at the
//! end of the timeline, runs its code, then closes its section with another
//! point. Steps run inside it land between the two, so nesting exists only
//! as these ranges on the timeline. The trace is append-only: nothing reruns
//! yet.
//!
//! ```
//! use caaqi_core::prelude::*;
//!
//! let mut machine = Machine::new();
//! let (outer, inner) = machine.run_step_with_id(|machine| machine.run_step_with_id(|_| ()).0);
//! let inner = *machine.read(inner).unwrap();
//! assert_eq!(machine.step(inner).unwrap().parent(), Some(outer));
//! assert_eq!(machine.steps().collect::<Vec<_>>(), [outer, inner]);
//! ```

use std::{
    collections::BTreeSet,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
};

use slotmap::SlotMap;

use crate::{
    machine::Machine,
    time::Time,
    value::{Value, ValueId},
};

slotmap::new_key_type! {
    /// A step of a run. Only valid in the [`Machine`] that ran it: in
    /// another, it's unsupported and may refer to an unrelated step.
    pub struct StepId;
}

/// The step isn't in this machine's trace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("{0:?} isn't in the trace")]
pub struct UnknownStep(pub StepId);

/// A read or a write of a value: one edge of the dependency graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Access {
    /// The value read or written.
    pub value: ValueId,
    /// The version read, or the version the write made.
    pub version: u64,
}

/// What the trace recorded about one step.
#[derive(Debug)]
pub struct Step {
    parent: Option<StepId>,
    start: Time,
    end: Option<Time>,
    reads: BTreeSet<Access>,
    writes: BTreeSet<Access>,
}

impl Step {
    /// The step that was running when this one ran, or `None` for a step run
    /// outside any other.
    pub fn parent(&self) -> Option<StepId> {
        self.parent
    }

    /// Where the step's section starts: the point added when it started
    /// running.
    pub fn start(&self) -> Time {
        self.start
    }

    /// Where the step's section ends: the point added when it finished
    /// running, or `None` while it's still running.
    pub fn end(&self) -> Option<Time> {
        self.end
    }

    /// Whether the step is still running.
    pub fn is_running(&self) -> bool {
        self.end.is_none()
    }

    /// The versions of values the step read itself, not counting the steps
    /// inside it. Each once, ordered by value, then version.
    pub fn reads(&self) -> impl Iterator<Item = Access> + '_ {
        self.reads.iter().copied()
    }

    /// The versions of values the step wrote itself, not counting the steps
    /// inside it, ordered by value, then version. Creating a value counts as
    /// writing its first version, and the step's result is version 0 of its
    /// result value.
    pub fn writes(&self) -> impl Iterator<Item = Access> + '_ {
        self.writes.iter().copied()
    }
}

/// The steps of a machine.
#[derive(Default)]
pub(crate) struct Trace {
    steps: SlotMap<StepId, Step>,
    /// Every step, in the order they started, which is timeline order.
    order: Vec<StepId>,
    /// The steps running now, innermost last.
    running: Vec<StepId>,
}

impl Trace {
    /// Records a read on the current step. Outside any step, does nothing.
    pub(crate) fn record_read(&mut self, access: Access) {
        if let Some(step) = self.current_mut() {
            step.reads.insert(access);
        }
    }

    /// Records a write on the current step. Outside any step, does nothing.
    pub(crate) fn record_write(&mut self, access: Access) {
        if let Some(step) = self.current_mut() {
            step.writes.insert(access);
        }
    }

    fn current_mut(&mut self) -> Option<&mut Step> {
        let current = *self.running.last()?;
        Some(&mut self.steps[current])
    }
}

impl Machine {
    /// Runs `f` as a new step, and returns a value holding what `f`
    /// returned. Inside `f`, the step's id is
    /// [`current_step`](Self::current_step);
    /// [`run_step_with_id`](Self::run_step_with_id) also returns it.
    ///
    /// The step starts with a point at the end of the timeline, inside the
    /// section of the innermost running step, which becomes its parent.
    /// While `f` runs, it's the [current step](Self::current_step): the
    /// values `f` reads and writes are recorded on it, and the steps it runs
    /// are inside its section. Then what `f` returned is written as the
    /// result value's version 0, and a last point closes the section.
    ///
    /// The result value's identity belongs to the parent's section: it's
    /// declared, with no contents yet, before the step starts, and only a
    /// rewrite of the parent would end it. Its contents are the step's
    /// output, so the write is recorded on the step, and its point is the
    /// last one inside the step's section. A parent that reads the result
    /// records a read, as with any value.
    ///
    /// If `f` panics, the declared result value is removed (it was never
    /// handed out), and the section is closed before the panic carries on.
    pub fn run_step<R: 'static>(&mut self, f: impl FnOnce(&mut Machine) -> R) -> Value<R> {
        self.run_step_with_id(f).1
    }

    /// Like [`run_step`](Self::run_step), but also returns the step's id.
    pub fn run_step_with_id<R: 'static>(
        &mut self,
        f: impl FnOnce(&mut Machine) -> R,
    ) -> (StepId, Value<R>) {
        // Declared while the parent is still current, though declaring
        // records nothing.
        let result = self.declare_value::<R>();
        let start = self.timeline.push_back();
        let trace = &mut self.trace;
        let step = trace.steps.insert(Step {
            parent: trace.running.last().copied(),
            start,
            end: None,
            reads: BTreeSet::new(),
            writes: BTreeSet::new(),
        });
        trace.order.push(step);
        trace.running.push(step);

        let output = catch_unwind(AssertUnwindSafe(|| f(self)));
        let output = match output {
            // Written while the step is still current, inside its section.
            Ok(output) => {
                self.write(result, output)
                    .expect("the result value was just declared");
                Ok(())
            }
            Err(panic) => {
                self.values.remove_unwritten(result);
                Err(panic)
            }
        };

        let closed = self.trace.running.pop();
        assert_eq!(closed, Some(step), "steps close in the order they started");
        let end = self.timeline.push_back();
        self.trace.steps[step].end = Some(end);

        match output {
            Ok(()) => (step, result),
            Err(panic) => resume_unwind(panic),
        }
    }

    /// The innermost running step, or `None` outside any step.
    pub fn current_step(&self) -> Option<StepId> {
        self.trace.running.last().copied()
    }

    /// What the trace recorded about `step`.
    pub fn step(&self, step: StepId) -> Result<&Step, UnknownStep> {
        self.trace.steps.get(step).ok_or(UnknownStep(step))
    }

    /// Every step, in the order they started, which is timeline order.
    pub fn steps(&self) -> impl Iterator<Item = StepId> + '_ {
        self.trace.order.iter().copied()
    }
}

#[cfg(test)]
#[path = "trace_tests.rs"]
mod tests;
