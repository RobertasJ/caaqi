use caaqi_core::{machine::Machine, time::UnknownTime};

use super::*;
use crate::{
    machine::scope,
    time::last,
    trace::{Access, run_step_with_id},
};

#[test]
fn get_set_and_update_change_the_value() {
    scope(&mut Machine::new(), || {
        let value = create_value(1);
        assert_eq!(value.get(), 1);

        value.set(2);
        value.update(|value| *value *= 10);
        assert_eq!(value.get(), 20);
        assert_eq!(value.with(|value| value + 1), 21);
        assert_eq!(value.version(), 2);
    });
}

#[test]
fn history_lists_every_write() {
    scope(&mut Machine::new(), || {
        let value = create_value("a");
        let created = last().unwrap();
        value.set("b");
        let written = last().unwrap();

        let expected = [
            HistoryEntry {
                time: created,
                snapshot: "a",
            },
            HistoryEntry {
                time: written,
                snapshot: "b",
            },
        ];
        assert_eq!(value.history(), expected);
    });
}

#[test]
fn get_at_returns_what_a_step_saw() {
    scope(&mut Machine::new(), || {
        let value = create_value(0);
        let runs: Vec<_> = (0..3)
            .map(|_| {
                run_step_with_id(|| {
                    let seen = value.get();
                    value.set(seen + 1);
                    seen
                })
            })
            .collect();

        for (step, seen) in runs {
            let seen = seen.get();
            assert_eq!(value.get_at(step.start()), Some(seen));
            assert_eq!(value.get_at(step.end().unwrap()), Some(seen + 1));
        }
    });
}

#[test]
fn get_at_before_creation_is_none() {
    scope(&mut Machine::new(), || {
        let (step, _) = run_step_with_id(|| ());
        let value = create_value(1);

        assert_eq!(value.get_at(step.start()), None);
    });
}

#[test]
fn reads_and_writes_are_recorded_on_the_current_step() {
    scope(&mut Machine::new(), || {
        let value = create_value(1);
        let (step, done) = run_step_with_id(|| {
            value.with(|_| ());
            value.update(|value| *value += 1);
        });

        let access = |version| Access {
            value: value.id(),
            version,
        };
        // `update` read version 0 too, to copy it.
        assert_eq!(step.reads(), [access(0)]);
        // The step also wrote its own result.
        let done = Access {
            value: done.id(),
            version: 0,
        };
        assert_eq!(step.writes(), [access(1), done]);
    });
}

/// A value from a machine with more values than the one in scope, so it
/// isn't in that one.
fn foreign_value() -> Value<i32> {
    scope(&mut Machine::new(), || {
        create_value(0);
        create_value(0)
    })
}

#[test]
fn try_methods_fail_on_an_unknown_value() {
    let unknown = foreign_value();
    scope(&mut Machine::new(), || {
        let known_time = create_value(0).history()[0].time;
        let error = UnknownValue(unknown.id());

        assert_eq!(unknown.try_get(), Err(error));
        assert_eq!(unknown.try_with(|_| ()), Err(error));
        assert_eq!(unknown.try_get_at(known_time), Err(error.into()));
        assert_eq!(unknown.try_set(1), Err(error));
        assert_eq!(unknown.try_update(|_| ()), Err(error));
        assert_eq!(unknown.try_version(), Err(error));
        assert_eq!(unknown.try_history(), Err(error));
    });
}

#[test]
fn get_at_an_unknown_time_fails() {
    let unknown_time = scope(&mut Machine::new(), || {
        for _ in 0..3 {
            create_value(0);
        }
        last().unwrap()
    });
    scope(&mut Machine::new(), || {
        let value = create_value(0);
        assert_eq!(
            value.try_get_at(unknown_time),
            Err(UnknownTime(unknown_time.into()).into())
        );
    });
}

#[test]
#[should_panic(expected = "isn't in the machine")]
fn get_panics_on_an_unknown_value() {
    let unknown = foreign_value();
    scope(&mut Machine::new(), || {
        unknown.get();
    });
}

#[test]
#[should_panic(expected = "no machine in scope")]
fn with_panics_if_its_closure_uses_the_machine() {
    scope(&mut Machine::new(), || {
        let value = create_value(0);
        value.with(|_| value.get());
    });
}
