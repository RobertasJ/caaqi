use super::*;
use crate::time::Timeline;

/// Runs `program` until it settles, and returns the machine and the result.
fn settle<R: Clone + 'static>(program: impl Fn(&mut Machine) -> R + 'static) -> (Machine, R) {
    let (mut machine, result) = Machine::new(program);
    machine.run().unwrap();
    let output = machine.read(result).unwrap().clone();
    (machine, output)
}

/// The snapshots of the value's history, oldest first: index `i` is
/// version `i`.
fn versions<T: Clone + 'static>(machine: &mut Machine, value: Value<T>) -> Vec<T> {
    machine
        .history(value)
        .unwrap()
        .iter()
        .map(|entry| entry.snapshot().clone())
        .collect()
}

/// A point from a timeline with more points than the machines in the
/// tests, so it isn't on theirs.
fn foreign_time() -> Time {
    let mut other = Timeline::new();
    (0..100).map(|_| other.push_back()).last().unwrap()
}

/// A value from another machine, with more values than the machines in the
/// tests, so its id isn't in them.
fn foreign_value() -> Value<i32> {
    settle(|machine| {
        for _ in 0..5 {
            machine.create_value(0).unwrap();
        }
        machine.create_value(0).unwrap()
    })
    .1
}

#[test]
fn a_new_value_holds_its_initial_contents_at_version_0_at_its_point() {
    let (mut machine, (value, after)) = settle(|machine| {
        let value = machine.create_value(String::from("a")).unwrap();
        (value, machine.now().unwrap())
    });

    assert_eq!(machine.read(value).unwrap(), "a");
    assert_eq!(machine.version(value), Ok(0));
    assert_eq!(versions(&mut machine, value), ["a"]);
    let created = machine.history(value).unwrap()[0].time();
    assert_eq!(value.id(), ValueId(created), "the id is the creation's point");
    assert_eq!(machine.timeline().next(created), Ok(Some(after)));
}

#[test]
fn writes_add_versions_at_their_own_points() {
    let (mut machine, value) = settle(|machine| {
        let value = machine.create_value(1).unwrap();
        machine.write(value, 2).unwrap();
        machine.update(value, |value| *value *= 10).unwrap();
        value
    });

    assert_eq!(machine.read(value), Ok(&20));
    assert_eq!(machine.version(value), Ok(2));
    assert_eq!(versions(&mut machine, value), [1, 2, 20]);
    let times: Vec<_> = machine
        .history(value)
        .unwrap()
        .iter()
        .map(HistoryEntry::time)
        .collect();
    // The writes are the creation and the next two events.
    let all: Vec<_> = machine.timeline().times().collect();
    assert_eq!(times, all[1..4]);
}

#[test]
fn values_keep_separate_histories() {
    let (mut machine, (a, b)) = settle(|machine| {
        let a = machine.create_value(1).unwrap();
        let b = machine.create_value("b").unwrap();
        machine.write(a, 2).unwrap();
        (a, b)
    });

    assert_eq!(versions(&mut machine, a), [1, 2]);
    assert_eq!(versions(&mut machine, b), ["b"]);
}

#[test]
fn a_read_sees_the_newest_write_at_or_before_it() {
    let (mut machine, (value, times)) = settle(|machine| {
        let before = machine.now().unwrap();
        let value = machine.create_value(1).unwrap();
        let between = machine.now().unwrap();
        machine.write(value, 2).unwrap();
        let after = machine.now().unwrap();
        (value, [before, between, after])
    });
    let [before, between, after] = times;
    let created = machine.history(value).unwrap()[0].time();
    let written = machine.history(value).unwrap()[1].time();

    assert_eq!(machine.read_at(value, before), Ok(None));
    assert_eq!(machine.read_at(value, created), Ok(Some(&1)));
    assert_eq!(machine.read_at(value, between), Ok(Some(&1)));
    assert_eq!(machine.read_at(value, written), Ok(Some(&2)));
    assert_eq!(machine.read_at(value, after), Ok(Some(&2)));
}

#[test]
fn reads_during_a_run_see_the_value_so_far() {
    let (_, seen) = settle(|machine| {
        let value = machine.create_value(0).unwrap();
        (1..4)
            .map(|n| {
                let seen = *machine.read(value).unwrap();
                machine.write(value, n).unwrap();
                seen
            })
            .collect::<Vec<_>>()
    });

    assert_eq!(seen, [0, 1, 2]);
}

#[test]
fn a_deleted_value_can_t_be_read_or_written() {
    let (mut machine, (value, alive, errors)) = settle(|machine| {
        let value = machine.create_value(1).unwrap();
        let alive = machine.now().unwrap();
        machine.delete(value).unwrap();
        let errors = (
            machine.read(value).unwrap_err(),
            machine.write(value, 2).unwrap_err(),
            machine.update(value, |_| ()).unwrap_err(),
            machine.delete(value).unwrap_err(),
            machine.version(value).unwrap_err(),
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
    // Its past can still be read, and inspected.
    assert_eq!(machine.read_at(value, alive), Ok(Some(&1)));
    let last = machine.timeline().last().unwrap();
    assert_eq!(machine.read_at(value, last), Err(deleted.into()));
    assert_eq!(versions(&mut machine, value), [1]);
}

#[test]
fn an_unknown_value_fails() {
    let unknown = foreign_value();
    let (mut machine, errors) = settle(move |machine| {
        let now = machine.now().unwrap();
        (
            machine.write(unknown, 2).unwrap_err(),
            machine.update(unknown, |_| ()).unwrap_err(),
            machine.delete(unknown).unwrap_err(),
            machine.write_at(unknown, now, 2).unwrap_err(),
        )
    });
    let error = UnknownValue(unknown.id());

    assert_eq!(
        errors,
        (error.into(), error.into(), error.into(), error.into())
    );
    assert_eq!(machine.read(unknown), Err(error.into()));
    let first = machine.timeline().first().unwrap();
    assert_eq!(machine.read_at(unknown, first), Err(error.into()));
    assert_eq!(machine.version(unknown), Err(error.into()));
    assert_eq!(machine.history(unknown).unwrap_err(), error);
}

#[test]
fn a_handle_of_another_type_fails() {
    let (mut machine, number) = settle(|machine| machine.create_value(1).unwrap());
    let text: Value<String> = Value::new(number.id());

    assert_eq!(machine.read(text), Err(UnknownValue(text.id()).into()));
}

#[test]
fn read_at_an_unknown_time_fails() {
    let (mut machine, (value, start)) = settle(|machine| {
        (
            machine.create_value(1).unwrap(),
            machine.timeline().first().unwrap(),
        )
    });
    let unknown = foreign_time();

    assert_eq!(
        machine.read_at(value, unknown),
        Err(UnknownTime(unknown).into())
    );
    // The program's start is a point, before the value's creation.
    assert_eq!(machine.read_at(value, start), Ok(None));
}

#[test]
fn the_result_is_unwritten_before_the_first_run() {
    let (mut machine, result) = Machine::new(|_| 1);
    let unwritten = Unwritten(result.id());

    assert_eq!(machine.read(result), Err(unwritten.into()));
    assert_eq!(machine.version(result), Err(unwritten.into()));
    assert_eq!(machine.history(result).map(<[_]>::len), Ok(0));
    let start = machine.timeline().first().unwrap();
    assert_eq!(machine.read_at(result, start), Ok(None));
}

#[test]
fn write_at_needs_an_earlier_event_of_this_run() {
    let (_, errors) = settle(|machine| {
        let start = machine.timeline().first().unwrap();
        let value = machine.create_value(1).unwrap();
        // The program's start isn't an event.
        let start = machine.write_at(value, start, 2).unwrap_err();
        let foreign = machine.write_at(value, foreign_time(), 2).unwrap_err();
        (start, foreign)
    });

    assert!(matches!(errors.0, WriteAtError::NotInThePast(_)));
    assert!(matches!(errors.1, WriteAtError::NotInThePast(_)));
}

#[test]
fn write_at_needs_the_value_to_exist_at_the_point() {
    let (_, (value, other, early, errors)) = settle(|machine| {
        let early = machine.now().unwrap();
        let value = machine.create_value(1).unwrap();
        let other = machine.create_value(1).unwrap();
        machine.delete(other).unwrap();
        let late = machine.now().unwrap();
        let errors = (
            machine.write_at(value, early, 2).unwrap_err(),
            machine.write_at(other, late, 2).unwrap_err(),
        );
        (value, other, early, errors)
    });

    assert_eq!(
        errors.0,
        NotYetCreated {
            value: value.id(),
            time: early
        }
        .into()
    );
    assert_eq!(errors.1, DeletedValue(other.id()).into());
}

#[test]
fn write_at_can_go_before_a_later_deletion() {
    let (mut machine, (value, alive)) = settle(|machine| {
        let value = machine.create_value(1).unwrap();
        let alive = machine.now().unwrap();
        let seen = *machine.read(value).unwrap();
        machine.delete(value).unwrap();
        if seen == 1 {
            machine.write_at(value, alive, 2).unwrap();
        }
        (value, alive)
    });

    assert_eq!(versions(&mut machine, value), [1, 2]);
    let written = machine.history(value).unwrap()[1].time();
    assert_eq!(machine.timeline().next(alive), Ok(Some(written)));
    assert_eq!(machine.read_at(value, written), Ok(Some(&2)));
}

#[test]
fn write_at_its_creation_point_is_in_the_past() {
    let (mut machine, value) = settle(|machine| {
        let value = machine.create_value(1).unwrap();
        if *machine.read(value).unwrap() == 1 {
            let created = machine.history(value).unwrap()[0].time();
            machine.write_at(value, created, 5).unwrap();
        }
        value
    });

    assert_eq!(versions(&mut machine, value), [1, 5]);
}

#[test]
fn bootstrapped_contents_need_no_clone() {
    /// Not `Clone`.
    #[derive(Debug, PartialEq)]
    struct Token(u32);

    let (mut machine, result) = Machine::new(|machine| {
        let token = machine.create_value(Token(0)).unwrap();
        let mark = machine.now().unwrap();
        if machine.read(token).unwrap().0 == 0 {
            machine.write_at(token, mark, Token(7)).unwrap();
        }
        token
    });
    machine.run().unwrap();
    machine.run().unwrap();
    let token = *machine.read(result).unwrap();

    assert_eq!(machine.read(token), Ok(&Token(7)));
}
