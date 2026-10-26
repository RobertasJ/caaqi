//! [`Machine`], which runs a program and holds everything its runs did.

use std::{
    cmp::Ordering,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    rc::Rc,
};

use crate::{
    event::{Events, is_stop},
    time::{Time, Timeline},
    value::{Value, ValueId, Values},
};

/// The machine is already running its program, so it can't start another
/// run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the machine is already running its program")]
pub struct AlreadyRunning;

/// Runs a program, and holds its timeline, its events, its bootstrapped
/// writes and its values.
///
/// The program is a closure the machine runs from the start, as many times
/// as it takes to settle (see [`run`](Self::run)). Everything it observes or
/// changes beyond its own locals goes through the machine, and every such
/// call during a run is an [event](crate::event). There's no place for
/// anything else: state the machine doesn't track would break the time
/// travel. Keep state in [values](crate::value).
///
/// Every operation is a method taking the machine. They're defined next to
/// what they work on: values in [`value`](crate::value), the current point
/// and the event mechanics in [`event`](crate::event).
pub struct Machine {
    /// The program, with the write of its result at the end.
    program: Rc<dyn Fn(&mut Machine)>,
    pub(crate) timeline: Timeline,
    pub(crate) values: Values,
    pub(crate) events: Events,
}

/// How a single run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Ran {
    /// It ran to the end without writing into the past.
    Settled,
    /// It stopped at a write into the past, so it has to run again.
    Stopped,
}

impl Machine {
    /// A machine that will run `program`, and the value each run writes the
    /// program's result to, as the run's last event. Nothing runs until
    /// [`run`](Self::run).
    ///
    /// The program is `Fn`, not `FnMut`: state kept in the closure between
    /// runs would be a dependency the machine can't see. It must depend on
    /// nothing but the machine (no clocks, globals or randomness): reruns
    /// match its events to the last run's by order, and nothing checks that
    /// they match.
    ///
    /// The timeline starts with one point, the program's start, which isn't
    /// an event: the result value's identity is there. Until the first run
    /// writes it, the result has no contents, and reading it fails with
    /// [`Unwritten`](crate::value::Unwritten).
    pub fn new<R: 'static>(program: impl Fn(&mut Machine) -> R + 'static) -> (Machine, Value<R>) {
        let mut timeline = Timeline::new();
        let start = timeline.push_back();
        let mut values = Values::default();
        let result = values.add_result::<R>(ValueId(start));
        let program = move |machine: &mut Machine| {
            let output = program(machine);
            machine
                .write(result, output)
                .expect("the result value is in the machine, and the machine is running");
        };
        let machine = Machine {
            program: Rc::new(program),
            timeline,
            values,
            events: Events::default(),
        };
        (machine, result)
    }

    /// Runs the program, and runs it again until a run ends without writing
    /// into the past. Returns how many runs it took.
    ///
    /// Each run starts from the beginning. Before it, the machine undoes
    /// every event of the last run, newest first, and the new run's events
    /// take the last run's points in order, so created values get their old
    /// ids back and the bootstrapped writes find their points again. Calling
    /// `run` on a settled machine runs the program once more, to the same
    /// end.
    ///
    /// A program that writes into the past on every run never settles, and
    /// then `run` never returns. The program decides whether a write is
    /// needed, for example by checking what's already there.
    ///
    /// A write into the past stops the run by unwinding, so stopping needs
    /// `panic = "unwind"` (the default): with `panic = "abort"`, the first
    /// write into the past aborts the process. A program that catches
    /// unwinds itself (with `catch_unwind`) and carries on after a stop
    /// breaks it. Any other panic in the program is passed on, with the
    /// machine left as the run left it; the next `run` starts over.
    ///
    /// Fails with [`AlreadyRunning`] when called from the program itself.
    pub fn run(&mut self) -> Result<usize, AlreadyRunning> {
        if self.is_running() {
            return Err(AlreadyRunning);
        }
        let mut runs = 1;
        while self.run_once() == Ran::Stopped {
            runs += 1;
        }
        Ok(runs)
    }

    /// Runs the program once: up to its end, or until it writes into the
    /// past. The write is applied before the next run.
    pub(crate) fn run_once(&mut self) -> Ran {
        self.begin_run();
        let program = Rc::clone(&self.program);
        let outcome = catch_unwind(AssertUnwindSafe(|| program(self)));
        let stopped = self.end_run();
        match outcome {
            Err(payload) if !is_stop(&*payload) => resume_unwind(payload),
            _ if stopped => Ran::Stopped,
            Ok(()) => Ran::Settled,
            Err(_) => unreachable!("a stop unwinds only after keeping its write into the past"),
        }
    }

    /// The timeline, read-only: only the machine adds and removes points.
    ///
    /// Handing out the timeline isn't an event: a reference into the machine
    /// can't be undone or replayed. During a run, it also shows the last
    /// run's points that this run hasn't reached yet.
    pub fn timeline(&self) -> &Timeline {
        &self.timeline
    }

    /// Compares two points that the machine knows are on the timeline.
    pub(crate) fn order(&self, a: Time, b: Time) -> Ordering {
        self.timeline
            .time_cmp(a, b)
            .expect("the points of events and of value histories stay on the timeline")
    }
}

#[cfg(test)]
#[path = "machine_tests.rs"]
mod tests;
