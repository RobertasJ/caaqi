//! The scope: [`scope`] puts a [`Machine`] in a thread-local for the length
//! of a closure, and inside, this crate's functions reach it.
//!
//! Code built on this crate can reach the machine too, with
//! [`with_machine`]. Wrap what you build in functions like this crate's, so
//! the code that uses it never sees the machine.

pub use caaqi_core::machine::Machine;

scoped_tls_hkt::scoped_thread_local!(static mut MACHINE: Machine);

/// Makes `machine` reachable to this crate's functions while `f` runs, and
/// returns what `f` returns.
///
/// Scopes nest: inside, the innermost machine is the one reachable, and the
/// outer one is reachable again once `f` returns, even if it panics.
pub fn scope<R>(machine: &mut Machine, f: impl FnOnce() -> R) -> R {
    MACHINE.set(machine, f)
}

/// Whether this crate's functions would find a machine: inside a [`scope`],
/// and not inside a [`with_machine`] closure.
pub fn in_scope() -> bool {
    MACHINE.is_set()
}

/// Calls `f` with the machine in scope, and returns what `f` returns. Every
/// function in this crate is built on it.
///
/// The machine is out of scope while `f` runs, so `f` mustn't call this
/// crate's functions, or `with_machine` itself. To run code that uses them
/// from inside `f`, put the machine back in scope with
/// [`scope`]`(machine, || ...)`.
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
        "no machine in scope: caaqi's functions work only inside `scope`, and not inside \
         a closure that has the machine, such as `with_machine`'s or `Value::with`'s"
    );
    MACHINE.with(f)
}

#[cfg(test)]
#[path = "machine_tests.rs"]
mod tests;
