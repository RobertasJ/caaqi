# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Purpose

caaqi gives programs the illusion of time travel. A program runs like a normal program, top to bottom, but its past can be inspected and later rewritten, and everything after a change plays out again. Under the hood it's an incremental computation engine; time travel is the semantics the programmer reasons about.

The goal is **consistency**: after any change, the end state is exactly what a fresh run from scratch would produce.

caaqi was restarted from scratch around this model. The previous implementations (a trace tree with rewinds, groups, a type-map `Context`, tracked cells and a linear executor; then an append-only trace of steps) are gone from the tree; jj history is the reference for them.

## Commands

Use `jj` instead of `git` for version-control operations, including status, diffs, and history.

Use the `justfile` recipes from the workspace root; fall back to direct Cargo only when no recipe fits. The Nix dev environment comes from `flake.nix` / `.envrc`.

```sh
just example <name>         # cargo run --example <name>, from examples/
just test                   # cargo test -q --workspace --lib
just test <name>            # run tests matching <name>
just test <name> --pager    # same, piped through less -R
just push                   # move `main` bookmark to @ and `jj git push`
just update-state           # fetch, rebase @ onto main, keep working-copy contents
```

## Workspace Layout

- `caaqi-core/`: the core, with an explicit API: everything takes `&mut Machine` (or `&Machine` for `timeline()` and `is_running()`). Modules: `machine` (`Machine`, `AlreadyRunning`), `event` (`NotRunning`, `now`, and the crate-private event API: `event`, bootstrapped writes, the stop, run bookkeeping), `time` (`Timeline`, `Time`), `value` (`Value<T>`, `ValueId`, `HistoryEntry`, and the value errors). Operations are inherent `Machine` methods defined in their own module's `impl Machine` block; the state lives in `pub(crate)` fields of `Machine` (`timeline`, `values: Values`, `events: Events`), and `Values` / `Events` are private to the crate; actions and undos reach the state through the machine. Core methods return `Result` and have no panicking versions (the stop is the one exception: see the conventions).
- `caaqi/`: a scoped wrapper over caaqi-core for easier use, depending only on it. `machine::new_machine(program)` takes a `Fn() -> R` program and returns the core `Machine` and a `Value<R>`; each run calls the program inside `scope`, and you call the core's `Machine::run` on it. `machine::scope(&mut machine, || ...)` puts the machine in a scoped thread-local (`scoped-tls-hkt`), for reading results outside a run; `with_machine(f)` borrows it back, takes it out of the thread-local while `f` runs (so a nested call panics instead of aliasing), and is public so other code can build scoped APIs the same way. A stop unwinds cleanly through both, since `scoped-tls-hkt` restores the slot on unwind. Modules mirror the core's: `time::Time` and `value::Value<T>` are `Copy` newtypes over the core's handles with `From` both ways, and their methods are the operations, each panicking method with a `try_` version returning the core's error (errors carry the core's handles). `time` has `now` / `try_now` and the read-only timeline (`first`, `last`, `len`, `is_empty`, `times`; `Time::exists`, `time_cmp`, `next`, `prev`; reading the timeline isn't an event), and `Time` is `PartialOrd` by timeline order. `value` has `create_value` / `try_create_value`, and on `Value`: `get`, `with`, `get_at`, `set`, `update`, `delete`, `write_at` (returns `!`; `try_write_at` returns `Result<Infallible, WriteAtError>`), `version`, `history`. Getters can't return references into the machine, so they copy (`get`, `get_at`, `history`, needing `T: Clone`) or take a closure (`with`, `update`) that runs with the machine out of scope and mustn't call caaqi.
- `examples/`: top-level examples, in the root package (`caaqi-examples`, no library or binary of its own), depending on caaqi. `just example <name>` runs them. For learning the tool, one idea each, in order: `reading` (`get`, `get_at`, `now`, history), `writing` (`write_at` stops the run, and a read at the point doesn't see the write), `wiping` (rule 2: an earlier write wipes later ones, unless the code makes them again) and `settling` (the program checks before writing, or it never settles). Then examples that need something from later in the run: `history` (a shop restocks at its opening, then the stock's history), `layout` (nested boxes drawn in one pass, each writing its measured width into its own past) and `queens` (eight queens by backtracking: a dead end writes into the row above's past, with no search stack). Programs don't print, since they'd print on every run: they return their output, and `main` prints it after `run`.

## The Model

### The rules

A program is a closure the machine runs from the start. The code between machine calls is plain Rust: given the same answers from the machine, it does the same thing. Everything a program observes or changes beyond its own locals goes through the machine, so the machine controls every read and write and can run the program again.

The base behavior is two rules. They have to stay simple enough that anyone can predict a result by playing the program through in their head, without knowing how the machine works:

1. **A read sees the newest write at or before it.**
2. **A write into the past wipes everything after it, and the program continues from there.**

The rest follows from them:

- Changing an input is rule 2, done from the outside.
- A write into the past lasts until something before it is written (bootstrapping). Every rerun puts it back at its point. If an earlier write into the past wipes it, it's gone, unless the code that made it runs again and makes it again.
- A run that writes into the past stops there: the rest of it comes after the write, so it's wiped anyway.
- Code that writes into the past on every run never settles. The program decides whether a write is needed, for example by checking what's already there. That comparison belongs to the program, not the machine.

### Events

Every interaction with the machine is an event at its own point on the timeline: creating, deleting, reading and writing a value, and asking for the current point. An event is an action that returns its undo: creating a value is undone by removing it, a write by removing its history entry, and a read has nothing to undo.

A write into the past becomes a bootstrapped write: an event attached to the event at its point, kept by the machine across runs. Whenever a run executes that event, the machine executes the bootstrapped writes attached to it right after it.

### The machine without steps

The machine without steps defines what a program means: every rerun starts from the beginning. Before a rerun, the machine undoes every event, newest first. Everything before the newest write into the past is identical to the last run, so the rerun's events are matched to the old ones by order, like hooks: each takes the point its old event had, and a created value gets its old id back. That's how bootstrapped writes find their points again. After the newest write into the past, events get new points. Nothing checks that the events match: the machine trusts the program to depend on nothing but the machine (no clocks, globals or randomness).

The program closure is `Fn`, not `FnMut`: state kept in the closure between runs would be a dependency the machine can't see. Its return value is written to a value at the end of each run, which is how the outside gets results.

### Optimizations

Steps come later, as places the machine can restart from, so that less reruns. Skipping work when a rewritten value equals the old one (opt-in `PartialEq` per value) is another. Every optimization must give exactly the same result as the machine without it; the machine without steps is the oracle to test against.

### Terms

- **Timeline**: the linear order of a run. The user should always believe their program ran in normal program order.
- **Time**: a point on the timeline. A `Copy` handle compared in O(1) through order-maintenance labels (`time.rs`).
- **Value**: state inside the timeline, reached through a `Value<T>` handle. Its identity (its creation) and its contents (its writes) are separate. Each write is its own event with its own point, and the value keeps a (Time, snapshot) history, so the value as seen at any point is the newest write at or before it.
- **Event**: an interaction with the machine, at its own point, with an undo (see Events).
- **Machine**: holds the program, the timeline, the events, the bootstrapped writes and the values. There is no type-map: untracked state breaks the time travel.

### What's built

- **Time** (`caaqi-core/src/time.rs`): `Timeline` is an order-maintenance list, a plain data structure with the full API (`push_back` / `push_front`, `insert_after` / `insert_before`, `move_after` / `move_before`, `remove`, `time_cmp`, `next` / `prev`, `contains`, `times`; errors `UnknownTime`, `SameTime`, `MoveTimeError`). Points compare in O(1): each has a private `u64` label, increasing along the list; a new point takes the midpoint between its neighbors' labels, and when no gap is left every label is respread evenly (`Timeline::respread`, kept separate so it can become Bender et al.'s local relabeling). The core `Time` isn't ordered by itself, since comparing needs the timeline. Store `Time` handles, never labels. `Machine::timeline()` hands out `&Timeline` only: only the machine adds and removes points. Handing it out isn't an event (a reference into the machine can't be undone or replayed), and during a run it also shows old points the run hasn't reached yet.
- **Machine** (`machine.rs`): `Machine::new(program)` takes `Fn(&mut Machine) -> R + 'static` and returns `(Machine, Value<R>)`; nothing runs yet. The timeline starts with one point, the program's start, which isn't an event: it's the result value's id. The result is the only value made outside an event (`Values::add_result`), with an empty history until a run writes it; reading it before then fails with `Unwritten`. The stored program (`Rc<dyn Fn(&mut Machine)>`, cloned out before each call) wraps the user's closure and writes its return value to the result, as the run's last event. `run()` returns `Result<usize, AlreadyRunning>`, how many runs it took (`AlreadyRunning` when the program calls it); the crate-private `run_once()` returns `Ran::Settled` or `Ran::Stopped`. `is_running()` tells whether a run is on. A program that writes into the past on every run never settles, and `run` never returns.
- **Events** (`event.rs`): the crate-private `Machine::event(action)` is the one way to make an event. `action: FnOnce(&mut Machine, Time) -> Result<(R, U), E>` gets the machine and its point, and returns its output and its undo `U: FnOnce(&mut Machine) + 'static`. An event made while an action (or a bootstrapped write) runs would be logged before the outer one and break the undo order, so the machine tracks `Events::in_action` and `event` asserts that no action is running; that's a broken internal invariant while the API is crate-private, and needs an error once it's public (TODO in the code). Undos run between runs, where `event` fails with `NotRunning`. The point is the next old one (`Events::old`, with a `matched` cursor) while any are left, or a new one pushed at the end. A failing action isn't an event: a new point is removed and an old one isn't consumed, so failures can't shift the matching. Outside a run, `event` fails with `NotRunning`. Executed events go in `Events::log`, in timeline order, each with its undo and `attached_to` (the event a bootstrapped write is attached to, `None` otherwise). Reads and inspections compute their answer, then record a no-op event (`observation()`); outside a run they aren't events. `now()` is an event that returns its own point. A bootstrapped write lives in one place at a time. Waiting, it's in the queue, `Events::bootstrapped: HashMap<Time, Vec<Bootstrapped>>`, keyed by the point of the event it's attached to, each list in timeline order. When `event` executes an event, it takes that event's list out of the queue and applies each write right after it, at its own fixed point. A `Bootstrapped` is its point and an `FnOnce(&mut Machine) -> Undo`, made by the generic `queued_write(value, point, event, contents)` in `value.rs`: applying consumes it and moves the contents into the value's history, and its undo pops the entry and requeues `queued_write(..., popped snapshot)` in front of its event's list (undos run newest first, so the writes after it are back already). The contents only ever move, so nothing needs `Clone`. Between runs, every key of the queue is an event in the log; after a settled run, applied writes are in the log's undos until the next run undoes them.
- **The stop**: `write_at(value, time, contents)` checks `NotRunning`, `UnknownValue`, `NotInThePast` (`time` must be the point of an event in this run's log, which can be a bootstrapped write), `NotYetCreated` (the value's id, its creation point, is after `time`) and `DeletedValue` (deleted at or before `time`). Then it stores a `Stop { target, make }` (`make` builds the queued write once its point and event are known) and unwinds with `resume_unwind` and the private `StopUnwind` payload, which skips the panic hook. `run_once` catches it with `catch_unwind`; any other payload is resumed after `end_run`, which leaves the machine valid: not running, and as far as the run got. `write_at` returns `Result<Infallible, WriteAtError>`. Each run starts with `undo_last_run()`: undo the log newest first, keeping the points; then apply the pending stop (`write_into_the_past`): rule 2 removes the logged events after the target with their points, drops (takes out of the queue, with their points off the timeline) the bootstrapped writes attached to them and the ones after the target in the target's own list (older ones at the same point, since the new one goes directly after the target), and inserts the new bootstrapped write at a new point right after the target; then the remaining non-bootstrapped points become `old`. So old events are removed only after a stop's target: rerunning a settled machine keeps every point, and every id. `end_run()` drops the old events the run didn't reach, with their bootstrapped writes; a deterministic program always reaches them. Stopping needs `panic = "unwind"`, and a program that catches unwinds itself breaks it.
- **Values** (`value.rs`): `ValueId(Time)` is the creation's point, so a matched creation gives back the same id, and handles held by bootstrapped writes and the outside stay valid across reruns. `Values` is `HashMap<ValueId, Box<dyn Any>>`, each a `Slot<T> { history: Vec<HistoryEntry<T>>, deleted: Option<Time> }`; a handle of the wrong type fails with `UnknownValue`. `create_value(initial)` is one event: its point is the value's id and the time of `initial`, version 0; its undo removes the slot. `write` and `update` each push an entry at their own point (during a run that's always the newest point, so the history stays sorted); their undo pops it. `update` calls its closure before the event, so a panic in it leaves no event. `delete` marks the slot deleted at its point and its undo clears the mark; afterwards `read`, `write`, `update`, `delete` and `version` fail with `DeletedValue`, `read_at` fails at or after the deletion and works before it, and `history` still works. `read` returns the newest snapshot; `read_at(value, time)` binary-searches the history for the newest write at or before `time` (`None` before the first write). Each fallible method returns one error struct or its own enum of them (`ReadError`, `ReadAtError`, `WriteError`, `UpdateError`, `DeleteError`, `VersionError`, `WriteAtError`). A version is the entry's index in the history, not a stored field. That's fine without cutoff, but once cutoff exists, a rerun rewrites histories, so a rewritten write could reuse a version and fool it: versions will need a per-value counter that never goes back.
- **Tests**: core tests reach the run internals (`run_once`, `undo_last_run`, `end_run`, `bootstrapped_at`, `bootstrapped_count`); the layout test (a parent centering a label using the width its child writes into the past) lives in `machine_tests.rs`, and in the crate docs of both crates.

### Next: rewriting only what's affected

The rules above are what the machine without steps does. The next implementation step narrows rule 2 to what a write actually affects. Use these words when explaining how caaqi behaves:

> **Axiom.** An observer sees the present: a read sees the newest write at or before its own point. Looking at the past is explicit (`read_at`).
>
> **Rule.** A write into the past rewrites everything after it that it affects.
>
> **What "affects" means.** Whatever observed something the write changed, then whatever observed those changes, and so on. That includes writes into the past made by affected code, since they exist because that code made them.
>
> **What follows.**
> - A write into the past lasts as long as the code that made it is unaffected and the event it's attached to still happens.
> - Changing an input is a write into the past from outside.
> - A run settles when it makes no write into the past. Whether a program settles is up to the program: guess, check, correct.
> - The machine may rewrite more than what's affected, never less. Rewriting more only costs runs, because unaffected code would make the same writes again.

The machine without steps needs no change to be correct under this rule: it rewrites everything after the write, which is more than what's affected. That only costs runs. The `layout` example takes 2^n runs for n nested boxes, because the outer box's write wipes the inner box's, and the inner box makes the same fix again. The narrower rule would keep the inner box's write, since the inner box never observes the outer box's width. `queens` relies on the clause about writes made by affected code: the rows below restart at column 0 because the dead ends that pushed them forward were affected. The `wiping` example shows both cases: a write by affected code that's gone, and one by unaffected code that's made again.

### Planned

- Rewriting only what's affected (above): track which events observed what, and rewrite only those, instead of everything after the write.

- Inputs from the outside, as writes into the past.
- Steps, then opt-in `PartialEq` cutoff.
- User-defined events (the event API is crate-private for now).
- The outside world: writes to the outside wait until the timeline settles, and what can't be undone (printing, sending a request) is eternal.
- Value history strategies: snapshots vs undo-based.
- Async (reordering).

## Implementation Conventions

- No method may silently do nothing, or return an empty or default result, when its input is invalid (an unknown time or value, a call outside a run, and so on). Check inside the method that relies on the input, return a specific error (defined with `thiserror`), and let callers pass it up with `?`. Don't rely on a caller having validated the input first. Don't add standalone "ensure it exists" helpers either: call the owner's fallible accessor and pass its error up. Panic (`expect` / `assert!`) only when a broken internal invariant makes the input impossible, never for bad caller input. The one exception is user-facing convenience methods: `foo` may panic on bad input when a `try_foo` alongside it returns the error.
- Core methods have no panicking versions, with one exception: a successful `write_at` stops the run by unwinding (`resume_unwind` with the private `StopUnwind` payload, which skips the panic hook, so nothing is printed), and `run_once` catches it. Anything between a `write_at` and that `catch_unwind` (the scoped wrapper's `scope` / `with_machine`, the program's own frames) must survive the unwind, and `run_once` must leave the machine valid when it passes on any other panic. Stopping needs `panic = "unwind"`.
- Events' actions and undos get `&mut Machine` (actions also their point). An action mustn't make another event: `event` asserts it. Run user code (like `update`'s closure) outside the action, so a panic in it leaves no half-made event.
- Keep public APIs explicit: `&mut Machine` in caaqi-core, the scope in caaqi. Do not introduce hook-based APIs (React, Dioxus, Freya hooks) or hook lifecycle abstractions. (Matching a rerun's events to the last run's by order is internal to the machine, not an API.)
- Export commonly needed items through `lib.rs`'s `prelude`.
- Keep changes focused and avoid manually editing generated files.
- Unit tests live in a sibling file named `<module>_tests.rs` (e.g. `time_tests.rs` for `time.rs`), declared at the end of the module with `#[cfg(test)] #[path = "<module>_tests.rs"] mod tests;`. It's still a child module, so it uses `super::*` and can reach private items.

## TODOs

There are different kinds of TODOs; pick the one that matches how urgent it is:

- **`// TODO: ...` comment**: a note for later that nothing forces anyone to act on.
- **`todo!()`**: marks unfinished code. It compiles but panics when that code runs.
- **`compile_error!("TODO: ...")`**: a mandatory TODO. The build fails until someone deals with it and deletes the line. Use it when the user asks for a TODO they must not forget, for example a review they need to do before continuing work. Never add one unless the user asks for it, and never remove one without the user's approval.
