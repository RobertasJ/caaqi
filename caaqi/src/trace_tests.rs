use caaqi_core::machine::Machine;

use super::*;
use crate::{time::times, value::create_value};

#[test]
fn run_step_returns_the_step_and_the_result() {
    scope(&mut Machine::new(), || {
        let (step, result) = run_step_with_id(|| 7);

        assert_eq!(result.get(), 7);
        assert!(step.exists());
        assert_eq!(step.parent(), None);
        assert!(!step.is_running());
        assert_eq!(steps(), [step]);
        let created = result.history()[0].time;
        assert_eq!(times(), [step.start(), step.end().unwrap(), created]);
    });
}

#[test]
fn a_running_step_is_current() {
    scope(&mut Machine::new(), || {
        assert_eq!(current_step(), None);
        let (step, inside) = run_step_with_id(|| {
            let current = current_step().unwrap();
            (current, current.is_running(), current.end())
        });
        let (current, running, end) = inside.get();

        assert_eq!((current, running, end), (step, true, None));
        assert_eq!(current_step(), None);
    });
}

#[test]
fn a_step_run_inside_another_is_inside_its_section() {
    scope(&mut Machine::new(), || {
        let (outer, inner) = run_step_with_id(|| run_step_with_id(|| ()).0);
        let inner = inner.get();

        assert_eq!(inner.parent(), Some(outer));
        assert_eq!(steps(), [outer, inner]);
        assert!(outer.start() < inner.start());
        assert!(inner.end().unwrap() < outer.end().unwrap());
    });
}

#[test]
fn reads_and_writes_are_recorded_on_the_current_step() {
    scope(&mut Machine::new(), || {
        let a = create_value(1);
        let b = create_value(0);
        let (step, _) = run_step_with_id(|| b.set(a.get() + 1));

        assert_eq!(
            step.reads(),
            [Access {
                value: a.id(),
                version: 0
            }]
        );
        assert_eq!(
            step.writes(),
            [Access {
                value: b.id(),
                version: 1
            }]
        );
    });
}

#[test]
fn a_panicking_step_is_closed_and_the_machine_stays_in_scope() {
    scope(&mut Machine::new(), || {
        let panicked = std::panic::catch_unwind(|| run_step_with_id(|| panic!("boom")));

        assert!(panicked.is_err());
        assert_eq!(current_step(), None);
        let [step] = steps()[..] else {
            panic!("one step ran")
        };
        assert!(!step.is_running());
    });
}

/// A step from a machine with more steps than the one in scope, so it isn't
/// in that one's trace.
fn foreign_step() -> Step {
    scope(&mut Machine::new(), || {
        run_step_with_id(|| ());
        run_step_with_id(|| ()).0
    })
}

#[test]
fn try_methods_fail_on_an_unknown_step() {
    let unknown = foreign_step();
    scope(&mut Machine::new(), || {
        let error = UnknownStep(unknown.into());

        assert!(!unknown.exists());
        assert_eq!(unknown.try_parent(), Err(error));
        assert_eq!(unknown.try_start(), Err(error));
        assert_eq!(unknown.try_end(), Err(error));
        assert_eq!(unknown.try_is_running(), Err(error));
        assert_eq!(unknown.try_reads(), Err(error));
        assert_eq!(unknown.try_writes(), Err(error));
    });
}

#[test]
#[should_panic(expected = "isn't in the trace")]
fn start_panics_on_an_unknown_step() {
    let unknown = foreign_step();
    scope(&mut Machine::new(), || {
        unknown.start();
    });
}

#[test]
fn run_step_returns_only_the_result() {
    scope(&mut Machine::new(), || {
        let step = run_step(current_step).get().expect("the step was current");
        assert_eq!(steps(), [step]);
    });
}
