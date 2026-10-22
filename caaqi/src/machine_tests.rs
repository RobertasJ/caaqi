use super::*;
use crate::{trace::run_step, value::create_value};

#[test]
fn the_machine_is_in_scope_only_inside_scope() {
    assert!(!in_scope());
    scope(&mut Machine::new(), || assert!(in_scope()));
    assert!(!in_scope());
}

#[test]
fn scopes_nest() {
    let mut outer = Machine::new();
    let mut inner = Machine::new();
    scope(&mut outer, || {
        create_value(1);
        scope(&mut inner, || {
            create_value(2);
            create_value(3);
        });
        create_value(4);
    });

    assert_eq!(outer.timeline().len(), 2);
    assert_eq!(inner.timeline().len(), 2);
}

#[test]
fn with_machine_reaches_the_machine_in_scope() {
    let mut machine = Machine::new();
    scope(&mut machine, || {
        run_step(|| ());
        assert_eq!(with_machine(|machine| machine.steps().count()), 1);
    });
}

#[test]
fn scope_inside_with_machine_puts_it_back() {
    let mut machine = Machine::new();
    scope(&mut machine, || {
        with_machine(|machine| {
            assert!(!in_scope());
            scope(machine, || create_value(1));
        });
    });
    assert_eq!(machine.timeline().len(), 1);
}

#[test]
fn the_machine_is_back_in_scope_after_a_panic() {
    let mut machine = Machine::new();
    scope(&mut machine, || {
        let panicked = std::panic::catch_unwind(|| with_machine(|_| panic!("boom")));
        assert!(panicked.is_err());
        assert!(in_scope());
    });
}

#[test]
#[should_panic(expected = "no machine in scope")]
fn with_machine_panics_outside_scope() {
    with_machine(|_| ());
}

#[test]
#[should_panic(expected = "no machine in scope")]
fn with_machine_panics_inside_with_machine() {
    scope(&mut Machine::new(), || {
        with_machine(|_| with_machine(|_| ()))
    });
}
