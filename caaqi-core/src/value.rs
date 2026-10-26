//! Values: state inside the timeline.
//!
//! A value is reached through a [`Value<T>`] handle. Its identity (its
//! creation) and its contents (its writes) are separate: the creation's
//! point is the value's id and holds its initial contents, and every later
//! write is an event of its own, at its own point. Each value keeps its
//! history: one [`HistoryEntry`] per write (counting the initial contents),
//! with the write's point and a snapshot of the value. So the value as seen
//! at any point is the newest write at or before it, which is what
//! [`read_at`](Machine::read_at) returns. During a run, every write lands at
//! the current point, the newest one, so the history stays sorted.
//!
//! A write's version is its index in the history: 0 for the contents the
//! value was created with, then one more per write.
//!
//! Creating, writing and deleting work only during a run. Reading and
//! inspecting work at any time, and during a run they're events too.
//!
//! ```
//! use caaqi_core::prelude::*;
//!
//! let (mut machine, result) = Machine::new(|machine| {
//!     let count = machine.create_value(1).unwrap();
//!     let doubled = *machine.read(count).unwrap() * 2;
//!     machine.write(count, doubled).unwrap();
//!     count
//! });
//! machine.run().unwrap();
//!
//! let count = *machine.read(result).unwrap();
//! assert_eq!(machine.read(count), Ok(&2));
//! // At its creation, the value held 1.
//! let created = machine.history(count).unwrap()[0].time();
//! assert_eq!(machine.read_at(count, created), Ok(Some(&1)));
//! ```

use std::{
    any::Any, cmp::Ordering, collections::HashMap, convert::Infallible, fmt, hash,
    marker::PhantomData,
};

use crate::{
    event::{Bootstrapped, NotRunning, Undo},
    machine::Machine,
    time::{Time, UnknownTime},
};

/// A value, whatever its type. Only valid in the [`Machine`] that created
/// it.
///
/// Under the hood, it's the point of the value's creation, so a rerun that
/// creates the value again at the same point gives back the same id, and
/// handles from earlier runs stay valid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ValueId(pub(crate) Time);

/// A handle to a value of type `T`. `Copy` whatever `T` is.
///
/// Only valid in the [`Machine`] that created it: in another, it's
/// unsupported, and either fails with [`UnknownValue`] or refers to an
/// unrelated value of the same type.
pub struct Value<T> {
    id: ValueId,
    _type: PhantomData<fn() -> T>,
}

impl<T> Value<T> {
    fn new(id: ValueId) -> Value<T> {
        Value {
            id,
            _type: PhantomData,
        }
    }

    /// The value's untyped id.
    pub fn id(self) -> ValueId {
        self.id
    }
}

impl<T> Clone for Value<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Value<T> {}

impl<T> PartialEq for Value<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T> Eq for Value<T> {}

impl<T> hash::Hash for Value<T> {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl<T> fmt::Debug for Value<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Value").field(&self.id).finish()
    }
}

/// The value isn't in this machine: it was never created here, its creation
/// was undone, or the handle has another type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("{0:?} isn't in the machine")]
pub struct UnknownValue(pub ValueId);

/// The value was deleted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("{0:?} was deleted")]
pub struct DeletedValue(pub ValueId);

/// The value has no contents yet. Only the program's result value is ever
/// in this state: before the first run writes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("{0:?} hasn't been written yet")]
pub struct Unwritten(pub ValueId);

/// A write into the past needs the point of an earlier event in this run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("{0:?} isn't the point of an earlier event in this run")]
pub struct NotInThePast(pub Time);

/// The value didn't exist yet at the point: it was created after it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("{value:?} didn't exist yet at {time:?}")]
pub struct NotYetCreated {
    pub value: ValueId,
    pub time: Time,
}

/// Why [`Machine::read`] failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ReadError {
    #[error(transparent)]
    UnknownValue(#[from] UnknownValue),
    #[error(transparent)]
    DeletedValue(#[from] DeletedValue),
    #[error(transparent)]
    Unwritten(#[from] Unwritten),
}

/// Why [`Machine::read_at`] failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ReadAtError {
    #[error(transparent)]
    UnknownValue(#[from] UnknownValue),
    #[error(transparent)]
    UnknownTime(#[from] UnknownTime),
    #[error(transparent)]
    DeletedValue(#[from] DeletedValue),
}

/// Why [`Machine::write`] failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum WriteError {
    #[error(transparent)]
    NotRunning(#[from] NotRunning),
    #[error(transparent)]
    UnknownValue(#[from] UnknownValue),
    #[error(transparent)]
    DeletedValue(#[from] DeletedValue),
}

/// Why [`Machine::update`] failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum UpdateError {
    #[error(transparent)]
    NotRunning(#[from] NotRunning),
    #[error(transparent)]
    UnknownValue(#[from] UnknownValue),
    #[error(transparent)]
    DeletedValue(#[from] DeletedValue),
    #[error(transparent)]
    Unwritten(#[from] Unwritten),
}

/// Why [`Machine::delete`] failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DeleteError {
    #[error(transparent)]
    NotRunning(#[from] NotRunning),
    #[error(transparent)]
    UnknownValue(#[from] UnknownValue),
    #[error(transparent)]
    DeletedValue(#[from] DeletedValue),
}

/// Why [`Machine::version`] failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum VersionError {
    #[error(transparent)]
    UnknownValue(#[from] UnknownValue),
    #[error(transparent)]
    DeletedValue(#[from] DeletedValue),
    #[error(transparent)]
    Unwritten(#[from] Unwritten),
}

/// Why [`Machine::write_at`] failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum WriteAtError {
    #[error(transparent)]
    NotRunning(#[from] NotRunning),
    #[error(transparent)]
    UnknownValue(#[from] UnknownValue),
    #[error(transparent)]
    NotInThePast(#[from] NotInThePast),
    #[error(transparent)]
    NotYetCreated(#[from] NotYetCreated),
    #[error(transparent)]
    DeletedValue(#[from] DeletedValue),
}

/// One write in a value's history.
#[derive(Debug)]
pub struct HistoryEntry<T> {
    time: Time,
    snapshot: T,
}

impl<T> HistoryEntry<T> {
    /// The write's point.
    pub fn time(&self) -> Time {
        self.time
    }

    /// The value as the write left it.
    pub fn snapshot(&self) -> &T {
        &self.snapshot
    }
}

/// One value: its history, sorted by time, and when it was deleted.
struct Slot<T> {
    history: Vec<HistoryEntry<T>>,
    deleted: Option<Time>,
}

impl<T> Slot<T> {
    /// The slot, if the value wasn't deleted.
    fn live(&self, id: ValueId) -> Result<&Slot<T>, DeletedValue> {
        match self.deleted {
            Some(_) => Err(DeletedValue(id)),
            None => Ok(self),
        }
    }

    /// Like [`live`](Self::live), for changing the slot.
    fn live_mut(&mut self, id: ValueId) -> Result<&mut Slot<T>, DeletedValue> {
        match self.deleted {
            Some(_) => Err(DeletedValue(id)),
            None => Ok(self),
        }
    }

    /// The index of the newest write, which is the newest version.
    fn newest(&self, id: ValueId) -> Result<usize, Unwritten> {
        self.history.len().checked_sub(1).ok_or(Unwritten(id))
    }
}

/// The machine's state: every value, each a `Slot<T>` for its own `T`.
/// Events' actions and undos reach it through the machine they get.
#[derive(Default)]
pub(crate) struct Values {
    slots: HashMap<ValueId, Box<dyn Any>>,
}

impl Values {
    /// Adds the program's result value, at `id`, with no contents yet. It's
    /// the only value made outside an event.
    pub(crate) fn add_result<T: 'static>(&mut self, id: ValueId) -> Value<T> {
        let value = Value::new(id);
        self.insert(
            value,
            Slot::<T> {
                history: Vec::new(),
                deleted: None,
            },
        );
        value
    }

    fn insert<T: 'static>(&mut self, value: Value<T>, slot: Slot<T>) {
        let previous = self.slots.insert(value.id, Box::new(slot));
        assert!(
            previous.is_none(),
            "a value's id is its creation's point, and each point has one event"
        );
    }

    fn slot<T: 'static>(&self, value: Value<T>) -> Result<&Slot<T>, UnknownValue> {
        self.slots
            .get(&value.id)
            .and_then(|slot| slot.downcast_ref())
            .ok_or(UnknownValue(value.id))
    }

    fn slot_mut<T: 'static>(&mut self, value: Value<T>) -> Result<&mut Slot<T>, UnknownValue> {
        self.slots
            .get_mut(&value.id)
            .and_then(|slot| slot.downcast_mut())
            .ok_or(UnknownValue(value.id))
    }

    /// The snapshot at `index`, which was just found in the value's history.
    fn snapshot<T: 'static>(&self, value: Value<T>, index: usize) -> &T {
        let slot = self.slot(value).expect("the value was just found");
        &slot.history[index].snapshot
    }

    /// Undoes a write at `time`: takes its entry out of the history.
    fn pop<T: 'static>(&mut self, value: Value<T>, time: Time) -> HistoryEntry<T> {
        let slot = self
            .slot_mut(value)
            .expect("undos run newest first, so a write is undone before its value's creation");
        let entry = slot
            .history
            .pop()
            .expect("an undone write's entry is in the history");
        assert_eq!(
            entry.time, time,
            "undos run newest first, so a write's entry is its value's newest"
        );
        entry
    }
}

/// The undo of a write at `time`: it removes the write's history entry.
fn undo_write<T: 'static>(value: Value<T>, time: Time) -> impl FnOnce(&mut Machine) + 'static {
    move |machine: &mut Machine| {
        machine.values.pop(value, time);
    }
}

/// A bootstrapped write of `contents` to `value` at `time`, attached to
/// `event`, waiting in the queue. Applying it moves the contents into the
/// value's history; its undo pops them and queues the write again, so the
/// contents only ever move.
fn queued_write<T: 'static>(value: Value<T>, time: Time, event: Time, contents: T) -> Bootstrapped {
    Bootstrapped::new(time, move |machine: &mut Machine| -> Undo {
        let slot = machine
            .values
            .slot_mut(value)
            .ok()
            .and_then(|slot| slot.live_mut(value.id).ok())
            .expect(
                "a bootstrapped write's value exists at its point, as long as the program \
                 depends on nothing but the machine",
            );
        slot.history.push(HistoryEntry {
            time,
            snapshot: contents,
        });
        Box::new(move |machine: &mut Machine| {
            let entry = machine.values.pop(value, time);
            let write = queued_write(value, time, event, entry.snapshot);
            machine.requeue(event, write);
        })
    })
}

impl Machine {
    /// Creates a value holding `initial`. The creation is an event, and its
    /// point is both the value's identity and the time of `initial`, its
    /// version 0. Undoing it removes the value.
    ///
    /// Fails with [`NotRunning`] outside a run.
    pub fn create_value<T: 'static>(&mut self, initial: T) -> Result<Value<T>, NotRunning> {
        self.event(|machine, time| {
            let value = Value::new(ValueId(time));
            let slot = Slot {
                history: vec![HistoryEntry {
                    time,
                    snapshot: initial,
                }],
                deleted: None,
            };
            machine.values.insert(value, slot);
            let undo = move |machine: &mut Machine| {
                machine.values.slots.remove(&value.id).expect(
                    "undos run newest first, so a value is there when its creation is undone",
                );
            };
            Ok((value, undo))
        })
    }

    /// The value now: its newest write. During a run, the read is an event.
    pub fn read<T: 'static>(&mut self, value: Value<T>) -> Result<&T, ReadError> {
        let index = self.values.slot(value)?.live(value.id)?.newest(value.id)?;
        self.observation();
        Ok(self.values.snapshot(value, index))
    }

    /// The value as seen at `time`: its newest write at or before it, or
    /// `None` if the value was written only after `time`. During a run, the
    /// read is an event.
    ///
    /// Fails with [`DeletedValue`] if the value was deleted at or before
    /// `time`: before its deletion, its past can still be read.
    pub fn read_at<T: 'static>(
        &mut self,
        value: Value<T>,
        time: Time,
    ) -> Result<Option<&T>, ReadAtError> {
        let slot = self.values.slot(value)?;
        if !self.timeline.contains(time) {
            return Err(UnknownTime(time).into());
        }
        if let Some(deleted) = slot.deleted
            && self.order(deleted, time) != Ordering::Greater
        {
            return Err(DeletedValue(value.id).into());
        }
        // The history is sorted by time.
        let after = slot
            .history
            .partition_point(|entry| self.order(entry.time, time) != Ordering::Greater);
        let index = after.checked_sub(1);
        self.observation();
        Ok(index.map(|index| self.values.snapshot(value, index)))
    }

    /// Replaces the value with `new`: a write event, at its own point, that
    /// makes a new version. Undoing it removes its history entry.
    ///
    /// Fails with [`NotRunning`] outside a run.
    pub fn write<T: 'static>(&mut self, value: Value<T>, new: T) -> Result<(), WriteError> {
        self.event(|machine, time| {
            let slot = machine.values.slot_mut(value)?.live_mut(value.id)?;
            slot.history.push(HistoryEntry {
                time,
                snapshot: new,
            });
            Ok(((), undo_write(value, time)))
        })
    }

    /// Writes a copy of the newest contents, changed by `f`. One event,
    /// which reads the value and writes it, and is undone like
    /// [`write`](Self::write). The old snapshot stays in the history.
    ///
    /// Fails with [`NotRunning`] outside a run, before calling `f`.
    pub fn update<T: Clone + 'static>(
        &mut self,
        value: Value<T>,
        f: impl FnOnce(&mut T),
    ) -> Result<(), UpdateError> {
        if !self.is_running() {
            return Err(NotRunning.into());
        }
        let slot = self.values.slot(value)?.live(value.id)?;
        let mut new = slot.history[slot.newest(value.id)?].snapshot.clone();
        // Called outside the event, so a panic in `f` leaves no event.
        f(&mut new);
        self.event(|machine, time| {
            let slot = machine.values.slot_mut(value)?.live_mut(value.id)?;
            slot.history.push(HistoryEntry {
                time,
                snapshot: new,
            });
            Ok(((), undo_write(value, time)))
        })
    }

    /// Deletes the value: an event that marks it deleted. From then on,
    /// reading, writing or deleting it fails with [`DeletedValue`]. Undoing
    /// it un-deletes the value.
    ///
    /// Fails with [`NotRunning`] outside a run.
    pub fn delete<T: 'static>(&mut self, value: Value<T>) -> Result<(), DeleteError> {
        self.event(|machine, time| {
            machine.values.slot_mut(value)?.live_mut(value.id)?.deleted = Some(time);
            let undo = move |machine: &mut Machine| {
                let slot = machine
                    .values
                    .slot_mut(value)
                    .expect("undos run newest first, so a deletion is undone before the creation");
                slot.deleted = None;
            };
            Ok(((), undo))
        })
    }

    /// The value's newest version. During a run, the inspection is an
    /// event.
    pub fn version<T: 'static>(&mut self, value: Value<T>) -> Result<u64, VersionError> {
        let newest = self.values.slot(value)?.live(value.id)?.newest(value.id)?;
        self.observation();
        Ok(newest as u64)
    }

    /// The value's whole history, oldest first: each entry's index is its
    /// version. It's still there after the value is deleted. During a run,
    /// the inspection is an event.
    pub fn history<T: 'static>(
        &mut self,
        value: Value<T>,
    ) -> Result<&[HistoryEntry<T>], UnknownValue> {
        self.values.slot(value)?;
        self.observation();
        let slot = self.values.slot(value).expect("the value was just found");
        Ok(&slot.history)
    }

    /// Writes `contents` into the past, right after `time`, which must be
    /// the point of an earlier event in this run. The value must exist at
    /// that point: created at or before it, and not deleted at or before
    /// it.
    ///
    /// The write becomes a bootstrapped write, attached to the event at
    /// `time` and kept across runs: every run that executes that event
    /// executes the write right after it, at the same point. By rule 2, it
    /// wipes everything after it, including the bootstrapped writes after
    /// it (those attached to later events, and older ones attached to the
    /// same event). Then the run stops, since the rest of it comes after the
    /// write, and [`run`](Self::run) starts the next one. The contents only
    /// move, between the queue of bootstrapped writes and the value's
    /// history, so they don't need `Clone`.
    ///
    /// So on success, `write_at` doesn't return: it unwinds, with a private
    /// payload that skips the panic hook, so nothing is printed. That needs
    /// `panic = "unwind"`, and the program mustn't catch the unwind itself:
    /// see [`run`](Self::run).
    pub fn write_at<T: 'static>(
        &mut self,
        value: Value<T>,
        time: Time,
        contents: T,
    ) -> Result<Infallible, WriteAtError> {
        if !self.is_running() {
            return Err(NotRunning.into());
        }
        let slot = self.values.slot(value)?;
        if !self.is_event_of_this_run(time) {
            return Err(NotInThePast(time).into());
        }
        // The id is the creation's point.
        if self.order(value.id.0, time) == Ordering::Greater {
            return Err(NotYetCreated {
                value: value.id,
                time,
            }
            .into());
        }
        if let Some(deleted) = slot.deleted
            && self.order(deleted, time) != Ordering::Greater
        {
            return Err(DeletedValue(value.id).into());
        }

        let make = move |point: Time, event: Time| queued_write(value, point, event, contents);
        self.stop(time, Box::new(make))
    }
}

#[cfg(test)]
#[path = "value_tests.rs"]
mod tests;
