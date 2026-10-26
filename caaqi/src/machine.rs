//! The machine, scoped: [`new_machine`] makes a machine whose program runs
//! with the machine in a thread-local, and this crate's functions reach it
//! there. [`scope`] does the same for any closure, such as code that reads
//! results outside a run.
//!
//! Code built on this crate can reach the machine too, with
//! [`with_machine`]. Wrap what you build in functions like this crate's, so
//! the code that uses it never sees the machine.

use caaqi_core::value::Value as CoreValue;
pub use caaqi_core::{
    event::NotRunning,
    machine::{AlreadyRunning, Machine},
};

use crate::value::Value;

scoped_tls_hkt::scoped_thread_local!(static mut MACHINE: Machine);

/// A machine that runs `program` with the machine in scope, and the value
/// each run writes the program's result to: [`Machine::new`], scoped.
///
/// Nothing runs until [`Machine::run`], which runs the program until it
/// settles. The program must depend on nothing but the machine, and it's
/// `Fn`, so it keeps no state of its own between runs. A write into the past
/// ([`Value::write_at`]) stops the run by unwinding: that needs
/// `panic = "unwind"`, and the program mustn't catch unwinds itself.
pub fn new_machine<R: 'static>(program: impl Fn() -> R + 'static) -> (Machine, Value<R>) {
    let (machine, result): (Machine, CoreValue<R>) =
        Machine::new(move |machine| scope(machine, &program));
    (machine, result.into())
}

/// Makes `machine` reachable to this crate's functions while `f` runs, and
/// returns what `f` returns.
///
/// Scopes nest: inside, the innermost machine is the one reachable, and the
/// outer one is reachable again once `f` returns, even if it panics or a
/// write into the past unwinds through it.
pub fn scope<R>(machine: &mut Machine, f: impl FnOnce() -> R) -> R {
    MACHINE.set(machine, f)
}

/// Whether this crate's functions would find a machine: inside a [`scope`]
/// (a running program is inside one), and not inside a [`with_machine`]
/// closure.
pub fn in_scope() -> bool {
    MACHINE.is_set()
}

/// Calls `f` with the machine in scope, and returns what `f` returns. Every
/// function in this crate is built on it.
///
/// The machine is out of scope while `f` runs, so `f` mustn't call this
/// crate's functions, or `with_machine` itself. To run code that uses them
/// from inside `f`, put the machine back in scope with
/// [`scope`]`(machine, || ...)`. A write into the past in `f` unwinds
/// through `with_machine` cleanly: the machine is back in scope as the
/// unwind leaves `f`.
///
/// # Panics
///
/// If no machine is in scope: outside a [`scope`], or inside another
/// `with_machine` closure, such as the closure of
/// [`Value::with`](crate::value::Value::with).
#[track_caller]
pub fn with_machine<R>(f: impl FnOnce(&mut Machine) -> R) -> R {
    assert!(
        in_scope(),
        "no machine in scope: caaqi's functions work only inside `scope` or a running \
         program, and not inside a closure that has the machine, such as `with_machine`'s or \
         `Value::with`'s"
    );
    MACHINE.with(f)
}

#[cfg(test)]
#[path = "machine_tests.rs"]
mod tests;
