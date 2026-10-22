//! A timeline: points in time that can be compared in O(1), and inserted
//! between any two existing points.
//!
//! Each context has one timeline. Its points are [`Time`]s, which code can
//! keep sorted by time and binary-search, since comparing two of them is
//! cheap:
//!
//! ```
//! use caaqi::time::push_back;
//! use caaqi_core_scoped::context::{Context, scope};
//!
//! scope(&mut Context::new(), || {
//!     let start = push_back();
//!     let end = push_back();
//!     let middle = start.insert_after();
//!     assert!(start < middle && middle < end);
//! });
//! ```
//!
//! Under the hood, each point has a number, its label, and labels increase
//! along the timeline. Inserting takes a number between the neighbors'
//! labels; when there's none left, the labels are spread out again. So
//! labels change, and they stay private: code stores `Time` handles, which
//! never change.

use std::cmp::Ordering;

use caaqi_core_scoped::context::get_or_default;
use slotmap::SlotMap;

slotmap::new_key_type! {
    /// The slot of a point in the timeline.
    struct TimeKey;
}

/// A point on the timeline. `Copy`, and cheap to compare.
///
/// A `Time` doesn't borrow the context, so nothing keeps its point alive:
/// the methods panic if the point was [removed](Self::remove), and each has
/// a `try_` version returning [`UnknownTime`] instead. Check with
/// [`exists`](Self::exists) when that may have happened. A `Time` is only
/// valid in the context that created it: in another, it's unsupported and
/// may refer to an unrelated point.
///
/// It's `PartialOrd` by timeline order: `a < b` if `a` comes first. It isn't
/// `Ord`, because the order can change: [moving](Self::move_after) a point
/// changes where it sorts, so a sorted collection holding it must re-sort
/// it. Comparing panics if either point was removed;
/// [`try_time_cmp`](Self::try_time_cmp) returns the error instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Time(TimeKey);

impl PartialOrd for Time {
    fn partial_cmp(&self, other: &Time) -> Option<Ordering> {
        Some(self.time_cmp(*other))
    }
}

/// The point isn't on the timeline, because it was removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("{0:?} isn't on the timeline")]
pub struct UnknownTime(pub Time);

/// A point can't be moved relative to itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("can't move {0:?} relative to itself")]
pub struct SameTime(pub Time);

/// Why [`Time::try_move_after`] or [`Time::try_move_before`] failed.
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

/// The timeline resource: a doubly linked list of points in timeline order.
/// Everything public goes through [`Time`] and this module's functions.
#[derive(Default)]
struct Timeline {
    entries: SlotMap<TimeKey, Entry>,
    head: Option<TimeKey>,
    tail: Option<TimeKey>,
    /// How many times the labels were respread, so tests can check they were.
    #[cfg(test)]
    respreads: usize,
}

impl Timeline {
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

    fn insert_after(&mut self, time: Time) -> Result<Time, UnknownTime> {
        let next = self.entry(time)?.next;
        let key = self.create();
        self.link(key, Some(time.0), next);
        Ok(Time(key))
    }

    fn insert_before(&mut self, time: Time) -> Result<Time, UnknownTime> {
        let prev = self.entry(time)?.prev;
        let key = self.create();
        self.link(key, prev, Some(time.0));
        Ok(Time(key))
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

    fn move_after(&mut self, time: Time, anchor: Time) -> Result<(), MoveTimeError> {
        self.unlink_to_move(time, anchor)?;
        let next = self.entries[anchor.0].next;
        self.link(time.0, Some(anchor.0), next);
        Ok(())
    }

    fn move_before(&mut self, time: Time, anchor: Time) -> Result<(), MoveTimeError> {
        self.unlink_to_move(time, anchor)?;
        let prev = self.entries[anchor.0].prev;
        self.link(time.0, prev, Some(anchor.0));
        Ok(())
    }

    fn remove(&mut self, time: Time) -> Result<(), UnknownTime> {
        self.entry(time)?;
        self.unlink(time.0);
        self.entries.remove(time.0);
        Ok(())
    }

    fn cmp(&self, a: Time, b: Time) -> Result<Ordering, UnknownTime> {
        Ok(self.entry(a)?.label.cmp(&self.entry(b)?.label))
    }

    fn times(&self) -> Vec<Time> {
        let mut times = Vec::with_capacity(self.entries.len());
        let mut cursor = self.head;
        while let Some(key) = cursor {
            times.push(Time(key));
            cursor = self.entries[key].next;
        }
        times
    }
}

/// Calls `f` with the timeline.
fn timeline<R>(f: impl FnOnce(&mut Timeline) -> R) -> R {
    get_or_default(f)
}

/// Adds a point at the end of the timeline.
pub fn push_back() -> Time {
    timeline(|timeline| {
        let key = timeline.create();
        let tail = timeline.tail;
        timeline.link(key, tail, None);
        Time(key)
    })
}

/// Adds a point at the start of the timeline.
pub fn push_front() -> Time {
    timeline(|timeline| {
        let key = timeline.create();
        let head = timeline.head;
        timeline.link(key, None, head);
        Time(key)
    })
}

/// The first point, or `None` if the timeline is empty.
pub fn first() -> Option<Time> {
    timeline(|timeline| timeline.head.map(Time))
}

/// The last point, or `None` if the timeline is empty.
pub fn last() -> Option<Time> {
    timeline(|timeline| timeline.tail.map(Time))
}

/// How many points the timeline has.
pub fn len() -> usize {
    timeline(|timeline| timeline.entries.len())
}

/// Whether the timeline has no points.
pub fn is_empty() -> bool {
    len() == 0
}

/// Every point, in timeline order. They're collected first, so the timeline
/// can change while going through them.
pub fn times() -> Vec<Time> {
    timeline(|timeline| timeline.times())
}

impl Time {
    /// Whether the point is on the timeline. When it isn't, because it was
    /// removed, the other methods panic.
    pub fn exists(self) -> bool {
        timeline(|timeline| timeline.entries.contains_key(self.0))
    }

    /// Like [`try_insert_after`](Self::try_insert_after), but panics on
    /// error.
    #[track_caller]
    pub fn insert_after(self) -> Time {
        self.try_insert_after()
            .unwrap_or_else(|error| panic!("{error}"))
    }

    /// Adds a point right after this one.
    pub fn try_insert_after(self) -> Result<Time, UnknownTime> {
        timeline(|timeline| timeline.insert_after(self))
    }

    /// Like [`try_insert_before`](Self::try_insert_before), but panics on
    /// error.
    #[track_caller]
    pub fn insert_before(self) -> Time {
        self.try_insert_before()
            .unwrap_or_else(|error| panic!("{error}"))
    }

    /// Adds a point right before this one.
    pub fn try_insert_before(self) -> Result<Time, UnknownTime> {
        timeline(|timeline| timeline.insert_before(self))
    }

    /// Like [`try_move_after`](Self::try_move_after), but panics on error.
    #[track_caller]
    pub fn move_after(self, anchor: Time) {
        self.try_move_after(anchor)
            .unwrap_or_else(|error| panic!("{error}"));
    }

    /// Moves this point to right after `anchor`. It keeps its handle, and
    /// sorts in its new place from now on.
    ///
    /// Fails if either point isn't on the timeline, or with [`SameTime`] if
    /// `anchor` is this point.
    pub fn try_move_after(self, anchor: Time) -> Result<(), MoveTimeError> {
        timeline(|timeline| timeline.move_after(self, anchor))
    }

    /// Like [`try_move_before`](Self::try_move_before), but panics on error.
    #[track_caller]
    pub fn move_before(self, anchor: Time) {
        self.try_move_before(anchor)
            .unwrap_or_else(|error| panic!("{error}"));
    }

    /// Moves this point to right before `anchor`. It keeps its handle, and
    /// sorts in its new place from now on.
    ///
    /// Fails if either point isn't on the timeline, or with [`SameTime`] if
    /// `anchor` is this point.
    pub fn try_move_before(self, anchor: Time) -> Result<(), MoveTimeError> {
        timeline(|timeline| timeline.move_before(self, anchor))
    }

    /// Like [`try_remove`](Self::try_remove), but panics on error.
    #[track_caller]
    pub fn remove(self) {
        self.try_remove().unwrap_or_else(|error| panic!("{error}"));
    }

    /// Removes the point from the timeline. From then on, the handle is
    /// unknown.
    pub fn try_remove(self) -> Result<(), UnknownTime> {
        timeline(|timeline| timeline.remove(self))
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
        timeline(|timeline| timeline.cmp(self, other))
    }

    /// Like [`try_next`](Self::try_next), but panics on error.
    #[track_caller]
    pub fn next(self) -> Option<Time> {
        self.try_next().unwrap_or_else(|error| panic!("{error}"))
    }

    /// The point right after this one, or `None` if this is the last.
    pub fn try_next(self) -> Result<Option<Time>, UnknownTime> {
        timeline(|timeline| Ok(timeline.entry(self)?.next.map(Time)))
    }

    /// Like [`try_prev`](Self::try_prev), but panics on error.
    #[track_caller]
    pub fn prev(self) -> Option<Time> {
        self.try_prev().unwrap_or_else(|error| panic!("{error}"))
    }

    /// The point right before this one, or `None` if this is the first.
    pub fn try_prev(self) -> Result<Option<Time>, UnknownTime> {
        timeline(|timeline| Ok(timeline.entry(self)?.prev.map(Time)))
    }
}

#[cfg(test)]
#[path = "time_tests.rs"]
mod tests;
