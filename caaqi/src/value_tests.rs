use caaqi_core::time::UnknownTime;

use super::*;
use crate::{
    machine::{Machine, new_machine, scope},
    time::{last, now},
};

/// Runs `program` until it settles, and returns the machine and the result.
fn settle<R: Clone + 'static>(program: impl Fn() -> R + 'static) -> (Machine, R) {
    let (mut machine, result) = new_machine(program);
    machine.run().unwrap();
    let output = scope(&mut machine, || result.get());
    (machine, output)
}

/// A value from a machine with more values than the ones in the tests, so
/// it isn't in them.
fn foreign_value() -> Value<i32> {
    settle(|| {
        for _ in 0..100 {
            create_value(0);
        }
        create_value(0)
    })
    .1
}

#[test]
fn get_set_and_update_change_the_value() {
    let (mut machine, value) = settle(|| {
        let value = create_value(1);
        assert_eq!(value.get(), 1);
        value.set(2);
        value.update(|value| *value *= 10);
        value
    });

    scope(&mut machine, || {
        assert_eq!(value.get(), 20);
        assert_eq!(value.with(|value| value + 1), 21);
        assert_eq!(value.version(), 2);
    });
}

#[test]
fn history_lists_every_write() {
    let (mut machine, (value, created, written)) = settle(|| {
        let value = create_value("a");
        let created = now().prev().unwrap();
        value.set("b");
        let written = now().prev().unwrap();
        (value, created, written)
    });

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
    assert_eq!(scope(&mut machine, || value.history()), expected);
}

#[test]
fn get_at_returns_what_a_read_there_saw() {
    let (mut machine, (value, marks)) = settle(|| {
        let value = create_value(0);
        let marks: Vec<_> = (1..4)
            .map(|n| {
                let mark = now();
                value.set(n);
                (mark, n - 1)
            })
            .collect();
        (value, marks)
    });

    scope(&mut machine, || {
        for (mark, seen) in marks {
            assert_eq!(value.get_at(mark), Some(seen));
        }
    });
}

#[test]
fn get_at_before_creation_is_none() {
    let (mut machine, (before, value)) = settle(|| (now(), create_value(1)));
    scope(&mut machine, || assert_eq!(value.get_at(before), None));
}

#[test]
fn write_at_writes_into_the_past_and_the_program_goes_on_from_there() {
    let (mut machine, result) = new_machine(|| {
        let width = create_value::<Option<f32>>(None);
        let parent = now();
        let position = width.get().map_or(0.0, |width| (200.0 - width) / 2.0);
        if width.get() != Some(50.0) {
            width.write_at(parent, Some(50.0));
        }
        (width, parent, position)
    });

    assert_eq!(machine.run(), Ok(2));
    scope(&mut machine, || {
        let (width, parent, position) = result.get();
        assert_eq!(position, 75.0);
        let written = width.history()[1].time;
        assert_eq!(parent.next(), Some(written));
        assert_eq!(width.get_at(parent), Some(None));
        assert_eq!(width.get_at(written), Some(Some(50.0)));
    });
}

#[test]
fn a_deleted_value_can_t_be_read_or_written() {
    let (mut machine, (value, alive, errors)) = settle(|| {
        let value = create_value(1);
        let alive = now();
        value.delete();
        let errors = (
            value.try_get().unwrap_err(),
            value.try_set(2).unwrap_err(),
            value.try_update(|_| ()).unwrap_err(),
            value.try_delete().unwrap_err(),
            value.try_version().unwrap_err(),
        );
        (value, alive, errors)
    });
    let deleted = DeletedValue(value.id());

    assert_eq!(
        errors,
        (
            deleted.into(),
            deleted.into(),
            deleted.into(),
            deleted.into(),
            deleted.into()
        )
    );
    scope(&mut machine, || {
        assert_eq!(value.get_at(alive), Some(1));
        assert_eq!(value.history().len(), 1);
    });
}

#[test]
fn changes_outside_a_run_fail() {
    let (mut machine, value) = settle(|| create_value(1));

    scope(&mut machine, || {
        let time = last().unwrap();
        assert_eq!(try_create_value(1), Err(NotRunning));
        assert_eq!(value.try_set(2), Err(NotRunning.into()));
        assert_eq!(value.try_update(|_| ()), Err(NotRunning.into()));
        assert_eq!(value.try_delete(), Err(NotRunning.into()));
        assert_eq!(value.try_write_at(time, 2), Err(NotRunning.into()));
        // Reading still works.
        assert_eq!(value.get(), 1);
    });
}

#[test]
fn try_methods_fail_on_an_unknown_value() {
    let unknown = foreign_value();
    let (mut machine, (known_time, errors)) = settle(move || {
        let known_time = now();
        let errors = (
            unknown.try_set(1).unwrap_err(),
            unknown.try_update(|_| ()).unwrap_err(),
            unknown.try_delete().unwrap_err(),
            unknown.try_write_at(known_time, 1).unwrap_err(),
        );
        (known_time, errors)
    });
    let error = UnknownValue(unknown.id());

    assert_eq!(
        errors,
        (error.into(), error.into(), error.into(), error.into())
    );
    scope(&mut machine, || {
        assert_eq!(unknown.try_get(), Err(error.into()));
        assert_eq!(unknown.try_with(|_| ()), Err(error.into()));
        assert_eq!(unknown.try_get_at(known_time), Err(error.into()));
        assert_eq!(unknown.try_version(), Err(error.into()));
        assert_eq!(unknown.try_history(), Err(error));
    });
}

#[test]
fn try_write_at_fails_before_the_value_exists() {
    let (_, (value, early, error)) = settle(|| {
        let early = now();
        let value = create_value(1);
        let error = value.try_write_at(early, 2).unwrap_err();
        (value, early, error)
    });

    let expected = NotYetCreated {
        value: value.id(),
        time: early.into(),
    };
    assert_eq!(error, expected.into());
}

#[test]
fn get_at_an_unknown_time_fails() {
    let unknown_time = settle(|| {
        for _ in 0..100 {
            now();
        }
        now()
    })
    .1;
    let (mut machine, value) = settle(|| create_value(0));

    scope(&mut machine, || {
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
    let (mut machine, _) = settle(|| ());
    scope(&mut machine, || {
        unknown.get();
    });
}

#[test]
#[should_panic(expected = "isn't running")]
fn create_value_panics_outside_a_run() {
    let (mut machine, _) = settle(|| ());
    scope(&mut machine, || {
        create_value(1);
    });
}

#[test]
#[should_panic(expected = "no machine in scope")]
fn with_panics_if_its_closure_uses_the_machine() {
    let (mut machine, value) = settle(|| create_value(0));
    scope(&mut machine, || {
        value.with(|_| value.get());
    });
}
