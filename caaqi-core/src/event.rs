//! Events: every call a program makes on the machine during a run.
//!
//! Each event takes its own point on the timeline. An event is an action
//! that returns its undo: the action changes the machine's values, and the
//! undo it hands back puts them back as they were. Before every rerun, the
//! machine undoes every event of the last run, newest first.
//!
//! A write into the past ([`write_at`](Machine::write_at)) becomes a
//! bootstrapped write: an event attached to the event at its point, kept
//! across runs. Whenever a run executes that event, the bootstrapped writes
//! attached to it are executed right after it, at the points they had
//! before.
//!
//! ```
//! use caaqi_core::prelude::*;
//!
//! let (mut machine, seen) = Machine::new(|machine| {
//!     let count = machine.create_value(0).unwrap();
//!     let start = machine.now().unwrap();
//!     let seen = *machine.read(count).unwrap();
//!     if seen == 0 {
//!         // Wipes everything after `start`, and the program goes on from
//!         // there: the run stops, and the next one sees 10.
//!         machine.write_at(count, start, 10).unwrap();
//!     }
//!     seen
//! });
//!
//! assert_eq!(machine.run(), Ok(2));
//! assert_eq!(machine.read(seen), Ok(&10));
//! ```

use std::{any::Any, collections::HashMap, mem, panic::resume_unwind};

use crate::{machine::Machine, time::Time, value::Values};

/// An event's undo: puts the values back as they were before the event's
/// action.
pub(crate) type Undo = Box<dyn FnOnce(&mut Values)>;

/// What a bootstrapped write does in each run that executes it: given its
/// point, it writes its contents and returns its undo, which hands the
/// contents back for the next run.
pub(crate) type Apply = Box<dyn Fn(&mut Values, Time) -> Undo>;

/// The machine isn't running its program: creating, writing and deleting
/// values, [`now`](Machine::now) and [`write_at`](Machine::write_at) work
/// only during a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the machine isn't running its program")]
pub struct NotRunning;

/// An event executed in the current (or the last) run.
struct Logged {
    time: Time,
    /// The event a bootstrapped write is attached to, or `None` for an
    /// event the program made.
    attached_to: Option<Time>,
    undo: Undo,
}

/// A write into the past, kept across runs.
struct Bootstrapped {
    time: Time,
    apply: Apply,
}

/// A write into the past that stopped the run, applied before the next run.
struct Stop {
    /// The point the write goes right after.
    target: Time,
    apply: Apply,
}

/// The payload a stop unwinds with. Private, so nothing else can make one.
struct StopUnwind;

/// The events of a machine, and its bootstrapped writes.
#[derive(Default)]
pub(crate) struct Events {
    running: bool,
    /// Every event executed in this run, or in the last one between runs, in
    /// timeline order. Bootstrapped writes are in it too, right after the
    /// event they're attached to.
    log: Vec<Logged>,
    /// The points of the last run's events, in order, for this run's events
    /// to take: the nth event takes the nth point. Bootstrapped writes keep
    /// their own points, so they aren't in it.
    old: Vec<Time>,
    /// How many of `old` this run has taken.
    matched: usize,
    /// The bootstrapped writes, by the point of the event they're attached
    /// to, each list in timeline order. Between runs, every key is an event
    /// in `log`.
    bootstrapped: HashMap<Time, Vec<Bootstrapped>>,
    /// The write into the past that stopped the run.
    stop: Option<Stop>,
}

/// Whether a panic payload is a stop's.
pub(crate) fn is_stop(payload: &(dyn Any + Send)) -> bool {
    payload.is::<StopUnwind>()
}

impl Machine {
    /// Whether the machine is running its program.
    pub fn is_running(&self) -> bool {
        self.events.running
    }

    /// The current point: an event that returns its own point. A write into
    /// the past ([`write_at`](Self::write_at)) needs one.
    ///
    /// Fails with [`NotRunning`] outside a run.
    pub fn now(&mut self) -> Result<Time, NotRunning> {
        self.event(|_, time| Ok::<_, NotRunning>((time, |_: &mut Values| {})))
    }

    /// Executes an event: takes its point, runs `action` with the values and
    /// the point, and logs the undo the action returns. Then it executes the
    /// bootstrapped writes attached to the event.
    ///
    /// The point is the next one the last run's events left, while there
    /// are any, or a new one at the end of the timeline. If `action` fails,
    /// the call isn't an event: the point is given back, and nothing is
    /// logged.
    ///
    /// Fails with [`NotRunning`] outside a run.
    // TODO: make this public for user-defined events, along with `Values`.
    pub(crate) fn event<R, U, E>(
        &mut self,
        action: impl FnOnce(&mut Values, Time) -> Result<(R, U), E>,
    ) -> Result<R, E>
    where
        U: FnOnce(&mut Values) + 'static,
        E: From<NotRunning>,
    {
        if !self.events.running {
            return Err(NotRunning.into());
        }
        let events = &mut self.events;
        let (time, matched) = match events.old.get(events.matched) {
            Some(&time) => (time, true),
            None => (self.timeline.push_back(), false),
        };
        let (output, undo) = match action(&mut self.values, time) {
            Ok(done) => done,
            Err(error) => {
                if !matched {
                    self.timeline
                        .remove(time)
                        .expect("the point was just added");
                }
                return Err(error);
            }
        };

        let events = &mut self.events;
        if matched {
            events.matched += 1;
        }
        events.log.push(Logged {
            time,
            attached_to: None,
            undo: Box::new(undo),
        });
        for write in events.bootstrapped.get(&time).into_iter().flatten() {
            events.log.push(Logged {
                time: write.time,
                attached_to: Some(time),
                undo: (write.apply)(&mut self.values, write.time),
            });
        }
        Ok(output)
    }

    /// Records an observation (a read or an inspection) as an event, which
    /// has nothing to undo. Outside a run, observing isn't an event, so this
    /// does nothing.
    pub(crate) fn observation(&mut self) {
        if self.events.running {
            self.event(|_, _| Ok::<_, NotRunning>(((), |_: &mut Values| {})))
                .expect("the machine is running");
        }
    }

    /// Whether `time` is the point of an event executed in this run.
    pub(crate) fn is_event_of_this_run(&self, time: Time) -> bool {
        self.events.log.iter().any(|logged| logged.time == time)
    }

    /// Stops the run with a write into the past, which goes right after
    /// `target`, the point of an event of this run. The machine applies it
    /// before the next run (see [`begin_run`](Self::begin_run)).
    ///
    /// Unwinds with a private payload, which [`run`](Self::run) catches.
    /// `resume_unwind` doesn't call the panic hook, so nothing is printed.
    pub(crate) fn stop(&mut self, target: Time, apply: Apply) -> ! {
        debug_assert!(self.is_event_of_this_run(target));
        self.events.stop = Some(Stop { target, apply });
        resume_unwind(Box::new(StopUnwind))
    }

    /// Starts a run, after [undoing the last one](Self::undo_last_run).
    pub(crate) fn begin_run(&mut self) {
        assert!(!self.events.running, "runs don't nest");
        self.undo_last_run();
        self.events.running = true;
    }

    /// Undoes every event of the last run, newest first (their points stay
    /// on the timeline), applies the write into the past that stopped it, if
    /// any, and leaves the remaining events' points for the next run's
    /// events to take. With nothing logged, it changes nothing, so it can
    /// be called again before the next run.
    pub(crate) fn undo_last_run(&mut self) {
        if self.events.log.is_empty() {
            debug_assert!(self.events.stop.is_none(), "a stop needs an event");
            return;
        }
        let log = mem::take(&mut self.events.log);
        let mut points = Vec::with_capacity(log.len());
        for logged in log.into_iter().rev() {
            (logged.undo)(&mut self.values);
            points.push((logged.time, logged.attached_to));
        }
        points.reverse();

        if let Some(stop) = self.events.stop.take() {
            self.write_into_the_past(&mut points, stop);
        }

        let events = &mut self.events;
        events.old = points
            .into_iter()
            .filter(|(_, attached_to)| attached_to.is_none())
            .map(|(time, _)| time)
            .collect();
        events.matched = 0;
    }

    /// Ends a run, and returns whether it stopped at a write into the past.
    ///
    /// The last run's events that this run didn't reach are dropped, with
    /// their points and their bootstrapped writes. A deterministic program
    /// always reaches them, since everything before its newest write into
    /// the past plays out as before.
    pub(crate) fn end_run(&mut self) -> bool {
        self.events.running = false;
        let unmatched = self.events.old.split_off(self.events.matched);
        self.events.old.clear();
        self.events.matched = 0;
        for time in unmatched {
            self.drop_bootstrapped(time);
            self.timeline
                .remove(time)
                .expect("an old event's point is on the timeline");
        }
        self.events.stop.is_some()
    }

    /// Rule 2: the write into the past wipes everything after its target.
    /// The events after it go, with their points, and so do the
    /// bootstrapped writes after it: those attached to later events, and
    /// the ones after the target in its own event's list. Then the write
    /// becomes a bootstrapped write at a new point right after the target.
    ///
    /// `points` is the undone log, as (point, attached to); the events after
    /// the target are taken out of it.
    fn write_into_the_past(&mut self, points: &mut Vec<(Time, Option<Time>)>, stop: Stop) {
        let index = points
            .iter()
            .position(|&(time, _)| time == stop.target)
            .expect("a write into the past goes after an event of the run");
        for &(time, attached_to) in &points[index + 1..] {
            // The bootstrapped writes among them go with their events.
            if attached_to.is_none() {
                self.drop_bootstrapped(time);
                self.timeline
                    .remove(time)
                    .expect("an event's point is on the timeline");
            }
        }
        let event = points[index].1.unwrap_or(stop.target);
        points.truncate(index + 1);

        let writes = self.events.bootstrapped.entry(event).or_default();
        let kept = writes
            .iter()
            .position(|write| write.time == stop.target)
            .map_or(0, |position| position + 1);
        for dropped in writes.drain(kept..) {
            self.timeline
                .remove(dropped.time)
                .expect("a bootstrapped write's point is on the timeline");
        }
        let time = self
            .timeline
            .insert_after(stop.target)
            .expect("the target is on the timeline");
        writes.push(Bootstrapped {
            time,
            apply: stop.apply,
        });
    }

    /// Drops the bootstrapped writes attached to `event`, with their points.
    fn drop_bootstrapped(&mut self, event: Time) {
        for write in self.events.bootstrapped.remove(&event).into_iter().flatten() {
            self.timeline
                .remove(write.time)
                .expect("a bootstrapped write's point is on the timeline");
        }
    }

    /// How many bootstrapped writes the machine keeps.
    #[cfg(test)]
    pub(crate) fn bootstrapped_count(&self) -> usize {
        self.events.bootstrapped.values().map(Vec::len).sum()
    }

    /// The points of the bootstrapped writes attached to `event`, in order.
    #[cfg(test)]
    pub(crate) fn bootstrapped_at(&self, event: Time) -> Vec<Time> {
        self.events
            .bootstrapped
            .get(&event)
            .into_iter()
            .flatten()
            .map(|write| write.time)
            .collect()
    }
}

#[cfg(test)]
#[path = "event_tests.rs"]
mod tests;
