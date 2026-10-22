# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Purpose

caaqi is a Rust library for rewindable computation. A computation is a trace: a tree of actions, each of which can undo its own effects (rewind) and run again. On top of that, an executor decides what reruns and when. The long-term idea is something like 5D Chess with Multiverse Time Travel: an executor that can move a computation back to earlier points and rerun it from there, far beyond rerunning whatever changed.

It's built in three layers, each depending only on the one before:

- **caaqi-core**: the trace, rewinds, groups and storage, all kept in an explicit `Context`.
- **caaqi-core-scoped**: the same API with the context in a scoped thread-local, so code doesn't pass it around.
- **caaqi**: the executor layer. For now it has tracked cells and a linear executor, which give fine-grained reactivity: a write to a cell reruns the actions that read it. Self-adjustment (actions exiting early and being rerun) is planned.

Everything uses explicit context operations, never framework hook systems.

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

- `caaqi-core/` — the core library: the context, trace, rewinds, groups, current and storage (see Architecture).
- `caaqi-core-scoped/` — caaqi-core with a scoped context: `scope(&mut ctx, f)` puts a context in a scoped thread-local (`scoped-tls-hkt`) and is the only function that takes one; `context::with_ctx(f)` borrows it back, and is public so other code can build scoped APIs the same way. Its modules mirror caaqi-core's (`context`, `trace`, `trace_iter`, `rewind`, `group`, `current`, `storage`) with the same names: each `*Ext` method on `Context` is a free function, nodes are `trace::TraceNode`, a `Copy` newtype over caaqi-core's `NodeKey` with `From` both ways, whose inherent methods are the node's operations in place of the `NodeRef` / `NodeMut` handles (`create_node`, walks, `current_node` and group members return `TraceNode`s; the methods panic if the node was deleted, `exists()` checks and replaces `contains_node`, there's no `node`/`node_mut`, and `trace_cmp(other)` replaces `cmp_nodes`; `TraceNode` is `PartialOrd` by trace order (`None` across trees, panics for a deleted node) and not `Ord`, unlike `NodeKey`, whose `cmp` / `<` compare slots; errors still carry `NodeKey`s), `rewind::NodeRewindExt` is implemented for `TraceNode` too, `group::Group` is a `Copy` newtype over `GroupId` with `From` both ways, whose methods (`exists`, `contains`, `members`, `insert`, `remove`, `delete`) return caaqi-core's errors, while the node side (`group_count`, `remove_from_all_groups`) is `TraceNode` methods that panic for a deleted node (`create_group` stays a function), `storage::Stored<T>` is a `Copy` newtype over caaqi-core's with `From` both ways and `get` / `get_mut` / `remove` as methods (`store` stays a function), runner and rewind traits take no context (`RunnerWithNode` is `FnMut(TraceNode)`, `Runner` is `FnMut()`, and so on) and are run with the context back in scope: `RunnerWithNode` / `RewindWithKey` are conversions to caaqi-core's runner / rewind (`into_runner` / `into_rewind`), and `WithCurrent` converts to caaqi-core's `WithCurrent`, since caaqi-core's `with_current_*` are private, getters returning references take a closure instead (which mustn't call the crate), and walks other than `subtree_top_down` collect eagerly. Every function goes through `with_ctx`, which takes the context out of the thread-local while its closure runs, so a nested call panics instead of aliasing; `scope(ctx, ..)` inside the closure puts it back. Keep it covering all of caaqi-core's public API.
- `caaqi/` — the top layer, built only on `caaqi-core-scoped` (no direct `caaqi-core` dependency), so its API is scoped too. It's an executor that will be generalized a lot further; the idea is something like 5D Chess with Multiverse Time Travel. For now it has cells and a linear executor. Its API uses free functions and `Copy` handles with methods, no `Context` parameters. Cells (`StoredCell`, `TrackedCell`) hold an `Rc<RefCell<T>>` in the context, and borrowing (`borrow` / `borrow_mut` on `StoredCell`, `read` / `write` on `TrackedCell`, each with a `try_` version) returns `CellRef` / `CellRefMut`, `'static` guards built with `ouroboros` that own an `Rc` clone and deref to the value, so a borrow doesn't hold the context. On `TrackedCell`, `read` is `silent_read` plus `subscribe` (the current node becomes a reader until rewound) and `write` is `silent_write` plus `notify` (the readers are queued on the linear executor); `set` replaces and notifies, and `set_if_changed` notifies only when the value differs. `remove` works while borrowed: new borrows fail with `UnknownStored`, and the value is dropped when the last existing borrow ends. Methods that panic on bad input have a `try_` version that returns the error.
  - The linear executor (`caaqi/src/linear_executor.rs`) runs queued nodes in trace order. `NodeExecutionExt` on `TraceNode` has `queue_execution` / `try_queue_execution` (`QueueExecutionError` is `UnknownNode` or `DifferentTrees`: all queued nodes must share one tree) and `unqueue_execution`; `execute_queued` rewinds and runs each queued node, round after round until the queue is empty. A node queued while executing runs in the same round if it comes after the running node in trace order, and in the next round otherwise. Queuing a node that's already queued or running does nothing. The executor never checks for deleted nodes, so **anything that deletes a node calls `unqueue_execution` on it first, after rewinding it** (`node.rewind(); node.unqueue_execution(); node.delete()`), usually in the rewind that deletes it. Every caaqi deletion path must follow this rule. A deleted node left in the queue makes the executor panic the next time it compares nodes.
  - The timeline (`caaqi/src/time.rs`) is an order-maintenance list, one per context: `Time` points that compare in O(1) and can be inserted between any two existing points, so other code can keep things sorted by time and binary-search them. Free functions `push_back` / `push_front` / `first` / `last` / `len` / `is_empty` / `times`; `Time` methods `exists`, `insert_after` / `insert_before`, `move_after` / `move_before` (the handle stays the same and the point gets a new position; moving relative to itself is `SameTime`), `remove`, `time_cmp`, `next` / `prev`, each with a `try_` version (`UnknownTime`, `MoveTimeError`). `Time` is `PartialOrd` by timeline order, not `Ord`: moving a point changes its order. Each point has a private `u64` label, increasing along the list. A new point takes the midpoint between its neighbors' labels, and when no gap is left, every label is respread evenly (`Timeline::respread`, kept separate so it can become Bender et al.'s local relabeling). Store `Time` handles, never labels.

## Architecture

This section describes `caaqi-core`. A rewrite is in progress; the previous implementation is in `reference/` for consultation only.

**Layers**, each depending only on the ones before it: context → trace → rewinds → groups / current. The trace must not import anything from `rewind.rs`, and no other layer imports from `current.rs`. All modules are at the crate's top level (`caaqi_core::trace`, `caaqi_core::rewind`, …); `caaqi_core::prelude` exports `Context`, the trace and rewind basics (`NodeKey`, `TraceExt`, `RunnerWithNode`, `RewindKey`, `RewindExt`, `NodeRewindExt`, `RewindWithKey`) and the current layer (`CurrentExt`, `Runner`, `Rewind`, `WithCurrent`).

**`Context` is a type-map of resources** (`context.rs`): one value per `TypeId`, stored as `Box<dyn Any>`. Everything lives in it, including caaqi-core's own state. Each module defines a resource struct plus an extension trait implemented for `Context`:

- `Trace` + `TraceExt` (`trace.rs`): the trace's shape plus each node's runner, so tree structure and running are one model. `TraceExt` hands out node handles, `NodeRef` / `NodeMut`.
- `TraceIterExt` (`trace_iter.rs`): the multi-step walks over the trace (`ancestors`, `subtree_top_down`, `subtree_bottom_up`, and their `_rev` variants). It uses only the public `TraceExt` API, so it could live in another crate; keep it that way and keep `TraceExt` and its node handles small (single steps, structural mutation and running).
- `Rewinds` + `RewindExt` (`rewind.rs`): rewinds, an optional layer built only on the trace's public API (`NodeKey`, `contains_node` / `node`, the handles).
- `Groups` + `GroupingExt` (`group.rs`): groups, sets of nodes identified by `GroupId`, an optional layer built on the trace and rewinds, with manual cleanup.
- `Current` + `CurrentExt` (`current.rs`): runners and rewinds that read their own key from the context, built only on the trace's and rewinds' public API.
- `StorageKeys` / `Storage<T>` + `StorageExt` (`storage.rs`): values of any type kept in the context, reached through `Stored<T>` handles (`Copy` whatever `T` is), built only on the context. Accessing a removed value fails with `UnknownStored`.

New subsystems should follow the same pattern: a resource type fetched lazily with `get_or_insert_with`, exposed through an `*Ext` trait. Every public operation on a resource, reads included, is a method of its `*Ext` trait; the resource struct itself is private to its module, and so are its methods, or `pub(crate)` when another module needs them. `Trace` has no `pub(crate)` items: other modules use only `TraceExt` and the node handles. Do not add fields to `Context`.

**Trace** (`trace.rs`): `nodes: SlotMap<NodeKey, Node>` holds `parent`/`children` links and `prev`/`next` links in trace order: each node before its descendants, siblings first to last. So a node's `prev` is its previous sibling's last descendant, or its parent when it's a first child. Each root's tree has its own order: a root has no `prev`, its tree's last node has no `next`, and two roots' orders never link. Besides the links, a node holds only its runner; other per-node data, rewinds included, belongs in other resources. `TraceExt` has only `contains_node`, `node`, `node_mut`, `create_node` (which takes the node's runner) and `cmp_nodes` (two nodes' trace order as an `Ordering`; `CmpNodesError` is `UnknownNode` or `DifferentTrees`, since separate trees have no order between them); the handles work like bevy's `EntityRef` / `EntityWorldMut`. `NodeRef` holds `&Context` and reads `parent`, `children`, `prev`, `next` and `is_running`. `NodeMut` holds `&mut Context` and has the same reads plus the writes: `set_parent` / `add_child` (the same operation from either side, attaching as the last child), `detach`, `delete` (leaves only) and `run`. Nodes are created and deleted without notifying anyone. Each write has its own error type: `set_parent` returns `SetParentError`, `add_child` returns `AddChildError`, `delete` returns `DeleteError` (`RunnerInUse` or `HasChildren`), and `run` returns the unit error `RunnerInUse`. The enums compose unit errors (`AlreadyParented`, `SelfParent`, `WouldCycle`, `HasChildren`, `RunnerInUse`); an unknown other key is `UnknownParent` / `UnknownChild`, non-transparent wrappers over `UnknownNode` that say which role the key had. Reparenting is explicit: attaching a node that already has a parent is `AlreadyParented`, so `detach` first. A node moves with its whole subtree: attaching puts the subtree right after the parent's last descendant, and detaching cuts it out into an order of its own. Cycles are rejected by walking up from the parent. A `NodeKey` is only valid in the `Context` that created it. Using a key with another context is unsupported and may refer to an unrelated node.

**Running** (`trace.rs`): every node has a runner, the code that runs it: a `RunnerWithNode`, which `FnMut(&mut Context, NodeKey)` closures implement. `run` takes it out of the node, calls it with the node's key and puts it back; `Node::runner` is `None` only in between, which is what `is_running` reads. A running node can't be run again or deleted (`RunnerInUse`). A panicking runner is put back before the panic is resumed.

**Rewinds** (`rewind.rs`): anything a run needs undone is registered on a node as a rewind, a `RewindWithKey` (which `FnOnce(&mut Context, RewindKey)` closures implement; its method takes `self: Box<Self>`) that receives its own key. `Rewinds` holds `entries: SlotMap<RewindKey, RewindEntry>` (the node plus the boxed rewind, which is `None` while it runs) and `by_node: SecondaryMap<NodeKey, SmallVec<[RewindKey; 2]>>`. `RewindExt` on `Context` has `register_rewind(node, f)`, `rewind_keys(node)` (an iterator borrowing the context), `run_rewind(key) -> bool` and `rewind_node_of(key)`; the node-taking methods return `UnknownNode` for a node that isn't in the trace. Rewinding a whole node is `NodeRewindExt::rewind` on the handle, `ctx.node_mut(node)?.rewind()`: it takes `&mut self`, so the same handle can run the node afterwards, and can't fail. `run_rewind` takes only the closure out, leaving the entry in place while it runs, then removes the entry through a drop guard, even on panic; it returns `false` for a rewind that's gone or running. `rewind` snapshots the node's keys, shuffles them in debug builds (a xorshift generator in `Rewinds`, seeded from `RandomState`; release keeps the snapshot order), and calls `run_rewind` on each, ignoring `false`. The rules, documented but not enforced:

- The order inside a node is unspecified, and code must not depend on it. To order rewinds, a rewind calls `run_rewind` on another rewind **in the same node**.
- To reach across nodes, call `ctx.node_mut(other_node)?.rewind()`. Calling `run_rewind` on a rewind of **another** node is strongly discouraged: it leaves that node partially rewound.
- Calling `rewind` on your **own** node from inside one of its rewinds only delays your work until the others have run.
- `rewind` works on a snapshot: rewinds registered on the node while it runs aren't seen, and stay registered.
- A rewind's key is valid while it runs, so it can clean up after itself; it's stale afterwards.
- Rewinds can be registered at any time: on running nodes, and inside other rewinds. Don't let them accumulate: rewind a node before rerunning it.
- A rewind must not delete its own node: the handle rewinding it may panic afterwards.
- Rewind a node before deleting it. The trace doesn't know about rewinds: a deleted node's rewinds are never run, and their storage isn't reclaimed.
- A panic in a rewind consumes only that rewind; the rest stay registered, and calling `rewind` again runs them.

**Groups** (`group.rs`): `Groups` stores membership both ways, `members: SlotMap<GroupId, HashSet<NodeKey>>` and `memberships: SecondaryMap<NodeKey, HashSet<GroupId>>`, which mirror each other exactly for nodes in the trace. `GroupingExt` on `Context` has `group_exists`, `group_contains`, `group_members`, `group_count`, `create_group`, `remove_group`, `add_to_group`, `remove_from_group` and `remove_from_all_groups`. A group is reachable only through its `GroupId`: nothing public returns the ids of a node's groups, so node-side operations (`group_count`, `remove_from_all_groups`) act on all of a node's groups without naming them. Keep it that way; `memberships` exists for those operations. `GroupId` is a slotmap key, like `NodeKey` only valid in the `Context` that created it. Cleanup is manual: groups don't track node deletion, and nothing removes a deleted node from its groups. `add_to_group` refuses a node that isn't in the trace (`UnknownNode`) at the time of the call, and `group_contains`, `group_count` and `remove_from_all_groups` do too, but `group_members` and `remove_group` return what's stored, stale keys included, and `remove_from_group` accepts stale keys (it fails only with `UnknownGroup`). A deleted node's slot may be reused, replacing its `memberships` entry, so removing a stale key tolerates a missing one. `add_to_group` memberships last until removed.

**Current** (`current.rs`): `Runner` is an `FnMut(&mut Context)` and `Rewind` an `FnOnce(&mut Context)` (its method takes `self`): code that isn't given its node's or rewind's key. Wrapping one in `WithCurrent(…)` gives a `RunnerWithNode` / `RewindWithKey` that pushes its key onto the `Current` resource's stacks (`nodes`, `rewinds`), runs the inner code and pops the key through a drop guard, even on panic. `CurrentExt` on `Context` has `current_node()` and `current_rewind()`, which return the innermost key, or `None` outside of one. Only `WithCurrent` pushes keys, through the private `with_current_node` / `with_current_rewind`, so only a running node or rewind can be current; keep them private. Runners and rewinds that aren't wrapped don't touch the stacks, and the two stacks are independent: a rewind run inside a node sees both.

Because `create_node` and `register_rewind` take `impl RunnerWithNode` / `impl RewindWithKey`, closures passed to them need their parameter types annotated (`|ctx: &mut Context, _|`).

**Per-node data in other resources** is keyed by `NodeKey` in a `SecondaryMap`. Slotmap keys are versioned, so a stale entry never shows up under a new node's key, but a deleted node's entry stays readable under its old key until the slot is reused: check the node exists (`TraceExt::node`) before trusting a lookup.

**Node handles are the extension point.** Other modules add per-node methods through their own extension traits implemented for `NodeRef` / `NodeMut`, the way bevy builds on `EntityWorldMut`. They reach the resources they need through `NodeRef::context`, `NodeMut::context` and `NodeMut::context_mut`. An extension may run nodes through the context, the handle's own included, but must not delete or restructure the handle's node: the handle's methods may panic afterwards.

**Borrowing constraint:** `Context` getters borrow the whole context, so a reference can't be held across another call that needs the context. Drop the borrow first, or use `Context::resource_scope` / `try_resource_scope` (like bevy's `World::resource_scope`): it takes the value out, calls a closure with the context and the value, and puts the value back, even on panic.

## Domain Language

Actions, rewinding and reactivity are being redesigned, so the descriptions below may not match the code yet.

- **Action**: Code represented by a node in the trace. An action can create and run sub-actions while it is running. When an action reruns, its descendants are recreated.
- **Trace**: The runtime hierarchy of actions. Each action has at most one parent and can have multiple sub-actions. Its nodes are also linked in trace order (parents before children).
- **Sub-action**: An action created and run by another action while the parent is executing.
- **Self adjustment** (planned, not yet implemented): Exiting an action early by returning `Err(SelfAdjust)`. `SelfAdjust` carries a `GroupId` explaining what caused the early exit.
- **Fine-grained reactivity** (planned): Rerunning the actions in a particular group.
- **Group**: A set of action nodes identified by a `GroupId`. Use `create_group`, `remove_group`, `add_to_group`, and `remove_from_group` terminology.
- **Rewinding**: Reversing the effects an action made on state before that action reruns.
- **Compute**: The broader process of evaluating actions and maintaining their relationships, tracked dependencies, and state effects.
- **Hooks**: Avoid hook-based APIs such as React, Dioxus, or Freya hooks. Prefer explicit `Context` operations and the trace.

## Implementation Conventions

- Preserve the trace invariants: a child has one parent, the parent's `children` list reflects that relationship, and `prev`/`next` follow trace order. Attaching rejects nodes that already have a parent and any parent inside the child's own subtree, which is what prevents cycles.
- No method may silently do nothing, or return an empty or default result, when its input is invalid (an unknown node key, an unknown group, and so on). Check inside the method that relies on the input, return a specific error, and let callers pass it up with `?`. Don't rely on a caller having validated the input first. Don't add standalone "ensure it exists" helpers either. Code in another module calls the owning resource's fallible accessor (for a node key, `TraceExt::node` or `TraceExt::node_mut`) and passes its error up with `?`. Panic (`expect` / `assert!`) only when a broken internal invariant makes the input impossible, never for bad caller input. The one exception is user-facing convenience methods: `foo` may panic on bad input when a `foo_checked` alongside it returns the error.
- Keep public APIs explicit and context-based. Do not introduce hook lifecycle abstractions.
- Export commonly needed items through `lib.rs`'s `prelude`.
- Keep changes focused and avoid manually editing generated files.
- Unit tests live in a sibling file named `<module>_tests.rs` (e.g. `group_tests.rs` for `group.rs`), declared at the end of the module with `#[cfg(test)] #[path = "<module>_tests.rs"] mod tests;`. It's still a child module, so it uses `super::*` and can reach private items.

## TODOs

There are different kinds of TODOs; pick the one that matches how urgent it is:

- **`// TODO: ...` comment**: a note for later that nothing forces anyone to act on.
- **`todo!()`**: marks unfinished code. It compiles but panics when that code runs.
- **`compile_error!("TODO: ...")`**: a mandatory TODO. The build fails until someone deals with it and deletes the line. Use it when the user asks for a TODO they must not forget, for example a review they need to do before continuing work. Never add one unless the user asks for it, and never remove one without the user's approval.
