//! Values, scoped: state inside the timeline, through [`Value<T>`] handles.
//!
//! Creating, writing and deleting work only while the program runs.
//! Reading and inspecting work at any time (outside a run, inside a
//! [`scope`](crate::machine::scope)), and during a run they're events too.
//! Each value keeps its history, so [`get_at`](Value::get_at) returns what
//! a read at that point saw, and [`write_at`](Value::write_at) writes into
//! the past:
//!
//! ```
//! use caaqi::prelude::*;
//!
//! let (mut machine, result) = new_machine(|| {
//!     let count = create_value(0);
//!     let start = now();
//!     if count.get() == 0 {
//!         // The run stops here, and the next one sees 10 after `start`.
//!         count.write_at(start, 10);
//!     }
//!     (count, start)
//! });
//! assert_eq!(machine.run(), Ok(2));
//!
//! scope(&mut machine, || {
//!     let (count, start) = result.get();
//!     assert_eq!(count.get(), 10);
//!     assert_eq!(count.get_at(start), Some(0));
//! });
//! ```

use std::convert::Infallible;

use caaqi_core::value::Value as CoreValue;
pub use caaqi_core::value::{
    DeleteError, DeletedValue, NotInThePast, NotYetCreated, ReadAtError, ReadError, UnknownValue,
    Unwritten, UpdateError, ValueId, VersionError, WriteAtError, WriteError,
};

use crate::{
    machine::{NotRunning, with_machine},
    time::Time,
};

/// A handle to a value of type `T` in the machine. `Copy` whatever `T` is.
///
/// A newtype over caaqi-core's [`Value`](caaqi_core::value::Value), with
/// `From` both ways. Errors carry the untyped [`ValueId`]. A `Value` is only
/// valid in the machine that created it, and stays valid across reruns.
pub struct Value<T>(CoreValue<T>);

impl<T> Clone for Value<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Value<T> {}

impl<T> PartialEq for Value<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<T> Eq for Value<T> {}

impl<T> std::hash::Hash for Value<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl<T> std::fmt::Debug for Value<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl<T> From<CoreValue<T>> for Value<T> {
    fn from(value: CoreValue<T>) -> Value<T> {
        Value(value)
    }
}

impl<T> From<Value<T>> for CoreValue<T> {
    fn from(value: Value<T>) -> CoreValue<T> {
        value.0
    }
}

/// One write in a value's history, copied out of the machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryEntry<T> {
    /// The write's point.
    pub time: Time,
    /// The value as the write left it.
    pub snapshot: T,
}

/// Like [`try_create_value`], but panics on error.
#[track_caller]
pub fn create_value<T: 'static>(initial: T) -> Value<T> {
    try_create_value(initial).unwrap_or_else(|error| panic!("{error}"))
}

/// Creates a value holding `initial`: an event whose point is the value's
/// identity and the time of its version 0.
///
/// Fails with [`NotRunning`] outside a run.
pub fn try_create_value<T: 'static>(initial: T) -> Result<Value<T>, NotRunning> {
    with_machine(|machine| machine.create_value(initial).map(Value))
}

impl<T: 'static> Value<T> {
    /// The value's untyped id.
    pub fn id(self) -> ValueId {
        self.0.id()
    }

    /// Like [`try_get`](Self::try_get), but panics on error.
    #[track_caller]
    pub fn get(self) -> T
    where
        T: Clone,
    {
        self.try_get().unwrap_or_else(|error| panic!("{error}"))
    }

    /// A copy of the value now: its newest write. During a run, the read is
    /// an event.
    pub fn try_get(self) -> Result<T, ReadError>
    where
        T: Clone,
    {
        self.try_with(T::clone)
    }

    /// Like [`try_with`](Self::try_with), but panics on error.
    #[track_caller]
    pub fn with<R>(self, f: impl FnOnce(&T) -> R) -> R {
        self.try_with(f).unwrap_or_else(|error| panic!("{error}"))
    }

    /// Calls `f` with the value now, without copying it. During a run, the
    /// read is an event.
    ///
    /// `f` runs while the machine is out of scope, so it mustn't call this
    /// crate's functions.
    pub fn try_with<R>(self, f: impl FnOnce(&T) -> R) -> Result<R, ReadError> {
        with_machine(|machine| machine.read(self.0).map(f))
    }

    /// Like [`try_get_at`](Self::try_get_at), but panics on error.
    #[track_caller]
    pub fn get_at(self, time: Time) -> Option<T>
    where
        T: Clone,
    {
        self.try_get_at(time)
            .unwrap_or_else(|error| panic!("{error}"))
    }

    /// A copy of the value as seen at `time`: its newest write at or before
    /// it, or `None` if the value was written only after `time`. During a
    /// run, the read is an event.
    ///
    /// Fails with [`DeletedValue`] if the value was deleted at or before
    /// `time`.
    pub fn try_get_at(self, time: Time) -> Result<Option<T>, ReadAtError>
    where
        T: Clone,
    {
        with_machine(|machine| Ok(machine.read_at(self.0, time.into())?.cloned()))
    }

    /// Like [`try_set`](Self::try_set), but panics on error.
    #[track_caller]
    pub fn set(self, new: T) {
        self.try_set(new).unwrap_or_else(|error| panic!("{error}"));
    }

    /// Replaces the value with `new`: a write event, at its own point, that
    /// makes a new version.
    ///
    /// Fails with [`NotRunning`] outside a run.
    pub fn try_set(self, new: T) -> Result<(), WriteError> {
        with_machine(|machine| machine.write(self.0, new))
    }

    /// Like [`try_update`](Self::try_update), but panics on error.
    #[track_caller]
    pub fn update(self, f: impl FnOnce(&mut T))
    where
        T: Clone,
    {
        self.try_update(f).unwrap_or_else(|error| panic!("{error}"));
    }

    /// Writes a copy of the value, changed by `f`: one event that reads and
    /// writes it. Like [`set`](Self::set), the old snapshot stays in the
    /// history.
    ///
    /// `f` runs while the machine is out of scope, so it mustn't call this
    /// crate's functions. Fails with [`NotRunning`] outside a run.
    pub fn try_update(self, f: impl FnOnce(&mut T)) -> Result<(), UpdateError>
    where
        T: Clone,
    {
        with_machine(|machine| machine.update(self.0, f))
    }

    /// Like [`try_delete`](Self::try_delete), but panics on error.
    #[track_caller]
    pub fn delete(self) {
        self.try_delete().unwrap_or_else(|error| panic!("{error}"));
    }

    /// Deletes the value: an event that marks it deleted. From then on,
    /// reading, writing or deleting it fails with [`DeletedValue`].
    ///
    /// Fails with [`NotRunning`] outside a run.
    pub fn try_delete(self) -> Result<(), DeleteError> {
        with_machine(|machine| machine.delete(self.0))
    }

    /// Like [`try_write_at`](Self::try_write_at), but panics on error.
    #[track_caller]
    pub fn write_at(self, time: Time, contents: T) -> ! {
        match self.try_write_at(time, contents) {
            Ok(never) => match never {},
            Err(error) => panic!("{error}"),
        }
    }

    /// Writes `contents` into the past, right after `time`, which must be
    /// the point of an earlier event in this run, where the value exists:
    /// [`Machine::write_at`](caaqi_core::machine::Machine::write_at), scoped.
    ///
    /// The write wipes everything after it and stops the run, so on success
    /// this doesn't return: it unwinds (through the scope, which puts the
    /// machine back as the unwind passes), and the machine runs the program
    /// again, with the write at its point. That needs `panic = "unwind"`,
    /// and the program mustn't catch unwinds itself.
    pub fn try_write_at(self, time: Time, contents: T) -> Result<Infallible, WriteAtError> {
        with_machine(|machine| machine.write_at(self.0, time.into(), contents))
    }

    /// Like [`try_version`](Self::try_version), but panics on error.
    #[track_caller]
    pub fn version(self) -> u64 {
        self.try_version().unwrap_or_else(|error| panic!("{error}"))
    }

    /// The value's newest version. During a run, the inspection is an
    /// event.
    pub fn try_version(self) -> Result<u64, VersionError> {
        with_machine(|machine| machine.version(self.0))
    }

    /// Like [`try_history`](Self::try_history), but panics on error.
    #[track_caller]
    pub fn history(self) -> Vec<HistoryEntry<T>>
    where
        T: Clone,
    {
        self.try_history().unwrap_or_else(|error| panic!("{error}"))
    }

    /// A copy of the value's whole history, oldest first: each entry's
    /// index is its version. It's still there after the value is deleted.
    /// During a run, the inspection is an event.
    pub fn try_history(self) -> Result<Vec<HistoryEntry<T>>, UnknownValue>
    where
        T: Clone,
    {
        with_machine(|machine| {
            let history = machine.history(self.0)?;
            Ok(history
                .iter()
                .map(|entry| HistoryEntry {
                    time: entry.time().into(),
                    snapshot: entry.snapshot().clone(),
                })
                .collect())
        })
    }
}

#[cfg(test)]
#[path = "value_tests.rs"]
mod tests;
