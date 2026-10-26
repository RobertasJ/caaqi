use std::{
    cell::{Cell, RefCell},
    panic::{self, AssertUnwindSafe},
};

use super::*;
use crate::value::{HistoryEntry, Unwritten};

/// The machine's points, in timeline order.
fn points(machine: &Machine) -> Vec<Time> {
    machine.timeline().times().collect()
}

/// The snapshots of the value's history, oldest first.
fn snapshots<T: Clone + 'static>(machine: &mut Machine, value: Value<T>) -> Vec<T> {
    machine
        .history(value)
        .unwrap()
        .iter()
        .map(|entry| entry.snapshot().clone())
        .collect()
}

/// The points of the value's history, oldest first.
fn times<T: 'static>(machine: &mut Machine, value: Value<T>) -> Vec<Time> {
    machine
        .history(value)
        .unwrap()
        .iter()
        .map(HistoryEntry::time)
        .collect()
}

#[test]
fn a_new_machine_runs_nothing() {
    let ran = std::rc::Rc::new(Cell::new(false));
    let (mut machine, result) = Machine::new({
        let ran = ran.clone();
        move |_| ran.set(true)
    });

    assert!(!ran.get());
    assert!(!machine.is_running());
    assert_eq!(machine.timeline().len(), 1, "only the program's start");
    assert_eq!(machine.read(result), Err(Unwritten(result.id()).into()));
}

#[test]
fn run_runs_the_program_and_writes_its_result() {
    let (mut machine, result) = Machine::new(|machine| {
        let value = machine.create_value(20).unwrap();
        machine.update(value, |value| *value += 1).unwrap();
        *machine.read(value).unwrap() * 2
    });

    assert_eq!(machine.run(), Ok(1));
    assert_eq!(machine.read(result), Ok(&42));
    assert!(!machine.is_running());
}

#[test]
fn the_program_is_running_only_during_a_run() {
    let (mut machine, result) = Machine::new(|machine| machine.is_running());
    machine.run().unwrap();

    assert_eq!(machine.read(result), Ok(&true));
    assert!(!machine.is_running());
}

#[test]
fn a_rerun_reuses_the_points_and_the_value_ids() {
    let (mut machine, result) = Machine::new(|machine| {
        let value = machine.create_value(1).unwrap();
        let now = machine.now().unwrap();
        machine.write(value, 2).unwrap();
        (value, now)
    });
    machine.run().unwrap();
    let first = points(&machine);
    let (value, now) = *machine.read(result).unwrap();

    machine.run().unwrap();
    assert_eq!(points(&machine), first);
    assert_eq!(*machine.read(result).unwrap(), (value, now));
    // The handle from the first run reaches the value of the second.
    assert_eq!(machine.read(value), Ok(&2));
}

/// The layout from the docs: the parent places a label in a 200-wide box,
/// using the width the child measures later. The child writes the width
/// into the past, at the parent's point.
fn layout() -> (Machine, Value<f32>) {
    Machine::new(|machine| {
        let child_width = machine.create_value::<Option<f32>>(None).unwrap();

        // The parent.
        let parent = machine.now().unwrap();
        let position = match *machine.read(child_width).unwrap() {
            Some(width) => (200.0 - width) / 2.0,
            None => 0.0,
        };

        // The child.
        let measured = 50.0;
        if *machine.read(child_width).unwrap() != Some(measured) {
            machine
                .write_at(child_width, parent, Some(measured))
                .unwrap();
        }

        position
    })
}

#[test]
fn layout_settles_on_the_second_run() {
    let (mut machine, position) = layout();

    assert_eq!(machine.run_once(), Ran::Stopped);
    assert_eq!(
        machine.read(position),
        Err(Unwritten(position.id()).into()),
        "the first run stopped before writing its result"
    );

    assert_eq!(machine.run_once(), Ran::Settled);
    assert_eq!(machine.read(position), Ok(&75.0));
    assert_eq!(machine.bootstrapped_count(), 1);
}

#[test]
fn layout_runs_twice() {
    let (mut machine, position) = layout();

    assert_eq!(machine.run(), Ok(2));
    assert_eq!(machine.read(position), Ok(&75.0));
    // Settled: running it again changes nothing.
    assert_eq!(machine.run(), Ok(1));
    assert_eq!(machine.read(position), Ok(&75.0));
}

/// A program that writes 10 into the past at `mark` if it sees 0.
fn bootstrapping() -> (Machine, Value<(Value<i32>, Time)>) {
    Machine::new(|machine| {
        let count = machine.create_value(0).unwrap();
        let mark = machine.now().unwrap();
        if *machine.read(count).unwrap() == 0 {
            machine.write_at(count, mark, 10).unwrap();
        }
        (count, mark)
    })
}

#[test]
fn a_bootstrapped_write_is_applied_at_its_point_on_every_rerun() {
    let (mut machine, result) = bootstrapping();
    assert_eq!(machine.run(), Ok(2));
    let (count, mark) = *machine.read(result).unwrap();

    let written = machine.bootstrapped_at(mark);
    assert_eq!(written.len(), 1);
    assert_eq!(machine.timeline().next(mark), Ok(Some(written[0])));
    assert_eq!(snapshots(&mut machine, count), [0, 10]);
    let history = times(&mut machine, count);
    assert_eq!(history[1], written[0], "the write is at its own point");

    for _ in 0..3 {
        assert_eq!(machine.run(), Ok(1));
        assert_eq!(*machine.read(result).unwrap(), (count, mark));
        assert_eq!(machine.bootstrapped_at(mark), written);
        assert_eq!(snapshots(&mut machine, count), [0, 10]);
        assert_eq!(times(&mut machine, count), history);
    }
}

#[test]
fn a_bootstrapped_write_is_seen_by_reads_after_its_event() {
    let (mut machine, result) = Machine::new(|machine| {
        let count = machine.create_value(0).unwrap();
        let mark = machine.now().unwrap();
        let seen = *machine.read(count).unwrap();
        if seen == 0 {
            machine.write_at(count, mark, 10).unwrap();
        }
        // At `mark` itself, the write isn't there yet: it's right after.
        let at_mark = *machine.read_at(count, mark).unwrap().unwrap();
        (seen, at_mark)
    });
    machine.run().unwrap();

    assert_eq!(machine.read(result), Ok(&(10, 0)));
}

#[test]
fn the_events_after_a_write_into_the_past_are_wiped_with_their_points() {
    let (mut machine, result) = bootstrapping();
    assert_eq!(machine.run_once(), Ran::Stopped);
    // The start, the creation, `now` and the read.
    let stopped = points(&machine);
    assert_eq!(stopped.len(), 4);

    machine.undo_last_run();
    // The read is gone, and the write is right after `now`.
    let mark = stopped[2];
    let after = points(&machine);
    assert_eq!(after.len(), 4);
    assert_eq!(after[..3], stopped[..3]);
    assert_eq!(machine.bootstrapped_at(mark), [after[3]]);
    assert!(!machine.timeline().contains(stopped[3]));

    assert_eq!(machine.run_once(), Ran::Settled);
    let (_, seen_mark) = *machine.read(result).unwrap();
    assert_eq!(seen_mark, mark, "`now` took its old point");
}

#[test]
fn a_newer_write_at_the_same_point_drops_the_older_one() {
    let (mut machine, result) = Machine::new(|machine| {
        let count = machine.create_value(0).unwrap();
        let mark = machine.now().unwrap();
        let seen = *machine.read(count).unwrap();
        if seen < 2 {
            machine.write_at(count, mark, seen + 1).unwrap();
        }
        (count, mark)
    });

    assert_eq!(machine.run(), Ok(3));
    let (count, mark) = *machine.read(result).unwrap();
    assert_eq!(machine.bootstrapped_at(mark).len(), 1);
    assert_eq!(snapshots(&mut machine, count), [0, 2]);
}

#[test]
fn a_write_further_into_the_past_drops_the_bootstrapped_writes_after_it() {
    // `b`'s write is made only while `a` is 0, and the write to `a`, at an
    // earlier point, wipes it.
    let (mut machine, result) = Machine::new(|machine| {
        let a = machine.create_value(0).unwrap();
        let early = machine.now().unwrap();
        let b = machine.create_value(0).unwrap();
        let late = machine.now().unwrap();
        let seen_a = *machine.read(a).unwrap();
        let seen_b = *machine.read(b).unwrap();
        if seen_a == 0 && seen_b == 0 {
            machine.write_at(b, late, 1).unwrap();
        }
        if seen_a == 0 && seen_b == 1 {
            machine.write_at(a, early, 1).unwrap();
        }
        (early, late, seen_a, seen_b)
    });

    // Run 1 writes `b` at `late`, run 2 writes `a` at `early`, and run 3,
    // with `b`'s write gone, settles.
    assert_eq!(machine.run(), Ok(3));
    let (early, late, seen_a, seen_b) = *machine.read(result).unwrap();
    assert_eq!((seen_a, seen_b), (1, 0));
    assert_eq!(machine.bootstrapped_at(early).len(), 1);
    assert_eq!(machine.bootstrapped_at(late), []);
    assert_eq!(machine.bootstrapped_count(), 1);
}

#[test]
fn a_dropped_write_comes_back_if_its_code_runs_again() {
    // As above, but `b`'s write doesn't depend on `a`: the code that made it
    // runs again after the write to `a`, and makes it again.
    let (mut machine, result) = Machine::new(|machine| {
        let a = machine.create_value(0).unwrap();
        let early = machine.now().unwrap();
        let b = machine.create_value(0).unwrap();
        let late = machine.now().unwrap();
        let seen_a = *machine.read(a).unwrap();
        let seen_b = *machine.read(b).unwrap();
        if seen_b == 0 {
            machine.write_at(b, late, 1).unwrap();
        }
        if seen_a == 0 {
            machine.write_at(a, early, 1).unwrap();
        }
        (seen_a, seen_b)
    });

    // Run 1 writes `b`, run 2 writes `a` (dropping `b`'s write), run 3
    // writes `b` again, and run 4 settles.
    assert_eq!(machine.run(), Ok(4));
    assert_eq!(machine.read(result), Ok(&(1, 1)));
    assert_eq!(machine.bootstrapped_count(), 2);
}

#[test]
fn a_write_into_the_past_can_go_after_a_bootstrapped_write() {
    let (mut machine, result) = Machine::new(|machine| {
        let a = machine.create_value(0).unwrap();
        let b = machine.create_value(0).unwrap();
        let mark = machine.now().unwrap();
        if *machine.read(a).unwrap() == 0 {
            machine.write_at(a, mark, 1).unwrap();
        }
        // `a`'s bootstrapped write is the first write at `mark` or after.
        let after_a = machine.history(a).unwrap()[1].time();
        if *machine.read(b).unwrap() == 0 {
            machine.write_at(b, after_a, 2).unwrap();
        }
        (a, b, mark)
    });

    assert_eq!(machine.run(), Ok(3));
    let (a, b, mark) = *machine.read(result).unwrap();
    let attached = machine.bootstrapped_at(mark);
    assert_eq!(attached.len(), 2, "both are attached to `mark`, in order");
    assert_eq!(times(&mut machine, a)[1], attached[0]);
    assert_eq!(times(&mut machine, b)[1], attached[1]);
    assert_eq!(snapshots(&mut machine, b), [0, 2]);
}

thread_local! {
    /// How many times the panic hook ran on this thread.
    static HOOK_CALLS: Cell<usize> = const { Cell::new(0) };
}

#[test]
fn a_stop_prints_nothing() {
    let previous = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        HOOK_CALLS.with(|calls| calls.set(calls.get() + 1));
        previous(info);
    }));

    let (mut machine, _) = layout();
    assert_eq!(machine.run(), Ok(2));
    assert_eq!(
        HOOK_CALLS.with(Cell::get),
        0,
        "a stop doesn't call the panic hook"
    );
}

#[test]
fn a_panic_in_the_program_is_passed_on_and_the_machine_stays_valid() {
    let seen = std::rc::Rc::new(RefCell::new(None));
    let (mut machine, result) = Machine::new({
        let seen = seen.clone();
        move |machine| {
            let value = machine.create_value(1).unwrap();
            *seen.borrow_mut() = Some(value);
            if *machine.read(value).unwrap() == 1 {
                panic!("boom");
            }
        }
    });

    let panicked = panic::catch_unwind(AssertUnwindSafe(|| machine.run()));
    let message = *panicked.unwrap_err().downcast::<&str>().unwrap();
    assert_eq!(message, "boom");
    assert!(!machine.is_running());
    let value = seen.borrow().unwrap();
    // The machine is as the run left it.
    assert_eq!(machine.read(value), Ok(&1));
    assert_eq!(machine.read(result), Err(Unwritten(result.id()).into()));

    // The next run starts over, at the same points.
    let before = points(&machine);
    let panicked = panic::catch_unwind(AssertUnwindSafe(|| machine.run()));
    assert!(panicked.is_err());
    assert_eq!(points(&machine), before);
}

#[test]
fn run_from_the_program_fails() {
    let (mut machine, result) = Machine::new(|machine| machine.run());
    machine.run().unwrap();

    assert_eq!(machine.read(result), Ok(&Err(AlreadyRunning)));
}
