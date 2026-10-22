//! A timeline: points in time that can be compared in O(1), and inserted
//! between any two existing points.
//!
//! Each [`Machine`](crate::machine::Machine) has one timeline, the order of
//! its run, and hands it out read-only with
//! [`timeline`](crate::machine::Machine::timeline): only the machine adds
//! points, as steps run and values are written. A [`Timeline`] is also a
//! plain data structure of its own:
//!
//! ```
//! use caaqi_core::time::Timeline;
//! use std::cmp::Ordering;
//!
//! let mut timeline = Timeline::new();
//! let start = timeline.push_back();
//! let end = timeline.push_back();
//! let middle = timeline.insert_after(start).unwrap();
//! assert_eq!(timeline.time_cmp(start, middle), Ok(Ordering::Less));
//! assert_eq!(timeline.time_cmp(middle, end), Ok(Ordering::Less));
//! ```
//!
//! Under the hood, each point has a number, its label, and labels increase
//! along the timeline. Inserting takes a number between the neighbors'
//! labels; when there's none left, the labels are spread out again. So
//! labels change, and they stay private: code stores [`Time`] handles,
//! which never change.

use std::cmp::Ordering;

use slotmap::SlotMap;

slotmap::new_key_type! {
    /// The slot of a point in the timeline.
    struct TimeKey;
}

/// A point on a timeline. `Copy`, and compared in O(1) with
/// [`Timeline::time_cmp`].
///
/// A `Time` doesn't borrow its timeline, so nothing keeps its point alive:
/// the timeline's methods return [`UnknownTime`] for a point that was
/// [removed](Timeline::remove). A `Time` is only valid in the timeline that
/// created it: in another, it's unsupported and may refer to an unrelated
/// point.
///
/// It isn't ordered by itself: comparing needs the timeline, whose order
/// can change when points [move](Timeline::move_after).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Time(TimeKey);

/// The point isn't on the timeline, because it was removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("{0:?} isn't on the timeline")]
pub struct UnknownTime(pub Time);

/// A point can't be moved relative to itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("can't move {0:?} relative to itself")]
pub struct SameTime(pub Time);

/// Why [`Timeline::move_after`] or [`Timeline::move_before`] failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MoveTimeError {
    #[error(transparent)]
    UnknownTime(#[from] UnknownTime),
    #[error(transparent)]
    SameTime(#[from] SameTime),
}

/// One point's place in the timeline.
struct Entry {
    /// Strictly increases along the list. Changes when labels are respread.
    label: u64,
    prev: Option<TimeKey>,
    next: Option<TimeKey>,
}

/// A doubly linked list of points in timeline order.
#[derive(Default)]
pub struct Timeline {
    entries: SlotMap<TimeKey, Entry>,
    head: Option<TimeKey>,
    tail: Option<TimeKey>,
    /// How many times the labels were respread, so tests can check they were.
    #[cfg(test)]
    respreads: usize,
}

impl Timeline {
    /// An empty timeline.
    pub fn new() -> Timeline {
        Timeline::default()
    }

    /// Adds a point at the end.
    pub fn push_back(&mut self) -> Time {
        let key = self.create();
        self.link(key, self.tail, None);
        Time(key)
    }

    /// Adds a point at the start.
    pub fn push_front(&mut self) -> Time {
        let key = self.create();
        self.link(key, None, self.head);
        Time(key)
    }

    /// The first point, or `None` if the timeline is empty.
    pub fn first(&self) -> Option<Time> {
        self.head.map(Time)
    }

    /// The last point, or `None` if the timeline is empty.
    pub fn last(&self) -> Option<Time> {
        self.tail.map(Time)
    }

    /// How many points the timeline has.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the timeline has no points.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Every point, in timeline order.
    pub fn times(&self) -> impl Iterator<Item = Time> + '_ {
        std::iter::successors(self.head, |&key| self.entries[key].next).map(Time)
    }

    /// Whether `time` is on the timeline. When it isn't, because it was
    /// removed, the other methods return [`UnknownTime`] for it.
    pub fn contains(&self, time: Time) -> bool {
        self.entries.contains_key(time.0)
    }

    /// Adds a point right after `time`.
    pub fn insert_after(&mut self, time: Time) -> Result<Time, UnknownTime> {
        let next = self.entry(time)?.next;
        let key = self.create();
        self.link(key, Some(time.0), next);
        Ok(Time(key))
    }

    /// Adds a point right before `time`.
    pub fn insert_before(&mut self, time: Time) -> Result<Time, UnknownTime> {
        let prev = self.entry(time)?.prev;
        let key = self.create();
        self.link(key, prev, Some(time.0));
        Ok(Time(key))
    }

    /// Moves `time` to right after `anchor`. It keeps its handle, and sorts
    /// in its new place from now on.
    ///
    /// Fails if either point isn't on the timeline, or with [`SameTime`] if
    /// `anchor` is `time`.
    pub fn move_after(&mut self, time: Time, anchor: Time) -> Result<(), MoveTimeError> {
        self.unlink_to_move(time, anchor)?;
        let next = self.entries[anchor.0].next;
        self.link(time.0, Some(anchor.0), next);
        Ok(())
    }

    /// Moves `time` to right before `anchor`. It keeps its handle, and
    /// sorts in its new place from now on.
    ///
    /// Fails if either point isn't on the timeline, or with [`SameTime`] if
    /// `anchor` is `time`.
    pub fn move_before(&mut self, time: Time, anchor: Time) -> Result<(), MoveTimeError> {
        self.unlink_to_move(time, anchor)?;
        let prev = self.entries[anchor.0].prev;
        self.link(time.0, prev, Some(anchor.0));
        Ok(())
    }

    /// Removes `time` from the timeline. From then on, the handle is
    /// unknown.
    pub fn remove(&mut self, time: Time) -> Result<(), UnknownTime> {
        self.entry(time)?;
        self.unlink(time.0);
        self.entries.remove(time.0);
        Ok(())
    }

    /// Compares `a` with `b` by timeline order: `Less` when `a` comes
    /// first. O(1).
    ///
    /// Fails with whichever point isn't on the timeline.
    pub fn time_cmp(&self, a: Time, b: Time) -> Result<Ordering, UnknownTime> {
        Ok(self.entry(a)?.label.cmp(&self.entry(b)?.label))
    }

    /// The point right after `time`, or `None` if it's the last.
    pub fn next(&self, time: Time) -> Result<Option<Time>, UnknownTime> {
        Ok(self.entry(time)?.next.map(Time))
    }

    /// The point right before `time`, or `None` if it's the first.
    pub fn prev(&self, time: Time) -> Result<Option<Time>, UnknownTime> {
        Ok(self.entry(time)?.prev.map(Time))
    }

    fn entry(&self, time: Time) -> Result<&Entry, UnknownTime> {
        self.entries.get(time.0).ok_or(UnknownTime(time))
    }

    fn label(&self, key: TimeKey) -> u64 {
        self.entries[key].label
    }

    /// Adds a point, not linked into the list yet.
    fn create(&mut self) -> TimeKey {
        self.entries.insert(Entry {
            label: 0,
            prev: None,
            next: None,
        })
    }

    /// Links the unlinked point `key` between `prev` and `next`, which must
    /// be neighbors (or the ends of the list), and gives it a label between
    /// theirs.
    fn link(&mut self, key: TimeKey, prev: Option<TimeKey>, next: Option<TimeKey>) {
        let label = match self.label_between(prev, next) {
            Some(label) => label,
            None => {
                self.respread();
                self.label_between(prev, next)
                    .expect("respreading leaves room between every two neighbors")
            }
        };

        let entry = &mut self.entries[key];
        entry.label = label;
        entry.prev = prev;
        entry.next = next;
        match prev {
            Some(prev) => self.entries[prev].next = Some(key),
            None => self.head = Some(key),
        }
        match next {
            Some(next) => self.entries[next].prev = Some(key),
            None => self.tail = Some(key),
        }
    }

    /// The midpoint of the labels around the gap between `prev` and `next`,
    /// or `None` if no integer is strictly between them. The ends of the
    /// list are bounded by 0 and `u64::MAX`.
    fn label_between(&self, prev: Option<TimeKey>, next: Option<TimeKey>) -> Option<u64> {
        let low = prev.map_or(0, |prev| self.label(prev));
        let high = next.map_or(u64::MAX, |next| self.label(next));
        (high - low >= 2).then(|| low + (high - low) / 2)
    }

    /// Takes `key` out of the list, joining its neighbors. It stays in
    /// `entries`, so it can be linked again under the same handle.
    fn unlink(&mut self, key: TimeKey) {
        let Entry { prev, next, .. } = self.entries[key];
        match prev {
            Some(prev) => self.entries[prev].next = next,
            None => self.head = next,
        }
        match next {
            Some(next) => self.entries[next].prev = prev,
            None => self.tail = prev,
        }
        let entry = &mut self.entries[key];
        entry.prev = None;
        entry.next = None;
    }

    /// Spreads the labels of the linked points evenly over the `u64` range,
    /// in list order. O(n), but rare: only when a gap runs out.
    fn respread(&mut self) {
        // This could be replaced by Bender et al.'s local relabeling ("Two
        // simplified algorithms for maintaining order in a list", 2002),
        // which relabels only a small range around the gap. Relabeling stays
        // in this one function so that swap doesn't change anything else.
        #[cfg(test)]
        {
            self.respreads += 1;
        }

        let count = self.entries.len() as u64;
        // `count` includes the point being linked, so there's a spare gap.
        let step = u64::MAX / (count + 1);
        let mut label = step;
        let mut cursor = self.head;
        while let Some(key) = cursor {
            let entry = &mut self.entries[key];
            entry.label = label;
            label += step;
            cursor = entry.next;
        }
    }

    /// Checks that `time` can move next to `anchor`, and unlinks it.
    fn unlink_to_move(&mut self, time: Time, anchor: Time) -> Result<(), MoveTimeError> {
        self.entry(time)?;
        self.entry(anchor)?;
        if time == anchor {
            return Err(SameTime(time).into());
        }
        self.unlink(time.0);
        Ok(())
    }
}

#[cfg(test)]
#[path = "time_tests.rs"]
mod tests;
