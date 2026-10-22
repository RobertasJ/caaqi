//! Values, scoped: state inside the timeline, through [`Value<T>`] handles.
//!
//! Reads and writes are recorded on the [current step](crate::trace::current_step),
//! and each value keeps its history, so [`get_at`](Value::get_at) returns
//! what a step at that point saw:
//!
//! ```
//! use caaqi::prelude::*;
//!
//! scope(&mut Machine::new(), || {
//!     let count = create_value(1);
//!     let (step, ()) = run_step(|| count.set(count.get() * 2));
//!
//!     assert_eq!(count.get(), 2);
//!     assert_eq!(count.get_at(step.start()), Some(1));
//! });
//! ```

use caaqi_core::value::Value as CoreValue;
pub use caaqi_core::value::{ReadAtError, UnknownValue, ValueId};

use crate::{machine::with_machine, time::Time};

/// A handle to a value of type `T` in the machine. `Copy` whatever `T` is.
///
/// A newtype over caaqi-core's [`Value`](caaqi_core::value::Value), with
/// `From` both ways. Errors carry the untyped [`ValueId`]. A `Value` is only
/// valid in the machine that created it.
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
    /// The point the write added to the timeline.
    pub time: Time,
    /// The value as the write left it.
    pub snapshot: T,
}

/// Creates a value holding `initial`. Its creation is its first write,
/// version 0, and it's recorded on the current step.
pub fn create_value<T: 'static>(initial: T) -> Value<T> {
    with_machine(|machine| Value(machine.create_value(initial)))
}

impl<T: 'static> Value<T> {
    /// The value's untyped id, as recorded in
    /// [`Access`](crate::trace::Access)es.
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

    /// A copy of the value now. The read is recorded on the current step.
    pub fn try_get(self) -> Result<T, UnknownValue>
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

    /// Calls `f` with the value now, without copying it. The read is
    /// recorded on the current step.
    ///
    /// `f` runs while the machine is out of scope, so it mustn't call this
    /// crate's functions.
    pub fn try_with<R>(self, f: impl FnOnce(&T) -> R) -> Result<R, UnknownValue> {
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
    /// it, or `None` if the value was created after `time`. When there's
    /// one, the read is recorded on the current step.
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

    /// Replaces the value with `new`: a new version, at a new point at the
    /// end of the timeline. The write is recorded on the current step.
    pub fn try_set(self, new: T) -> Result<(), UnknownValue> {
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

    /// Writes a copy of the value, changed by `f`. Like [`set`](Self::set),
    /// the old snapshot stays in the history. Both the read and the write
    /// are recorded on the current step.
    ///
    /// `f` runs while the machine is out of scope, so it mustn't call this
    /// crate's functions.
    pub fn try_update(self, f: impl FnOnce(&mut T)) -> Result<(), UnknownValue>
    where
        T: Clone,
    {
        with_machine(|machine| machine.update(self.0, f))
    }

    /// Like [`try_version`](Self::try_version), but panics on error.
    #[track_caller]
    pub fn version(self) -> u64 {
        self.try_version().unwrap_or_else(|error| panic!("{error}"))
    }

    /// The value's newest version. For inspecting the run: it isn't
    /// recorded as a read.
    pub fn try_version(self) -> Result<u64, UnknownValue> {
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
    /// index is its version. For inspecting the run: it isn't recorded as a
    /// read.
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
