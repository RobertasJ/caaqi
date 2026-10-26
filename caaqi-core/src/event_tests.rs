use std::cmp::Ordering;

use super::*;
use crate::value::{DeletedValue, UnknownValue, Value};

/// Runs `program` until it settles, and returns the machine and the result.
fn settle<R: Clone + 'static>(program: impl Fn(&mut Machine) -> R + 'static) -> (Machine, R) {
    let (mut machine, result) = Machine::new(program);
    machine.run().unwrap();
    let output = machine.read(result).unwrap().clone();
    (machine, output)
}

/// The machine's points, in timeline order.
fn points(machine: &Machine) -> Vec<Time> {
    machine.timeline().times().collect()
}

#[test]
fn every_call_during_a_run_is_an_event_at_its_own_point() {
    let (machine, times) = settle(|machine| {
        let mut times = Vec::new();
        let value = machine.create_value(1).unwrap();
        times.push(machine.history(value).unwrap()[0].time());
        machine.read(value).unwrap();
        times.push(machine.now().unwrap());
        machine.write(value, 2).unwrap();
        times.push(machine.history(value).unwrap()[1].time());
        machine.update(value, |value| *value += 1).unwrap();
        times.push(machine.history(value).unwrap()[2].time());
        machine.read_at(value, times[0]).unwrap();
        machine.version(value).unwrap();
        machine.delete(value).unwrap();
        times.push(machine.now().unwrap());
        times
    });

    // The program's start, then every event: create, history, read, now,
    // write, history, update, history, read_at, version, delete, now, and
    // the result's write.
    let all = points(&machine);
    assert_eq!(all.len(), 1 + 13);
    assert_eq!(times[0], all[1], "the creation is the first event");
    assert_eq!(times[1], all[4], "now returns its own point");
    assert_eq!(times[2], all[5], "the write's entry is at its point");
    assert_eq!(times[3], all[7], "the update's entry is at its point");
    assert_eq!(times[4], all[12]);
}

#[test]
fn now_returns_its_own_point_and_each_call_is_a_new_one() {
    let (machine, (a, b)) = settle(|machine| (machine.now().unwrap(), machine.now().unwrap()));

    assert_eq!(machine.timeline().time_cmp(a, b), Ok(Ordering::Less));
    assert_eq!(points(&machine)[1..3], [a, b]);
}

#[test]
fn outside_a_run_changes_and_now_fail() {
    let (mut machine, value) = settle(|machine| machine.create_value(1).unwrap());
    let len = machine.timeline().len();

    assert_eq!(machine.create_value(1), Err(NotRunning));
    assert_eq!(machine.now(), Err(NotRunning));
    assert_eq!(machine.write(value, 2), Err(NotRunning.into()));
    assert_eq!(machine.update(value, |_| ()), Err(NotRunning.into()));
    assert_eq!(machine.delete(value), Err(NotRunning.into()));
    let time = machine.timeline().last().unwrap();
    assert_eq!(machine.write_at(value, time, 2), Err(NotRunning.into()));
    assert_eq!(machine.timeline().len(), len, "no points were added");
}

#[test]
fn reading_and_inspecting_outside_a_run_are_not_events() {
    let (mut machine, value) = settle(|machine| machine.create_value(1).unwrap());
    let before = points(&machine);
    let created = machine.history(value).unwrap()[0].time();

    assert_eq!(machine.read(value), Ok(&1));
    assert_eq!(machine.read_at(value, created), Ok(Some(&1)));
    assert_eq!(machine.version(value), Ok(0));
    assert!(!machine.is_running());
    assert_eq!(points(&machine), before);
}

#[test]
fn a_failed_call_is_not_an_event() {
    let foreign = foreign_value();
    let (machine, (value, read, write)) = settle(move |machine| {
        let value = machine.create_value(1).unwrap();
        machine.delete(value).unwrap();
        let read = machine.read(foreign).unwrap_err();
        let write = machine.write(value, 2).unwrap_err();
        (value, read, write)
    });

    assert_eq!(read, UnknownValue(foreign.id()).into());
    assert_eq!(write, DeletedValue(value.id()).into());
    // The start, the creation, the deletion and the result's write.
    assert_eq!(machine.timeline().len(), 4);
}

#[test]
fn undoing_a_creation_removes_the_value() {
    let (mut machine, value) = settle(|machine| machine.create_value(1).unwrap());
    let before = points(&machine);

    machine.undo_last_run();
    assert_eq!(machine.read(value), Err(UnknownValue(value.id()).into()));
    assert_eq!(points(&machine), before, "undone events keep their points");
}

#[test]
fn undoing_a_write_removes_its_history_entry() {
    let (mut machine, value) = settle(|machine| {
        let value = machine.create_value(1).unwrap();
        machine.write(value, 2).unwrap();
        machine.update(value, |value| *value += 1).unwrap();
        value
    });
    assert_eq!(machine.history(value).unwrap().len(), 3);

    // Undo only the update and the write, by hand, newest first.
    let mut log = mem::take(&mut machine.events.log);
    let created_and_before = log.len() - 3;
    let result_write = log.pop().unwrap();
    (result_write.undo)(&mut machine.values);
    for logged in log.drain(created_and_before..).rev() {
        (logged.undo)(&mut machine.values);
    }
    let history: Vec<_> = machine
        .history(value)
        .unwrap()
        .iter()
        .map(|entry| *entry.snapshot())
        .collect();
    assert_eq!(history, [1]);
}

#[test]
fn undoing_a_deletion_un_deletes() {
    let (mut machine, value) = settle(|machine| {
        let value = machine.create_value(1).unwrap();
        machine.delete(value).unwrap();
        value
    });
    assert_eq!(machine.read(value), Err(DeletedValue(value.id()).into()));

    // Undo only the result's write and the deletion.
    let mut log = mem::take(&mut machine.events.log);
    for logged in log.drain(log.len() - 2..).rev() {
        (logged.undo)(&mut machine.values);
    }
    assert_eq!(machine.read(value), Ok(&1));
}

#[test]
fn undoing_a_read_does_nothing() {
    let (mut machine, value) = settle(|machine| {
        let value = machine.create_value(1).unwrap();
        machine.read(value).unwrap();
        machine.now().unwrap();
        value
    });

    // Undo the result's write, the `now` and the read.
    let mut log = mem::take(&mut machine.events.log);
    for logged in log.drain(log.len() - 3..).rev() {
        (logged.undo)(&mut machine.values);
    }
    assert_eq!(machine.read(value), Ok(&1));
    assert_eq!(machine.history(value).unwrap().len(), 1);
}

#[test]
fn the_result_is_written_as_the_last_event() {
    let (mut machine, result) = Machine::new(|machine| {
        machine.create_value(1).unwrap();
        7
    });
    machine.run().unwrap();

    let written = machine.history(result).unwrap()[0].time();
    assert_eq!(machine.timeline().last(), Some(written));
}

#[test]
fn undoing_the_last_run_undoes_every_event_and_keeps_the_points() {
    let (mut machine, value) = settle(|machine| {
        let value = machine.create_value(vec![1]).unwrap();
        machine.update(value, |value| value.push(2)).unwrap();
        value
    });
    let before = points(&machine);

    machine.undo_last_run();
    assert_eq!(machine.read(value), Err(UnknownValue(value.id()).into()));
    assert_eq!(points(&machine), before);
    assert_eq!(machine.events.old.len(), before.len() - 1);
    // Ending a run that took none of the old points drops them.
    machine.end_run();
    assert_eq!(machine.timeline().len(), 1, "only the program's start is left");
}

#[test]
fn events_are_logged_in_timeline_order() {
    let (machine, _) = settle(|machine| {
        let value = machine.create_value(0).unwrap();
        for n in 1..5 {
            machine.write(value, n).unwrap();
        }
    });

    let logged: Vec<_> = machine.events.log.iter().map(|logged| logged.time).collect();
    assert_eq!(logged, points(&machine)[1..]);
}

/// A value from another machine.
fn foreign_value() -> Value<i32> {
    settle(|machine| {
        machine.create_value(0).unwrap();
        machine.create_value(0).unwrap()
    })
    .1
}

#[test]
fn a_failed_event_gives_back_a_matched_point() {
    let foreign = foreign_value();
    let (mut machine, _) = Machine::new(move |machine| {
        let value = machine.create_value(0).unwrap();
        // Fails, so it takes no point, in this run or the next.
        machine.read(foreign).unwrap_err();
        machine.read(value).unwrap();
    });
    machine.run().unwrap();
    let first = points(&machine);
    machine.run().unwrap();

    assert_eq!(points(&machine), first);
}
