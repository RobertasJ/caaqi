use super::*;
use crate::{
    time::{Time, now},
    value::create_value,
};

/// Runs `program` until it settles, and returns the machine and the result.
fn settle<R: Clone + 'static>(program: impl Fn() -> R + 'static) -> (Machine, R) {
    let (mut machine, result) = new_machine(program);
    machine.run().unwrap();
    let output = scope(&mut machine, || result.get());
    (machine, output)
}

#[test]
fn the_machine_is_in_scope_only_inside_scope_or_the_program() {
    assert!(!in_scope());
    let (mut machine, inside) = settle(in_scope);
    assert!(inside);
    scope(&mut machine, || assert!(in_scope()));
    assert!(!in_scope());
}

#[test]
fn the_program_runs_with_the_machine_in_scope() {
    let (mut machine, value) = settle(|| {
        let value = create_value(1);
        value.update(|value| *value += 1);
        value
    });

    assert_eq!(scope(&mut machine, || value.get()), 2);
}

#[test]
fn scopes_nest() {
    let (mut outer, a) = settle(|| create_value("outer"));
    let (mut inner, b) = settle(|| {
        create_value(0);
        create_value("inner")
    });

    scope(&mut outer, || {
        assert_eq!(a.get(), "outer");
        scope(&mut inner, || assert_eq!(b.get(), "inner"));
        assert_eq!(a.get(), "outer", "the outer machine is back in scope");
    });
}

#[test]
fn with_machine_reaches_the_machine_in_scope() {
    let (_, running) = settle(|| with_machine(|machine| machine.is_running()));
    assert!(running);
}

#[test]
fn scope_inside_with_machine_puts_it_back() {
    let (mut machine, value) = settle(|| {
        with_machine(|machine| {
            assert!(!in_scope());
            scope(machine, || create_value(1))
        })
    });

    assert_eq!(scope(&mut machine, || value.get()), 1);
}

#[test]
fn the_machine_is_back_in_scope_after_a_panic() {
    let (_, back) = settle(|| {
        let panicked = std::panic::catch_unwind(|| with_machine(|_| panic!("boom")));
        panicked.is_err() && in_scope()
    });
    assert!(back);
}

#[test]
fn a_stop_unwinds_cleanly_through_with_machine_and_nested_scopes() {
    let (mut machine, result) = new_machine(|| {
        let count = create_value(0);
        let mark = now();
        if count.get() == 0 {
            with_machine(|machine| scope(machine, || count.write_at(mark, 10)));
        }
        (count, in_scope())
    });

    assert_eq!(machine.run(), Ok(2));
    assert!(!in_scope());
    let (count, inside) = scope(&mut machine, || result.get());
    assert!(inside);
    assert_eq!(scope(&mut machine, || count.get()), 10);
}

#[test]
fn run_from_the_program_fails() {
    let (_, result) = settle(|| with_machine(|machine| machine.run()));
    assert_eq!(result, Err(AlreadyRunning));
}

#[test]
fn the_result_is_written_on_every_run() {
    let (mut machine, result) = new_machine(|| {
        let count = create_value(0);
        let mark: Time = now();
        let seen = count.get();
        if seen < 3 {
            count.write_at(mark, seen + 1);
        }
        seen
    });

    assert_eq!(machine.run(), Ok(4));
    assert_eq!(scope(&mut machine, || result.get()), 3);
}

#[test]
#[should_panic(expected = "no machine in scope")]
fn with_machine_panics_outside_scope() {
    with_machine(|_| ());
}

#[test]
#[should_panic(expected = "no machine in scope")]
fn with_machine_panics_inside_with_machine() {
    let (mut machine, _) = settle(|| ());
    scope(&mut machine, || {
        with_machine(|_| with_machine(|_| ()));
    });
}
