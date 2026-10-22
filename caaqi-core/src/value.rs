//! Values: state inside the timeline.
//!
//! A value is reached through a [`Value<T>`] handle. Reads and writes are
//! recorded on the [current step](Machine::current_step), as the edges of
//! the dependency graph; outside any step, they aren't recorded.
//!
//! Each value keeps its history: one [`HistoryEntry`] per write, with the
//! point the write added to the timeline and a snapshot of the value. So the
//! value as seen at any point is the newest write at or before it, and
//! [`read_at`](Machine::read_at) returns what a step there saw. Writes
//! always add their point at the end of the timeline, so the history stays
//! sorted.
//!
//! A write's version is its index in the history: 0 for the value's
//! first write, then one more per write.
//!
//! A value's identity and its contents are owned separately. A write belongs
//! to the step whose run decides what it holds, and the value itself belongs
//! to the section whose rewrite would end its lifetime. Usually both are the
//! step that creates the value, but a step's result is declared by the
//! parent and written by the step: see
//! [`run_step`](Machine::run_step).
//!
//! ```
//! use caaqi_core::prelude::*;
//!
//! let mut machine = Machine::new();
//! let count = machine.create_value(1);
//! let (step, _) = machine.run_step_with_id(|machine| {
//!     let doubled = *machine.read(count).unwrap() * 2;
//!     machine.write(count, doubled).unwrap();
//! });
//!
//! assert_eq!(*machine.read(count).unwrap(), 2);
//! // At the step's start, the value hadn't been written yet.
//! let start = machine.step(step).unwrap().start();
//! assert_eq!(machine.read_at(count, start).unwrap(), Some(&1));
//! ```

use std::{any::Any, cmp::Ordering, fmt, hash, marker::PhantomData};

use slotmap::SlotMap;

use crate::{
    machine::Machine,
    time::{Time, UnknownTime},
    trace::Access,
};

slotmap::new_key_type! {
    /// A value, whatever its type: what [`Access`] records. Only valid in
    /// the [`Machine`] that created it.
    pub struct ValueId;
}

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
    /// The value's untyped id, as recorded in [`Access`]es.
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

/// The value isn't in this machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("{0:?} isn't in the machine")]
pub struct UnknownValue(pub ValueId);

/// Why [`Machine::read_at`] failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ReadAtError {
    #[error(transparent)]
    UnknownValue(#[from] UnknownValue),
    #[error(transparent)]
    UnknownTime(#[from] UnknownTime),
}

/// One write in a value's history.
#[derive(Debug)]
pub struct HistoryEntry<T> {
    time: Time,
    snapshot: T,
}

impl<T> HistoryEntry<T> {
    /// The point the write added to the timeline.
    pub fn time(&self) -> Time {
        self.time
    }

    /// The value as the write left it.
    pub fn snapshot(&self) -> &T {
        &self.snapshot
    }
}

/// Every value's history. Each is a `Vec<HistoryEntry<T>>` for its own `T`,
/// sorted by time. A history is empty only for a declared value that hasn't
/// been written yet, and such a value is never handed out.
#[derive(Default)]
pub(crate) struct Values {
    histories: SlotMap<ValueId, Box<dyn Any>>,
}

impl Values {
    /// Removes a declared value that was never written, such as the result
    /// of a step that panicked.
    pub(crate) fn remove_unwritten<T: 'static>(&mut self, value: Value<T>) {
        let history = self
            .histories
            .remove(value.id)
            .expect("the declared value is in the machine");
        let history = history
            .downcast::<Vec<HistoryEntry<T>>>()
            .expect("the declared value has its type");
        assert!(history.is_empty(), "the declared value was never written");
    }

    fn history<T: 'static>(&self, value: Value<T>) -> Result<&[HistoryEntry<T>], UnknownValue> {
        self.histories
            .get(value.id)
            .and_then(|history| history.downcast_ref::<Vec<HistoryEntry<T>>>())
            .map(Vec::as_slice)
            .ok_or(UnknownValue(value.id))
    }

    fn history_mut<T: 'static>(
        &mut self,
        value: Value<T>,
    ) -> Result<&mut Vec<HistoryEntry<T>>, UnknownValue> {
        self.histories
            .get_mut(value.id)
            .and_then(|history| history.downcast_mut::<Vec<HistoryEntry<T>>>())
            .ok_or(UnknownValue(value.id))
    }
}

/// The newest version of a history, which is its last index.
fn newest<T>(history: &[HistoryEntry<T>]) -> usize {
    history.len().checked_sub(1).expect(
        "only written values are handed out, and a value's history is empty only before its first write",
    )
}

/// The version of the entry at `index`.
fn version(index: usize) -> u64 {
    index as u64
}

impl Machine {
    /// Declares a value with no contents yet: its history is empty until
    /// its first [`write`](Self::write), which is version 0.
    ///
    /// Declaring isn't a write. It makes no version, adds no point to the
    /// timeline, and records nothing on the current step. It only gives the
    /// value its identity, so the value can be declared in one place and
    /// written in another: a step's result is declared before the step runs
    /// and written by the step itself.
    ///
    /// Until the first write, the value has no contents to see:
    /// [`read_at`](Self::read_at) returns `None` at any point.
    // TODO: make this public. See CLAUDE.md for what that takes.
    pub(crate) fn declare_value<T: 'static>(&mut self) -> Value<T> {
        let history: Vec<HistoryEntry<T>> = Vec::new();
        Value {
            id: self.values.histories.insert(Box::new(history)),
            _type: PhantomData,
        }
    }

    /// Creates a value holding `initial`: it declares the value, then writes
    /// `initial` as its first version, 0, at a new point at the end of the
    /// timeline. The write is recorded on the current step.
    pub fn create_value<T: 'static>(&mut self, initial: T) -> Value<T> {
        let value = self.declare_value();
        self.write(value, initial)
            .expect("the value was just declared");
        value
    }

    /// The value now: its newest write. The read is recorded on the current
    /// step.
    pub fn read<T: 'static>(&mut self, value: Value<T>) -> Result<&T, UnknownValue> {
        let index = newest(self.values.history(value)?);
        Ok(self.read_index(value, index))
    }

    /// The value as seen at `time`: its newest write at or before it, or
    /// `None` if the value's first write is after `time`. When there's one,
    /// the read is recorded on the current step.
    pub fn read_at<T: 'static>(
        &mut self,
        value: Value<T>,
        time: Time,
    ) -> Result<Option<&T>, ReadAtError> {
        let Some(index) = self.index_at(value, time)? else {
            return Ok(None);
        };
        Ok(Some(self.read_index(value, index)))
    }

    /// Replaces the value with `new`: a new version, at a new point at the
    /// end of the timeline. The write is recorded on the current step. On a
    /// value that hasn't been written yet, this first write is version 0.
    pub fn write<T: 'static>(&mut self, value: Value<T>, new: T) -> Result<(), UnknownValue> {
        // Fail before adding a point for a value that isn't there.
        let index = self.values.history(value)?.len();
        let time = self.timeline.push_back();
        self.values.history_mut(value)?.push(HistoryEntry {
            time,
            snapshot: new,
        });
        self.trace.record_write(Access {
            value: value.id,
            version: version(index),
        });
        Ok(())
    }

    /// Writes a copy of the value, changed by `f`. Like
    /// [`write`](Self::write), the old snapshot stays in the history. The
    /// new value depends on the old one, so both the read and the write are
    /// recorded on the current step.
    pub fn update<T: Clone + 'static>(
        &mut self,
        value: Value<T>,
        f: impl FnOnce(&mut T),
    ) -> Result<(), UnknownValue> {
        let mut new = self.read(value)?.clone();
        f(&mut new);
        self.write(value, new)
    }

    /// The value's newest version. For inspecting the run: it isn't
    /// recorded as a read.
    pub fn version<T: 'static>(&self, value: Value<T>) -> Result<u64, UnknownValue> {
        Ok(version(newest(self.values.history(value)?)))
    }

    /// The value's whole history, oldest first: each entry's index is its
    /// version. For inspecting the run: it isn't recorded as a read.
    pub fn history<T: 'static>(&self, value: Value<T>) -> Result<&[HistoryEntry<T>], UnknownValue> {
        self.values.history(value)
    }

    /// Records a read of the entry at `index`, which must be in the value's
    /// history, and returns its snapshot.
    fn read_index<T: 'static>(&mut self, value: Value<T>, index: usize) -> &T {
        self.trace.record_read(Access {
            value: value.id,
            version: version(index),
        });
        let history = self.values.history(value).expect("the value was found");
        history[index].snapshot()
    }

    /// The index of the newest entry at or before `time`, found by binary
    /// search, since the history is sorted by time.
    fn index_at<T: 'static>(
        &self,
        value: Value<T>,
        time: Time,
    ) -> Result<Option<usize>, ReadAtError> {
        let history = self.values.history(value)?;
        if !self.timeline.contains(time) {
            return Err(UnknownTime(time).into());
        }
        let after = history.partition_point(|entry| {
            let order = self
                .timeline
                .time_cmp(entry.time, time)
                .expect("history points stay on the timeline");
            order != Ordering::Greater
        });
        Ok(after.checked_sub(1))
    }
}

#[cfg(test)]
#[path = "value_tests.rs"]
mod tests;
