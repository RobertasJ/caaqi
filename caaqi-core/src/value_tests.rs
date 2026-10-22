use super::*;
use crate::trace::StepId;

/// The snapshots of the value's history, oldest first: index `i` is
/// version `i`.
fn versions<T: Clone + 'static>(machine: &Machine, value: Value<T>) -> Vec<T> {
    machine
        .history(value)
        .unwrap()
        .iter()
        .map(|entry| entry.snapshot().clone())
        .collect()
}

fn reads(machine: &Machine, step: StepId) -> Vec<Access> {
    machine.step(step).unwrap().reads().collect()
}

fn writes(machine: &Machine, step: StepId) -> Vec<Access> {
    machine.step(step).unwrap().writes().collect()
}

fn access<T>(value: Value<T>, version: u64) -> Access {
    Access {
        value: value.id(),
        version,
    }
}

#[test]
fn a_new_value_holds_its_initial_value_at_version_0() {
    let mut machine = Machine::new();
    let value = machine.create_value(String::from("a"));

    assert_eq!(machine.read(value).unwrap(), "a");
    assert_eq!(machine.version(value), Ok(0));
    assert_eq!(versions(&machine, value), [String::from("a")]);
    let created = machine.history(value).unwrap()[0].time();
    assert_eq!(machine.timeline().last(), Some(created));
}

#[test]
fn writes_add_versions_at_the_end_of_the_timeline() {
    let mut machine = Machine::new();
    let value = machine.create_value(1);
    machine.write(value, 2).unwrap();
    machine.update(value, |value| *value *= 10).unwrap();

    assert_eq!(*machine.read(value).unwrap(), 20);
    assert_eq!(machine.version(value), Ok(2));
    assert_eq!(versions(&machine, value), [1, 2, 20]);

    let times: Vec<_> = machine
        .history(value)
        .unwrap()
        .iter()
        .map(|e| e.time())
        .collect();
    assert_eq!(machine.timeline().times().collect::<Vec<_>>(), times);
}

#[test]
fn values_keep_separate_histories() {
    let mut machine = Machine::new();
    let a = machine.create_value(1);
    let b = machine.create_value("b");
    machine.write(a, 2).unwrap();

    assert_eq!(versions(&machine, a), [1, 2]);
    assert_eq!(versions(&machine, b), ["b"]);
}

#[test]
fn read_at_returns_the_newest_write_at_or_before_the_time() {
    let mut machine = Machine::new();
    let before_creation = machine.timeline.push_back();
    let value = machine.create_value(1);
    let between = machine.timeline.push_back();
    machine.write(value, 2).unwrap();
    let written = machine.history(value).unwrap()[1].time();
    let after = machine.timeline.push_back();

    assert_eq!(machine.read_at(value, before_creation), Ok(None));
    let created = machine.history(value).unwrap()[0].time();
    assert_eq!(machine.read_at(value, created), Ok(Some(&1)));
    assert_eq!(machine.read_at(value, between), Ok(Some(&1)));
    assert_eq!(machine.read_at(value, written), Ok(Some(&2)));
    assert_eq!(machine.read_at(value, after), Ok(Some(&2)));
}

#[test]
fn read_at_a_step_returns_what_it_saw() {
    let mut machine = Machine::new();
    let value = machine.create_value(0);
    let mut steps = Vec::new();
    for _ in 0..5 {
        let (step, seen) = machine.run_step_with_id(|machine| {
            let seen = *machine.read(value).unwrap();
            machine.write(value, seen + 1).unwrap();
            seen
        });
        steps.push((step, seen));
    }

    for (step, seen) in steps {
        let seen = *machine.read(seen).unwrap();
        let start = machine.step(step).unwrap().start();
        assert_eq!(machine.read_at(value, start), Ok(Some(&seen)));
        let end = machine.step(step).unwrap().end().unwrap();
        assert_eq!(machine.read_at(value, end), Ok(Some(&(seen + 1))));
    }
}

#[test]
fn reads_and_writes_are_recorded_with_their_versions() {
    let mut machine = Machine::new();
    let value = machine.create_value(1);
    let (step, _) = machine.run_step_with_id(|machine| {
        machine.read(value).unwrap();
        machine.write(value, 2).unwrap();
        machine.read(value).unwrap();
        machine.update(value, |value| *value += 1).unwrap();
    });

    assert_eq!(reads(&machine, step), [access(value, 0), access(value, 1)]);
    assert_eq!(writes(&machine, step), [access(value, 1), access(value, 2)]);
}

#[test]
fn update_records_a_read_of_the_version_it_copied() {
    let mut machine = Machine::new();
    let value = machine.create_value(1);
    let (step, _) = machine.run_step_with_id(|machine| {
        machine.update(value, |value| *value += 1).unwrap();
    });

    assert_eq!(reads(&machine, step), [access(value, 0)]);
    assert_eq!(writes(&machine, step), [access(value, 1)]);
}

#[test]
fn reading_the_same_version_twice_records_it_once() {
    let mut machine = Machine::new();
    let value = machine.create_value(1);
    let (step, _) = machine.run_step_with_id(|machine| {
        machine.read(value).unwrap();
        machine.read(value).unwrap();
    });

    assert_eq!(reads(&machine, step), [access(value, 0)]);
}

#[test]
fn creating_a_value_in_a_step_records_a_write() {
    let mut machine = Machine::new();
    let (step, value) = machine.run_step_with_id(|machine| machine.create_value(1));
    let value = *machine.read(value).unwrap();

    assert_eq!(writes(&machine, step), [access(value, 0)]);
}

#[test]
fn read_at_records_the_version_it_returned() {
    let mut machine = Machine::new();
    let early = machine.timeline.push_back();
    let value = machine.create_value(1);
    let created = machine.history(value).unwrap()[0].time();
    machine.write(value, 2).unwrap();

    let (step, _) = machine.run_step_with_id(|machine| {
        machine.read_at(value, created).unwrap();
        // Before the value existed: nothing seen, nothing recorded.
        machine.read_at(value, early).unwrap();
    });

    assert_eq!(reads(&machine, step), [access(value, 0)]);
}

#[test]
fn inspecting_is_not_recorded() {
    let mut machine = Machine::new();
    let value = machine.create_value(1);
    let (step, _) = machine.run_step_with_id(|machine| {
        machine.version(value).unwrap();
        machine.history(value).unwrap();
    });

    assert_eq!(reads(&machine, step), []);
}

#[test]
fn reads_and_writes_outside_steps_are_not_recorded() {
    let mut machine = Machine::new();
    let value = machine.create_value(1);
    machine.read(value).unwrap();
    machine.write(value, 2).unwrap();
    let (step, _) = machine.run_step_with_id(|_| ());

    assert_eq!(reads(&machine, step), []);
    assert_eq!(writes(&machine, step), []);
    // The write still made a version.
    assert_eq!(versions(&machine, value), [1, 2]);
}

#[test]
fn an_unknown_value_fails() {
    let mut other = Machine::new();
    let value = other.create_value(1);
    let mut machine = Machine::new();
    let unknown = UnknownValue(value.id());

    assert_eq!(machine.read(value), Err(unknown));
    assert_eq!(
        machine.read_at(value, other.timeline().last().unwrap()),
        Err(unknown.into())
    );
    assert_eq!(machine.write(value, 2), Err(unknown));
    assert_eq!(machine.update(value, |_| ()), Err(unknown));
    assert_eq!(machine.version(value), Err(unknown));
    assert_eq!(machine.history(value).unwrap_err(), unknown);
    assert!(
        machine.timeline().is_empty(),
        "a failed write adds no point"
    );
}

#[test]
fn a_handle_of_another_type_fails() {
    let mut other = Machine::new();
    let text = other.create_value(String::new());
    let mut machine = Machine::new();
    machine.create_value(1);

    // The same slot holds an `i32` here.
    assert_eq!(machine.read(text), Err(UnknownValue(text.id())));
}

#[test]
fn read_at_an_unknown_time_fails() {
    let mut machine = Machine::new();
    let value = machine.create_value(1);
    let removed = machine.timeline.push_back();
    machine.timeline.remove(removed).unwrap();

    assert_eq!(
        machine.read_at(value, removed),
        Err(UnknownTime(removed).into())
    );
}
