use super::*;
use crate::{
    machine::{Machine, new_machine, scope},
    value::create_value,
};

/// Runs `program` until it settles, and returns the machine and the result.
fn settle<R: Clone + 'static>(program: impl Fn() -> R + 'static) -> (Machine, R) {
    let (mut machine, result) = new_machine(program);
    machine.run().unwrap();
    let output = scope(&mut machine, || result.get());
    (machine, output)
}

/// A point from a machine with more points than the ones in the tests, so
/// it isn't on their timelines.
fn foreign_time() -> Time {
    settle(|| {
        for _ in 0..100 {
            now();
        }
        now()
    })
    .1
}

#[test]
fn now_is_an_event_at_its_own_point() {
    let (mut machine, (a, b, between)) = settle(|| {
        let a = now();
        let between = len();
        (a, now(), between)
    });

    scope(&mut machine, || {
        assert!(a < b);
        assert_eq!(a.next(), Some(b));
        // The start, then `a`, and nothing else yet when `len` ran.
        assert_eq!(between, 2);
    });
}

#[test]
fn now_outside_a_run_fails() {
    let (mut machine, _) = settle(|| ());
    scope(&mut machine, || assert_eq!(try_now(), Err(NotRunning)));
}

#[test]
#[should_panic(expected = "isn't running")]
fn now_panics_outside_a_run() {
    let (mut machine, _) = settle(|| ());
    scope(&mut machine, || {
        now();
    });
}

#[test]
fn the_timeline_follows_the_run() {
    let (mut machine, (created, mark)) = settle(|| {
        let created = create_value(0).history()[0].time;
        (created, now())
    });

    scope(&mut machine, || {
        let all = times();
        // The start, the creation, the history, `now` and the result.
        assert_eq!(all.len(), 5);
        assert_eq!(len(), 5);
        assert!(!is_empty());
        assert_eq!(first(), Some(all[0]));
        assert_eq!(last(), Some(all[4]));
        assert_eq!(all[1], created);
        assert_eq!(all[3], mark);
        assert!(mark.exists());
    });
}

#[test]
fn time_cmp_and_partial_ord_follow_the_timeline() {
    let (mut machine, (a, b)) = settle(|| (now(), now()));

    scope(&mut machine, || {
        assert_eq!(a.time_cmp(b), Ordering::Less);
        assert_eq!(b.time_cmp(a), Ordering::Greater);
        assert_eq!(a.time_cmp(a), Ordering::Equal);
        assert!(a < b);
        assert_eq!(a.partial_cmp(&a), Some(Ordering::Equal));
    });
}

#[test]
fn next_and_prev_step_along_the_timeline() {
    let (mut machine, (a, b)) = settle(|| (now(), now()));

    scope(&mut machine, || {
        let start = first().unwrap();
        assert_eq!(start.prev(), None);
        assert_eq!(a.prev(), Some(start));
        assert_eq!(a.next(), Some(b));
        assert_eq!(b.prev(), Some(a));
        let result = last().unwrap();
        assert_eq!(b.next(), Some(result));
        assert_eq!(result.next(), None);
    });
}

#[test]
fn try_methods_fail_on_an_unknown_point() {
    let unknown = foreign_time();
    let (mut machine, known) = settle(now);
    let error = UnknownTime(unknown.into());

    scope(&mut machine, || {
        assert!(!unknown.exists());
        assert_eq!(unknown.try_time_cmp(known), Err(error));
        assert_eq!(known.try_time_cmp(unknown), Err(error));
        assert_eq!(unknown.try_next(), Err(error));
        assert_eq!(unknown.try_prev(), Err(error));
    });
}

#[test]
#[should_panic(expected = "isn't on the timeline")]
fn comparing_panics_on_an_unknown_point() {
    let unknown = foreign_time();
    let (mut machine, known) = settle(now);
    scope(&mut machine, || {
        let _ = known < unknown;
    });
}

#[test]
#[should_panic(expected = "isn't on the timeline")]
fn next_panics_on_an_unknown_point() {
    let unknown = foreign_time();
    let (mut machine, _) = settle(|| ());
    scope(&mut machine, || {
        unknown.next();
    });
}
