use super::*;

/// The point's private label.
fn label(timeline: &Timeline, time: Time) -> u64 {
    timeline.entries[time.0].label
}

/// The points in timeline order, collected.
fn times(timeline: &Timeline) -> Vec<Time> {
    timeline.times().collect()
}

/// Whether `a` comes before `b`.
fn before(timeline: &Timeline, a: Time, b: Time) -> bool {
    timeline.time_cmp(a, b).unwrap() == Ordering::Less
}

/// Checks the list from both ends: labels strictly increase, `prev` and
/// `next` mirror each other, `head` and `tail` are the ends, and every point
/// is linked.
fn check_invariants(timeline: &Timeline) {
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
}

/// A timeline with a point `a`, and a point that was removed.
fn with_removed() -> (Timeline, Time, Time) {
    let mut timeline = Timeline::new();
    let a = timeline.push_back();
    let removed = timeline.push_back();
    timeline.remove(removed).unwrap();
    (timeline, a, removed)
}

#[test]
fn a_new_timeline_is_empty() {
    let timeline = Timeline::new();
    assert!(timeline.is_empty());
    assert_eq!(timeline.len(), 0);
    assert_eq!(timeline.first(), None);
    assert_eq!(timeline.last(), None);
    assert_eq!(times(&timeline), []);
}

#[test]
fn push_back_and_push_front_add_at_the_ends() {
    let mut timeline = Timeline::new();
    let a = timeline.push_back();
    let b = timeline.push_back();
    let c = timeline.push_front();

    assert_eq!(times(&timeline), [c, a, b]);
    assert_eq!(timeline.first(), Some(c));
    assert_eq!(timeline.last(), Some(b));
    assert_eq!(timeline.len(), 3);
    assert!(!timeline.is_empty());
    check_invariants(&timeline);
}

#[test]
fn the_first_point_is_in_the_middle_of_the_range() {
    let mut timeline = Timeline::new();
    let a = timeline.push_back();
    assert_eq!(label(&timeline, a), u64::MAX / 2);
}

#[test]
fn insert_after_and_before_add_next_to_the_point() {
    let mut timeline = Timeline::new();
    let a = timeline.push_back();
    let c = timeline.push_back();
    let b = timeline.insert_after(a).unwrap();
    let d = timeline.insert_after(c).unwrap();
    let start = timeline.insert_before(a).unwrap();
    let between = timeline.insert_before(c).unwrap();

    assert_eq!(times(&timeline), [start, a, b, between, c, d]);
    assert_eq!(timeline.first(), Some(start));
    assert_eq!(timeline.last(), Some(d));
    check_invariants(&timeline);
}

#[test]
fn next_and_prev_step_along_the_timeline() {
    let mut timeline = Timeline::new();
    let a = timeline.push_back();
    let b = timeline.push_back();
    let c = timeline.push_back();

    assert_eq!(timeline.prev(a), Ok(None));
    assert_eq!(timeline.next(a), Ok(Some(b)));
    assert_eq!(timeline.prev(b), Ok(Some(a)));
    assert_eq!(timeline.next(b), Ok(Some(c)));
    assert_eq!(timeline.prev(c), Ok(Some(b)));
    assert_eq!(timeline.next(c), Ok(None));
}

#[test]
fn time_cmp_follows_the_timeline() {
    let mut timeline = Timeline::new();
    let b = timeline.push_back();
    let a = timeline.push_front();

    assert_eq!(timeline.time_cmp(a, b), Ok(Ordering::Less));
    assert_eq!(timeline.time_cmp(b, a), Ok(Ordering::Greater));
    assert_eq!(timeline.time_cmp(a, a), Ok(Ordering::Equal));
}

#[test]
fn moving_keeps_the_handle_and_changes_the_order() {
    let mut timeline = Timeline::new();
    let a = timeline.push_back();
    let b = timeline.push_back();
    let c = timeline.push_back();

    timeline.move_after(a, c).unwrap();
    assert_eq!(times(&timeline), [b, c, a]);
    assert!(before(&timeline, c, a));
    assert_eq!(timeline.last(), Some(a));

    timeline.move_before(a, b).unwrap();
    assert_eq!(times(&timeline), [a, b, c]);
    assert_eq!(timeline.first(), Some(a));

    timeline.move_before(c, b).unwrap();
    assert_eq!(times(&timeline), [a, c, b]);
    assert_eq!(timeline.last(), Some(b));
    check_invariants(&timeline);
}

#[test]
fn moving_to_where_a_point_already_is_keeps_the_order() {
    let mut timeline = Timeline::new();
    let a = timeline.push_back();
    let b = timeline.push_back();

    timeline.move_after(b, a).unwrap();
    timeline.move_before(a, b).unwrap();
    assert_eq!(times(&timeline), [a, b]);
    check_invariants(&timeline);
}

#[test]
fn moving_relative_to_itself_fails() {
    let mut timeline = Timeline::new();
    let a = timeline.push_back();
    let b = timeline.push_back();

    assert_eq!(timeline.move_after(a, a), Err(SameTime(a).into()));
    assert_eq!(timeline.move_before(a, a), Err(SameTime(a).into()));
    assert_eq!(times(&timeline), [a, b]);
}

#[test]
fn moving_fails_with_whichever_point_is_unknown() {
    let (mut timeline, a, removed) = with_removed();

    let unknown = Err(UnknownTime(removed).into());
    assert_eq!(timeline.move_after(removed, a), unknown);
    assert_eq!(timeline.move_after(a, removed), unknown);
    assert_eq!(timeline.move_before(removed, a), unknown);
    assert_eq!(timeline.move_before(a, removed), unknown);
    assert_eq!(times(&timeline), [a]);
}

#[test]
fn remove_takes_the_point_off_the_timeline() {
    let mut timeline = Timeline::new();
    let a = timeline.push_back();
    let b = timeline.push_back();
    let c = timeline.push_back();

    timeline.remove(b).unwrap();
    assert!(!timeline.contains(b));
    assert!(timeline.contains(a));
    assert_eq!(times(&timeline), [a, c]);
    assert_eq!(timeline.next(a), Ok(Some(c)));
    assert_eq!(timeline.prev(c), Ok(Some(a)));
    assert_eq!(timeline.len(), 2);

    timeline.remove(a).unwrap();
    timeline.remove(c).unwrap();
    assert!(timeline.is_empty());
    assert_eq!(timeline.first(), None);
    assert_eq!(timeline.last(), None);
    check_invariants(&timeline);

    assert_eq!(timeline.remove(b), Err(UnknownTime(b)));
}

#[test]
fn removing_the_ends_updates_first_and_last() {
    let mut timeline = Timeline::new();
    let a = timeline.push_back();
    let b = timeline.push_back();
    let c = timeline.push_back();

    timeline.remove(a).unwrap();
    assert_eq!(timeline.first(), Some(b));
    timeline.remove(c).unwrap();
    assert_eq!(timeline.last(), Some(b));
    assert_eq!(timeline.prev(b), Ok(None));
    assert_eq!(timeline.next(b), Ok(None));
    check_invariants(&timeline);
}

#[test]
fn a_new_point_never_reuses_a_removed_handle() {
    let (mut timeline, _, removed) = with_removed();
    let new = timeline.push_back();

    assert_ne!(new, removed);
    assert!(!timeline.contains(removed));
}

#[test]
fn methods_fail_on_a_removed_point() {
    let (mut timeline, a, removed) = with_removed();

    let unknown = UnknownTime(removed);
    assert_eq!(timeline.insert_after(removed), Err(unknown));
    assert_eq!(timeline.insert_before(removed), Err(unknown));
    assert_eq!(timeline.next(removed), Err(unknown));
    assert_eq!(timeline.prev(removed), Err(unknown));
    assert_eq!(timeline.time_cmp(removed, a), Err(unknown));
    assert_eq!(timeline.time_cmp(a, removed), Err(unknown));
    assert_eq!(timeline.remove(removed), Err(unknown));
    assert_eq!(times(&timeline), [a]);
}

#[test]
fn inserting_into_the_same_gap_respreads_labels() {
    let mut timeline = Timeline::new();
    let anchor = timeline.push_back();
    let first_inserted = timeline.insert_after(anchor).unwrap();
    let first_label = label(&timeline, first_inserted);

    // Each point goes right after `anchor`, halving the same gap.
    let mut inserted = vec![first_inserted];
    for _ in 1..1000 {
        inserted.push(timeline.insert_after(anchor).unwrap());
    }

    assert!(timeline.respreads > 0);
    assert_ne!(
        label(&timeline, first_inserted),
        first_label,
        "labels were respread"
    );

    let mut expected = vec![anchor];
    expected.extend(inserted.iter().rev());
    assert_eq!(times(&timeline), expected);
    assert!(inserted.iter().all(|&time| timeline.contains(time)));
    assert!(
        expected
            .windows(2)
            .all(|pair| before(&timeline, pair[0], pair[1]))
    );
    check_invariants(&timeline);
}

#[test]
fn pushing_at_the_ends_respreads_labels() {
    let mut timeline = Timeline::new();
    // Each push halves the gap to 0 or to `u64::MAX`.
    let backs: Vec<_> = (0..1000).map(|_| timeline.push_back()).collect();
    let fronts: Vec<_> = (0..1000).map(|_| timeline.push_front()).collect();

    assert!(timeline.respreads > 0);
    let mut expected: Vec<_> = fronts.into_iter().rev().collect();
    expected.extend(backs);
    assert_eq!(times(&timeline), expected);
    check_invariants(&timeline);
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

    let mut timeline = Timeline::new();
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
            0 => model.push(timeline.push_back()),
            1 => model.insert(0, timeline.push_front()),
            2 => {
                let i = rng.index(&model);
                model.insert(i + 1, timeline.insert_after(model[i]).unwrap());
            }
            3 => {
                let i = rng.index(&model);
                model.insert(i, timeline.insert_before(model[i]).unwrap());
            }
            4 | 5 => {
                let after = op == 4;
                let from = rng.below(model.len());
                let anchor = rng.index(&model);
                let (time, anchor_time) = (model[from], model[anchor]);
                let result = if after {
                    timeline.move_after(time, anchor_time)
                } else {
                    timeline.move_before(time, anchor_time)
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
                timeline.remove(time).unwrap();
                removed.push(time);
            }
        }

        check_invariants(&timeline);
        assert_eq!(times(&timeline), model);
        assert_eq!(timeline.len(), model.len());
        assert_eq!(timeline.first(), model.first().copied());
        assert_eq!(timeline.last(), model.last().copied());
        for (i, &a) in model.iter().enumerate() {
            assert_eq!(timeline.prev(a), Ok(i.checked_sub(1).map(|j| model[j])));
            assert_eq!(timeline.next(a), Ok(model.get(i + 1).copied()));
            for (j, &b) in model.iter().enumerate() {
                assert_eq!(timeline.time_cmp(a, b), Ok(i.cmp(&j)));
            }
        }
    }

    assert!(
        timeline.respreads > 0,
        "the test should exercise respreading"
    );
    for time in removed {
        assert!(!timeline.contains(time));
    }
}
