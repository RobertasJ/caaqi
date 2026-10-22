use caaqi_core_scoped::context::{Context, scope};

use super::*;

/// The point's private label.
fn label(time: Time) -> u64 {
    timeline(|timeline| timeline.entries[time.0].label)
}

/// How many times the labels were respread.
fn respreads() -> usize {
    timeline(|timeline| timeline.respreads)
}

/// Checks the list from both ends: labels strictly increase, `prev` and
/// `next` mirror each other, `head` and `tail` are the ends, and every point
/// is linked.
fn check_invariants() {
    timeline(|timeline| {
        let mut count = 0;
        let mut prev: Option<TimeKey> = None;
        let mut cursor = timeline.head;
        while let Some(key) = cursor {
            let entry = &timeline.entries[key];
            assert_eq!(entry.prev, prev, "`prev` mirrors `next`");
            if let Some(prev) = prev {
                assert!(
                    timeline.entries[prev].label < entry.label,
                    "labels strictly increase"
                );
            }
            count += 1;
            prev = Some(key);
            cursor = entry.next;
        }
        assert_eq!(timeline.tail, prev, "`tail` is the last point");
        assert_eq!(count, timeline.entries.len(), "every point is linked");
    });
}

#[test]
fn a_new_timeline_is_empty() {
    scope(&mut Context::new(), || {
        assert!(is_empty());
        assert_eq!(len(), 0);
        assert_eq!(first(), None);
        assert_eq!(last(), None);
        assert_eq!(times(), []);
    });
}

#[test]
fn push_back_and_push_front_add_at_the_ends() {
    scope(&mut Context::new(), || {
        let a = push_back();
        let b = push_back();
        let c = push_front();

        assert_eq!(times(), [c, a, b]);
        assert_eq!(first(), Some(c));
        assert_eq!(last(), Some(b));
        assert_eq!(len(), 3);
        assert!(!is_empty());
        check_invariants();
    });
}

#[test]
fn the_first_point_is_in_the_middle_of_the_range() {
    scope(&mut Context::new(), || {
        assert_eq!(label(push_back()), u64::MAX / 2);
    });
}

#[test]
fn insert_after_and_before_add_next_to_the_point() {
    scope(&mut Context::new(), || {
        let a = push_back();
        let c = push_back();
        let b = a.insert_after();
        let d = c.insert_after();
        let start = a.insert_before();
        let between = c.insert_before();

        assert_eq!(times(), [start, a, b, between, c, d]);
        assert_eq!(first(), Some(start));
        assert_eq!(last(), Some(d));
        check_invariants();
    });
}

#[test]
fn next_and_prev_step_along_the_timeline() {
    scope(&mut Context::new(), || {
        let a = push_back();
        let b = push_back();
        let c = push_back();

        assert_eq!(a.prev(), None);
        assert_eq!(a.next(), Some(b));
        assert_eq!(b.prev(), Some(a));
        assert_eq!(b.next(), Some(c));
        assert_eq!(c.prev(), Some(b));
        assert_eq!(c.next(), None);
    });
}

#[test]
fn time_cmp_and_partial_ord_follow_the_timeline() {
    scope(&mut Context::new(), || {
        let b = push_back();
        let a = push_front();

        assert_eq!(a.time_cmp(b), Ordering::Less);
        assert_eq!(b.time_cmp(a), Ordering::Greater);
        assert_eq!(a.time_cmp(a), Ordering::Equal);
        assert!(a < b);
        assert_eq!(a.partial_cmp(&a), Some(Ordering::Equal));
    });
}

#[test]
fn moving_keeps_the_handle_and_changes_the_order() {
    scope(&mut Context::new(), || {
        let a = push_back();
        let b = push_back();
        let c = push_back();

        a.move_after(c);
        assert_eq!(times(), [b, c, a]);
        assert!(c < a);
        assert_eq!(last(), Some(a));

        a.move_before(b);
        assert_eq!(times(), [a, b, c]);
        assert_eq!(first(), Some(a));

        c.move_before(b);
        assert_eq!(times(), [a, c, b]);
        assert_eq!(last(), Some(b));
        check_invariants();
    });
}

#[test]
fn moving_to_where_a_point_already_is_keeps_the_order() {
    scope(&mut Context::new(), || {
        let a = push_back();
        let b = push_back();

        b.move_after(a);
        a.move_before(b);
        assert_eq!(times(), [a, b]);
        check_invariants();
    });
}

#[test]
fn moving_relative_to_itself_fails() {
    scope(&mut Context::new(), || {
        let a = push_back();
        let b = push_back();

        assert_eq!(a.try_move_after(a), Err(SameTime(a).into()));
        assert_eq!(a.try_move_before(a), Err(SameTime(a).into()));
        assert_eq!(times(), [a, b]);
    });
}

#[test]
fn moving_fails_with_whichever_point_is_unknown() {
    scope(&mut Context::new(), || {
        let a = push_back();
        let removed = push_back();
        removed.remove();

        assert_eq!(removed.try_move_after(a), Err(UnknownTime(removed).into()));
        assert_eq!(a.try_move_after(removed), Err(UnknownTime(removed).into()));
        assert_eq!(removed.try_move_before(a), Err(UnknownTime(removed).into()));
        assert_eq!(a.try_move_before(removed), Err(UnknownTime(removed).into()));
        assert_eq!(times(), [a]);
    });
}

#[test]
fn remove_takes_the_point_off_the_timeline() {
    scope(&mut Context::new(), || {
        let a = push_back();
        let b = push_back();
        let c = push_back();

        b.remove();
        assert!(!b.exists());
        assert!(a.exists());
        assert_eq!(times(), [a, c]);
        assert_eq!(a.next(), Some(c));
        assert_eq!(c.prev(), Some(a));
        assert_eq!(len(), 2);

        a.remove();
        c.remove();
        assert!(is_empty());
        assert_eq!(first(), None);
        assert_eq!(last(), None);
        check_invariants();

        assert_eq!(b.try_remove(), Err(UnknownTime(b)));
    });
}

#[test]
fn removing_the_ends_updates_first_and_last() {
    scope(&mut Context::new(), || {
        let a = push_back();
        let b = push_back();
        let c = push_back();

        a.remove();
        assert_eq!(first(), Some(b));
        c.remove();
        assert_eq!(last(), Some(b));
        assert_eq!(b.prev(), None);
        assert_eq!(b.next(), None);
        check_invariants();
    });
}

#[test]
fn a_new_point_never_reuses_a_removed_handle() {
    scope(&mut Context::new(), || {
        let removed = push_back();
        removed.remove();
        let new = push_back();

        assert_ne!(new, removed);
        assert!(!removed.exists());
    });
}

#[test]
fn try_methods_fail_on_a_removed_point() {
    scope(&mut Context::new(), || {
        let a = push_back();
        let removed = push_back();
        removed.remove();

        assert_eq!(removed.try_insert_after(), Err(UnknownTime(removed)));
        assert_eq!(removed.try_insert_before(), Err(UnknownTime(removed)));
        assert_eq!(removed.try_next(), Err(UnknownTime(removed)));
        assert_eq!(removed.try_prev(), Err(UnknownTime(removed)));
        assert_eq!(removed.try_time_cmp(a), Err(UnknownTime(removed)));
        assert_eq!(a.try_time_cmp(removed), Err(UnknownTime(removed)));
        assert_eq!(times(), [a]);
    });
}

/// Runs `f` on a removed point, in a fresh context.
fn with_removed(f: impl FnOnce(Time, Time)) {
    scope(&mut Context::new(), || {
        let a = push_back();
        let removed = push_back();
        removed.remove();
        f(a, removed);
    });
}

#[test]
#[should_panic(expected = "isn't on the timeline")]
fn insert_after_panics_on_a_removed_point() {
    with_removed(|_, removed| {
        removed.insert_after();
    });
}

#[test]
#[should_panic(expected = "isn't on the timeline")]
fn insert_before_panics_on_a_removed_point() {
    with_removed(|_, removed| {
        removed.insert_before();
    });
}

#[test]
#[should_panic(expected = "isn't on the timeline")]
fn move_after_panics_on_a_removed_anchor() {
    with_removed(|a, removed| a.move_after(removed));
}

#[test]
#[should_panic(expected = "isn't on the timeline")]
fn move_before_panics_on_a_removed_point() {
    with_removed(|a, removed| removed.move_before(a));
}

#[test]
#[should_panic(expected = "relative to itself")]
fn move_after_panics_relative_to_itself() {
    scope(&mut Context::new(), || {
        let a = push_back();
        a.move_after(a);
    });
}

#[test]
#[should_panic(expected = "isn't on the timeline")]
fn remove_panics_on_a_removed_point() {
    with_removed(|_, removed| removed.remove());
}

#[test]
#[should_panic(expected = "isn't on the timeline")]
fn time_cmp_panics_on_a_removed_point() {
    with_removed(|a, removed| {
        a.time_cmp(removed);
    });
}

#[test]
#[should_panic(expected = "isn't on the timeline")]
fn comparing_panics_on_a_removed_point() {
    with_removed(|a, removed| {
        let _ = a < removed;
    });
}

#[test]
#[should_panic(expected = "isn't on the timeline")]
fn next_panics_on_a_removed_point() {
    with_removed(|_, removed| {
        removed.next();
    });
}

#[test]
#[should_panic(expected = "isn't on the timeline")]
fn prev_panics_on_a_removed_point() {
    with_removed(|_, removed| {
        removed.prev();
    });
}

#[test]
fn inserting_into_the_same_gap_respreads_labels() {
    scope(&mut Context::new(), || {
        let anchor = push_back();
        let first_inserted = anchor.insert_after();
        let first_label = label(first_inserted);

        // Each point goes right after `anchor`, halving the same gap.
        let mut inserted = vec![first_inserted];
        for _ in 1..1000 {
            inserted.push(anchor.insert_after());
        }

        assert!(respreads() > 0);
        assert_ne!(label(first_inserted), first_label, "labels were respread");

        let mut expected = vec![anchor];
        expected.extend(inserted.iter().rev());
        assert_eq!(times(), expected);
        assert!(inserted.iter().all(|time| time.exists()));
        assert!(expected.windows(2).all(|pair| pair[0] < pair[1]));
        check_invariants();
    });
}

#[test]
fn pushing_at_the_ends_respreads_labels() {
    scope(&mut Context::new(), || {
        // Each push halves the gap to 0 or to `u64::MAX`.
        let backs: Vec<_> = (0..1000).map(|_| push_back()).collect();
        let fronts: Vec<_> = (0..1000).map(|_| push_front()).collect();

        assert!(respreads() > 0);
        let mut expected: Vec<_> = fronts.into_iter().rev().collect();
        expected.extend(backs);
        assert_eq!(times(), expected);
        check_invariants();
    });
}

/// A xorshift generator with a fixed seed, so the randomized test is
/// reproducible.
struct Rng(u64);

impl Rng {
    /// A number in `0..len`. `len` must not be 0.
    fn below(&mut self, len: usize) -> usize {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        (x % len as u64) as usize
    }

    /// An index into `model`, often 0, so the same gaps fill up and
    /// respreading happens.
    fn index(&mut self, model: &[Time]) -> usize {
        if self.below(2) == 0 {
            0
        } else {
            self.below(model.len())
        }
    }
}

/// Moves `model[from]` next to `model[anchor]` in the model, as
/// `move_after` (`after == true`) or `move_before` do.
fn move_in_model(model: &mut Vec<Time>, from: usize, anchor: usize, after: bool) {
    let anchor = model[anchor];
    let time = model.remove(from);
    let position = model.iter().position(|&t| t == anchor).unwrap();
    model.insert(position + usize::from(after), time);
}

#[test]
fn random_operations_match_a_vec_model() {
    const MAX_LEN: usize = 50;

    scope(&mut Context::new(), || {
        let mut rng = Rng(0x2545_f491_4f6c_dd1d);
        let mut model: Vec<Time> = Vec::new();
        let mut removed: Vec<Time> = Vec::new();

        for _ in 0..2000 {
            let mut op = rng.below(7);
            if model.is_empty() {
                op = rng.below(2);
            } else if model.len() >= MAX_LEN && op < 4 {
                op = 6;
            }

            match op {
                0 => model.push(push_back()),
                1 => model.insert(0, push_front()),
                2 => {
                    let i = rng.index(&model);
                    model.insert(i + 1, model[i].insert_after());
                }
                3 => {
                    let i = rng.index(&model);
                    model.insert(i, model[i].insert_before());
                }
                4 | 5 => {
                    let after = op == 4;
                    let from = rng.below(model.len());
                    let anchor = rng.index(&model);
                    let (time, anchor_time) = (model[from], model[anchor]);
                    let result = if after {
                        time.try_move_after(anchor_time)
                    } else {
                        time.try_move_before(anchor_time)
                    };
                    if from == anchor {
                        assert_eq!(result, Err(SameTime(time).into()));
                    } else {
                        result.unwrap();
                        move_in_model(&mut model, from, anchor, after);
                    }
                }
                _ => {
                    let time = model.remove(rng.below(model.len()));
                    time.remove();
                    removed.push(time);
                }
            }

            check_invariants();
            assert_eq!(times(), model);
            assert_eq!(len(), model.len());
            assert_eq!(first(), model.first().copied());
            assert_eq!(last(), model.last().copied());
            for (i, &a) in model.iter().enumerate() {
                assert_eq!(a.prev(), i.checked_sub(1).map(|j| model[j]));
                assert_eq!(a.next(), model.get(i + 1).copied());
                for (j, &b) in model.iter().enumerate() {
                    assert_eq!(a.time_cmp(b), i.cmp(&j));
                }
            }
        }

        assert!(respreads() > 0, "the test should exercise respreading");
        for time in removed {
            assert!(!time.exists());
        }
    });
}
