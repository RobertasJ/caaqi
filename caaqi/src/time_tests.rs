use caaqi_core::machine::Machine;

use super::*;
use crate::{machine::scope, trace::run_step_with_id, value::create_value};

/// A point from a machine with more points than the one in scope, so it
/// isn't on that one's timeline.
fn foreign_time() -> Time {
    let mut other = Machine::new();
    scope(&mut other, || {
        for _ in 0..3 {
            create_value(0);
        }
        last().unwrap()
    })
}

#[test]
fn a_new_timeline_is_empty() {
    scope(&mut Machine::new(), || {
        assert!(is_empty());
        assert_eq!(len(), 0);
        assert_eq!(first(), None);
        assert_eq!(last(), None);
        assert_eq!(times(), []);
    });
}

#[test]
fn the_timeline_follows_the_run() {
    scope(&mut Machine::new(), || {
        let (step, result) = run_step_with_id(|| ());
        let (start, end) = (step.start(), step.end().unwrap());
        let written = result.history()[0].time;

        assert_eq!(times(), [start, written, end]);
        assert_eq!(first(), Some(start));
        assert_eq!(last(), Some(end));
        assert_eq!(len(), 3);
        assert!(!is_empty());
        assert!(start.exists());
    });
}

#[test]
fn time_cmp_and_partial_ord_follow_the_timeline() {
    scope(&mut Machine::new(), || {
        let (step, _) = run_step_with_id(|| ());
        let (a, b) = (step.start(), step.end().unwrap());

        assert_eq!(a.time_cmp(b), Ordering::Less);
        assert_eq!(b.time_cmp(a), Ordering::Greater);
        assert_eq!(a.time_cmp(a), Ordering::Equal);
        assert!(a < b);
        assert_eq!(a.partial_cmp(&a), Some(Ordering::Equal));
    });
}

#[test]
fn next_and_prev_step_along_the_timeline() {
    scope(&mut Machine::new(), || {
        let (step, result) = run_step_with_id(|| ());
        let (a, b, c) = (step.start(), result.history()[0].time, step.end().unwrap());

        assert_eq!(a.prev(), None);
        assert_eq!(a.next(), Some(b));
        assert_eq!(b.prev(), Some(a));
        assert_eq!(b.next(), Some(c));
        assert_eq!(c.next(), None);
    });
}

#[test]
fn try_methods_fail_on_an_unknown_point() {
    let unknown = foreign_time();
    scope(&mut Machine::new(), || {
        let known = create_value(0).history()[0].time;
        let error = UnknownTime(unknown.into());

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
    scope(&mut Machine::new(), || {
        let known = create_value(0).history()[0].time;
        let _ = known < unknown;
    });
}

#[test]
#[should_panic(expected = "isn't on the timeline")]
fn next_panics_on_an_unknown_point() {
    let unknown = foreign_time();
    scope(&mut Machine::new(), || {
        unknown.next();
    });
}
