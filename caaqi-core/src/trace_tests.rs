use std::cmp::Ordering;

use super::*;

/// Whether `a` comes before `b` on the machine's timeline.
fn before(machine: &Machine, a: Time, b: Time) -> bool {
    machine.timeline().time_cmp(a, b).unwrap() == Ordering::Less
}

/// The step's start and end. Panics if it's still running.
fn section(machine: &Machine, step: StepId) -> (Time, Time) {
    let step = machine.step(step).unwrap();
    (step.start(), step.end().expect("the step finished"))
}

#[test]
fn run_step_returns_the_step_and_the_result() {
    let mut machine = Machine::new();
    let (step, result) = machine.run_step(|_| 7);

    assert_eq!(result, 7);
    let recorded = machine.step(step).unwrap();
    assert_eq!(recorded.parent(), None);
    assert!(!recorded.is_running());
    assert_eq!(machine.steps().collect::<Vec<_>>(), [step]);
}

#[test]
fn an_empty_step_adds_its_start_and_end_to_the_timeline() {
    let mut machine = Machine::new();
    let (step, ()) = machine.run_step(|_| ());

    let (start, end) = section(&machine, step);
    let times: Vec<_> = machine.timeline().times().collect();
    assert_eq!(times, [start, end]);
}

#[test]
fn a_running_step_is_current_and_has_no_end() {
    let mut machine = Machine::new();
    assert_eq!(machine.current_step(), None);

    let (step, inside) = machine.run_step(|machine| {
        let current = machine.current_step().unwrap();
        let recorded = machine.step(current).unwrap();
        (current, recorded.is_running(), recorded.end())
    });

    assert_eq!(inside, (step, true, None));
    assert_eq!(machine.current_step(), None);
}

#[test]
fn a_step_run_inside_another_is_its_child_and_inside_its_section() {
    let mut machine = Machine::new();
    let (outer, (inner, current_after)) = machine.run_step(|machine| {
        let (inner, ()) = machine.run_step(|_| ());
        (inner, machine.current_step())
    });

    assert_eq!(machine.step(inner).unwrap().parent(), Some(outer));
    assert_eq!(
        current_after,
        Some(outer),
        "the outer step is current again"
    );

    let (outer_start, outer_end) = section(&machine, outer);
    let (inner_start, inner_end) = section(&machine, inner);
    assert!(before(&machine, outer_start, inner_start));
    assert!(before(&machine, inner_start, inner_end));
    assert!(before(&machine, inner_end, outer_end));
}

#[test]
fn sibling_sections_follow_each_other() {
    let mut machine = Machine::new();
    let (parent, (first, second)) = machine.run_step(|machine| {
        let (first, ()) = machine.run_step(|_| ());
        let (second, ()) = machine.run_step(|_| ());
        (first, second)
    });

    assert_eq!(machine.step(first).unwrap().parent(), Some(parent));
    assert_eq!(machine.step(second).unwrap().parent(), Some(parent));
    let (_, first_end) = section(&machine, first);
    let (second_start, _) = section(&machine, second);
    assert!(before(&machine, first_end, second_start));
}

#[test]
fn steps_lists_every_step_in_timeline_order() {
    let mut machine = Machine::new();
    let (a, (b, c)) = machine.run_step(|machine| {
        let (b, c) = machine.run_step(|machine| machine.run_step(|_| ()).0);
        (b, c)
    });
    let (d, ()) = machine.run_step(|_| ());

    assert_eq!(machine.steps().collect::<Vec<_>>(), [a, b, c, d]);
}

#[test]
fn a_panicking_step_is_closed() {
    let mut machine = Machine::new();
    let mut started = None;
    let panicked = catch_unwind(AssertUnwindSafe(|| {
        machine.run_step(|machine| {
            started = machine.current_step();
            machine.run_step(|_| panic!("boom"));
        })
    }));

    assert!(panicked.is_err());
    assert_eq!(machine.current_step(), None);
    let outer = started.unwrap();
    assert!(!machine.step(outer).unwrap().is_running());
    // Both steps closed: two starts and two ends.
    assert_eq!(machine.timeline().len(), 4);
    for step in machine.steps().collect::<Vec<_>>() {
        assert!(!machine.step(step).unwrap().is_running());
    }
}

#[test]
fn reads_and_writes_are_recorded_on_the_innermost_step() {
    let mut machine = Machine::new();
    let a = machine.create_value(1);
    let b = machine.create_value(2);

    let (outer, inner) = machine.run_step(|machine| {
        machine.read(a).unwrap();
        machine
            .run_step(|machine| {
                machine.read(b).unwrap();
                machine.write(a, 10).unwrap();
            })
            .0
    });

    let outer = machine.step(outer).unwrap();
    let a0 = Access {
        value: a.id(),
        version: 0,
    };
    assert_eq!(outer.reads().collect::<Vec<_>>(), [a0]);
    assert_eq!(outer.writes().count(), 0);

    let inner = machine.step(inner).unwrap();
    let b0 = Access {
        value: b.id(),
        version: 0,
    };
    let a1 = Access {
        value: a.id(),
        version: 1,
    };
    assert_eq!(inner.reads().collect::<Vec<_>>(), [b0]);
    assert_eq!(inner.writes().collect::<Vec<_>>(), [a1]);
}

#[test]
fn an_unknown_step_fails() {
    let mut other = Machine::new();
    let (step, ()) = other.run_step(|_| ());

    assert_eq!(Machine::new().step(step).unwrap_err(), UnknownStep(step));
}
