# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Purpose

caaqi gives programs the illusion of time travel. A program runs like a normal program, top to bottom, but its past can be inspected and later rewritten, and everything after a change plays out again. Under the hood it's an incremental computation engine; time travel is the semantics the programmer reasons about.

The goal is **consistency**: after any change, the end state is exactly what a fresh run from scratch would produce.

caaqi was restarted from scratch around this model. The previous implementation (a trace tree with rewinds, groups, a type-map `Context`, tracked cells and a linear executor) is gone from the tree; jj history is the reference for it.

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

- `caaqi-core/`: the core, with an explicit API: everything takes `&mut Machine` (or `&Machine` to inspect). Modules: `machine` (`Machine`), `time` (`Timeline`, `Time`), `trace` (`run_step`, `StepId`, the `Step` record, `Access`), `value` (`Value<T>`, `ValueId`, `HistoryEntry`). Operations on steps and values are inherent `Machine` methods defined in their own module's `impl Machine` block; the state lives in `pub(crate)` fields of `Machine` (`timeline`, `trace: Trace`, `values: Values`), and `Trace` / `Values` are private to the crate. Core methods return `Result` and have no panicking versions.
- `caaqi/`: a scoped wrapper over caaqi-core for easier use, depending only on it. `machine::scope(&mut machine, || ...)` puts the machine in a scoped thread-local (`scoped-tls-hkt`); `with_machine(f)` borrows it back, takes it out of the thread-local while `f` runs (so a nested call panics instead of aliasing), and is public so other code can build scoped APIs the same way. Modules mirror the core's: `time::Time`, `trace::Step` and `value::Value<T>` are `Copy` newtypes over the core's handles with `From` both ways, and their methods are the operations, each panicking method with a `try_` version returning the core's error (errors carry the core's handles). The timeline is read-only here too (`first`, `last`, `len`, `is_empty`, `times`; `Time::exists`, `time_cmp`, `next`, `prev`), and `Time` is `PartialOrd` by timeline order. Getters can't return references into the machine, so they copy (`get`, `get_at`, `history`, `reads`, all needing `T: Clone` where it applies) or take a closure (`with`, `update`) that runs with the machine out of scope and mustn't call caaqi. `run_step(f)` / `run_step_with_id(f)` run `f` with the machine back in scope.
- `examples/`: top-level examples, in the root package (`caaqi-examples`, no library or binary of its own), depending on both crates. `just example <name>` runs them.

## The Model

### Axes and levels

A program's history lives on axes: **execution time** (when in a run something happened) and **revision time** (when the past was rewritten). Each axis has a capability level, and each level needs the one before it:

1. **Forward only**: the past is gone.
2. **Inspectable**: the past is recorded, append-only, and can be read.
3. **Reversible**: go back to a point, rewrite from there, keep going.
4. **Branching**: going back keeps the old path, and you can switch paths.

caaqi is building execution time up through these levels one at a time. **It is at level 2 now**: everything is recorded, and nothing reruns yet.

### Terms

- **Timeline**: the linear order of a run. The user should always believe their program ran in normal program order.
- **Time**: a point on the timeline. A `Copy` handle compared in O(1) through order-maintenance labels (`time.rs`). At level 2, points are only appended.
- **Step**: the unit of recording, and later of rerunning: everything inside a step is redone together. How much code a step covers is the user's choice (granularity).
- **Section**: the range of the timeline a step's run produced. Steps created while a step runs are inside its section. Nesting exists only as sections, ranges on the line, not as a tree to maintain. It comes from decisions: code whose shape (which steps follow) depends on a value. At level 3, rerunning a step rewrites its section and nothing outside it.
- **Value**: state inside the timeline, reached through a `Value<T>` handle. Every read and write is recorded on the current step: these are the edges of the dependency graph. Outside any step, reads and writes aren't recorded. Each value keeps its own history, a (Time, snapshot) entry per write, so the value as seen at any point is the newest write at or before it. A write's version is its index in the history. Versions let later levels skip reruns cheaply: a step whose inputs' versions didn't change doesn't rerun (early cutoff).
- **Machine**: holds the timeline, the trace (steps, sections, edges) and the values. There is no type-map: nothing untracked can be stored, because untracked state breaks the time travel.

### Level 2: what's built

- **Time** (`caaqi-core/src/time.rs`): `Timeline` is an order-maintenance list, a plain data structure with the full API (`push_back` / `push_front`, `insert_after` / `insert_before`, `move_after` / `move_before`, `remove`, `time_cmp`, `next` / `prev`, `contains`, `times`; errors `UnknownTime`, `SameTime`, `MoveTimeError`). Points compare in O(1): each has a private `u64` label, increasing along the list; a new point takes the midpoint between its neighbors' labels, and when no gap is left every label is respread evenly (`Timeline::respread`, kept separate so it can become Bender et al.'s local relabeling). The core `Time` isn't ordered by itself, since comparing needs the timeline. Store `Time` handles, never labels. `Machine::timeline()` hands out `&Timeline` only: only the machine adds points, so steps' and writes' points are never moved or removed.
- **Trace** (`trace.rs`), append-only: `Machine::run_step(f)` pushes the step's start point, makes it the current step (`current_step()`, a stack of running steps), runs `f`, then pushes its end point and returns a `Value<R>` (`R: 'static`), not the step's id: inside `f` the id is `current_step()`, and `run_step_with_id(f)` returns `(StepId, Value<R>)` when it's needed outside. What `f` returned is stored as a new value, created after the section closes, so its point is outside the section and its creation is recorded as a write on the parent step. A panic in `f` still closes the section, through `catch_unwind`, before resuming, and creates no result value. The `Step` record (`machine.step(id)`, `UnknownStep`) has `parent`, `start`, `end` (`None` while running), `is_running`, and the step's own `reads` / `writes` (not its children's): sets of `Access { value: ValueId, version }`, ordered by value then version. `steps()` lists steps in start order, which is timeline order.
- **Values** (`value.rs`): `create_value(initial)` writes version 0 at a new point; `write` / `update` push a new point and a new version each, so every write has its own time and the history (`Vec<HistoryEntry<T>>` of time and snapshot, behind `Box<dyn Any>` per value) stays sorted. A version is the entry's index in the history, not a stored field: that holds only while the history is append-only, so level 3's truncation will need a stored version from a per-value counter that never goes back, or a rewritten write could reuse a version and fool early cutoff. `read` returns the newest snapshot; `read_at(value, time)` binary-searches the history for the newest write at or before `time` (`None` before the value was created). Recording rules: `read`, `read_at` (when it finds a write) and `update` (which reads the version it copies) record reads; `create_value`, `write` and `update` record writes; `version` and `history` are inspection and are never recorded. Using a handle from another machine fails with `UnknownValue`, including when the slot holds a different type.

### Planned

The direction, not built yet:

- **Level 3**: rerunning a step truncates and rewrites its section. Pending steps are settled earliest first, so a step never runs before its past is final: that's what keeps up the illusion of normal program order.
- **Value history strategies**: how values keep their past becomes a trait. Snapshots give cheap reads at any point but cost memory; undo-based history uses little memory, but reading earlier means undoing later writes, whose writers then rerun.
- **Rewinds**: backward-going cleanup for a step's run (like `Drop` for a run). When a section is rewritten, its rewinds run newest first.
- **The outside world**, which can't time travel: reads from outside become recorded inputs; writes to outside are deferred until the timeline settles and applied in time order, by reconciling to the final state or with a stack of rewinds per resource. What can't be undone (printing, sending a request) is **eternal**: it happens once and is never undone.
- **Later ideas**: mundane values (move forward with time) vs tesseract values (one value across a stretch of time); mail sent to an earlier point (D-Mail rules: delivered mail stays as input); branching and caching decisions (level 4).

## Implementation Conventions

- No method may silently do nothing, or return an empty or default result, when its input is invalid (an unknown time, step or value, and so on). Check inside the method that relies on the input, return a specific error (defined with `thiserror`), and let callers pass it up with `?`. Don't rely on a caller having validated the input first. Don't add standalone "ensure it exists" helpers either: call the owner's fallible accessor and pass its error up. Panic (`expect` / `assert!`) only when a broken internal invariant makes the input impossible, never for bad caller input. The one exception is user-facing convenience methods: `foo` may panic on bad input when a `try_foo` alongside it returns the error.
- Keep public APIs explicit: `&mut Machine` in caaqi-core, the scope in caaqi. Do not introduce hook-based APIs (React, Dioxus, Freya hooks) or hook lifecycle abstractions.
- Export commonly needed items through `lib.rs`'s `prelude`.
- Keep changes focused and avoid manually editing generated files.
- Unit tests live in a sibling file named `<module>_tests.rs` (e.g. `time_tests.rs` for `time.rs`), declared at the end of the module with `#[cfg(test)] #[path = "<module>_tests.rs"] mod tests;`. It's still a child module, so it uses `super::*` and can reach private items.

## TODOs

There are different kinds of TODOs; pick the one that matches how urgent it is:

- **`// TODO: ...` comment**: a note for later that nothing forces anyone to act on.
- **`todo!()`**: marks unfinished code. It compiles but panics when that code runs.
- **`compile_error!("TODO: ...")`**: a mandatory TODO. The build fails until someone deals with it and deletes the line. Use it when the user asks for a TODO they must not forget, for example a review they need to do before continuing work. Never add one unless the user asks for it, and never remove one without the user's approval.
