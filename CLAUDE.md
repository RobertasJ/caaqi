# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Purpose

caaqi is a Rust library for rewindable, self-adjusting computation with fine-grained reactivity. It models computation as a tree of actions that can be rerun without relying on framework hook systems.

## Commands

Use `jj` instead of `git` for version-control operations, including status, diffs, and history.

Use the `justfile` recipes from the workspace root; fall back to direct Cargo only when no recipe fits. The Nix dev environment comes from `flake.nix` / `.envrc`.

```sh
just run                    # cargo run — the scratch binary in src/main.rs
just test                   # cargo test -q -p caaqi --lib
just test <name>            # run tests matching <name>
just test <name> --pager    # same, piped through less -R
just push                   # move `main` bookmark to @ and `jj git push`
just update-state           # fetch, rebase @ onto main, keep working-copy contents
```

## Workspace Layout

- `caaqi/` — the library crate (the actual product).
- `src/main.rs` — the `caaqi-testing` binary, a scratch playground that exercises the library. It is not part of the library's API.

## Architecture

A rewrite is in progress; the previous implementation is in `reference/` for consultation only.

**Three layers**, each depending only on the ones before it: context → trace → rewinds. The trace must not import anything from `rewind.rs`.

**`Context` is a type-map of resources** (`context.rs`): one value per `TypeId`, stored as `Box<dyn Any>`. Everything lives in it, including caaqi's own state. Each module defines a resource struct plus an extension trait implemented for `Context`:

- `Trace` + `TraceExt` (`trace.rs`): the trace's shape plus each node's runner, so tree structure and running are one model. `TraceExt` hands out node handles, `NodeRef` / `NodeMut`.
- `TraceIterExt` (`trace_iter.rs`): the multi-step walks over the trace (`ancestors`, `subtree_top_down`, `subtree_bottom_up`, and their `_rev` variants). It uses only the public `TraceExt` API, so it could live in another crate; keep it that way and keep `TraceExt` and its node handles small (single steps, structural mutation and running).
- `Rewinds` + `RewindExt` (`rewind.rs`): rewinds, an optional layer built only on the trace's public API (`NodeKey`, `contains_node` / `node`, the handles).

New subsystems should follow the same pattern: a resource type fetched lazily with `get_or_insert_with`, exposed through an `*Ext` trait. Every public operation on a resource, reads included, is a method of its `*Ext` trait; the resource struct's own methods are private, or `pub(crate)` when another module needs them. `Trace` has no `pub(crate)` items: other modules use only `TraceExt` and the node handles. Do not add fields to `Context`.

**Trace** (`trace.rs`): `nodes: SlotMap<NodeKey, Node>` holds `parent`/`children` links and `prev`/`next` links in trace order: each node before its descendants, siblings first to last. So a node's `prev` is its previous sibling's last descendant, or its parent when it's a first child. Each root's tree has its own order: a root has no `prev`, its tree's last node has no `next`, and two roots' orders never link. Besides the links, a node holds only its runner; other per-node data, rewinds included, belongs in other resources. `TraceExt` has only `contains_node`, `node`, `node_mut` and `create_node` (which takes the node's runner); the handles work like bevy's `EntityRef` / `EntityWorldMut`. `NodeRef` holds `&Context` and reads `parent`, `children`, `prev`, `next` and `is_running`. `NodeMut` holds `&mut Context` and has the same reads plus the writes: `set_parent` / `add_child` (the same operation from either side, attaching as the last child), `detach`, `delete` (leaves only) and `run`. Nodes are created and deleted without notifying anyone. Each write has its own error type: `set_parent` returns `SetParentError`, `add_child` returns `AddChildError`, `delete` returns `DeleteError` (`RunnerInUse` or `HasChildren`), and `run` returns the unit error `RunnerInUse`. The enums compose unit errors (`AlreadyParented`, `SelfParent`, `WouldCycle`, `HasChildren`, `RunnerInUse`); an unknown other key is `UnknownParent` / `UnknownChild`, non-transparent wrappers over `UnknownNode` that say which role the key had. Reparenting is explicit: attaching a node that already has a parent is `AlreadyParented`, so `detach` first. A node moves with its whole subtree: attaching puts the subtree right after the parent's last descendant, and detaching cuts it out into an order of its own. Cycles are rejected by walking up from the parent. A `NodeKey` is only valid in the `Context` that created it. Using a key with another context is unsupported and may refer to an unrelated node.

**Running** (`trace.rs`): every node has a runner, the code that runs it: a `Runner`, which `FnMut(&mut Context, NodeKey)` closures implement. `run` takes it out of the node, calls it with the node's key and puts it back; `Node::runner` is `None` only in between, which is what `is_running` reads. A running node can't be run again or deleted (`RunnerInUse`). A panicking runner is put back before the panic is resumed.

**Rewinds** (`rewind.rs`): anything a run needs undone is registered on a node as a rewind, an `FnOnce(&mut Context, RewindKey)` that receives its own key. `Rewinds` holds `entries: SlotMap<RewindKey, RewindEntry>` (the node plus the closure, which is `None` while it runs) and `by_node: SecondaryMap<NodeKey, SmallVec<[RewindKey; 2]>>`. `RewindExt` on `Context` has `register_rewind(node, f)`, `rewind_keys(node)` (an owned snapshot), `run_rewind(key) -> bool`, `rewind(node)` and `rewind_node_of(key)`; the node-taking methods return `UnknownNode` for a node that isn't in the trace. `run_rewind` takes only the closure out, leaving the entry in place while it runs, then removes the entry through a drop guard, even on panic; it returns `false` for a rewind that's gone or running. `rewind` snapshots the node's keys, shuffles them in debug builds (a xorshift generator in `Rewinds`, seeded from `RandomState`; release keeps the snapshot order), and calls `run_rewind` on each, ignoring `false`. The rules, documented but not enforced:

- The order inside a node is unspecified, and code must not depend on it. To order rewinds, a rewind calls `run_rewind` on another rewind **in the same node**.
- To reach across nodes, call `rewind(other_node)`. Calling `run_rewind` on a rewind of **another** node is strongly discouraged: it leaves that node partially rewound.
- Calling `rewind` on your **own** node from inside one of its rewinds only delays your work until the others have run.
- `rewind` works on a snapshot: rewinds registered on the node while it runs aren't seen, and stay registered.
- A rewind's key is valid while it runs, so it can clean up after itself; it's stale afterwards.
- Rewinds can be registered at any time: on running nodes, and inside other rewinds. Don't let them accumulate: rewind a node before rerunning it.
- Rewind a node before deleting it. The trace doesn't know about rewinds: a deleted node's rewinds are never run, and their storage isn't reclaimed.
- A panic in a rewind consumes only that rewind; the rest stay registered, and calling `rewind` again runs them.

**Per-node data in other resources** is keyed by `NodeKey` in a `SecondaryMap`. Slotmap keys are versioned, so a stale entry never shows up under a new node's key, but a deleted node's entry stays readable under its old key until the slot is reused: check the node exists (`TraceExt::node`) before trusting a lookup.

**Node handles are the extension point.** Other modules add per-node methods through their own extension traits implemented for `NodeRef` / `NodeMut`, the way bevy builds on `EntityWorldMut`. They reach the resources they need through `NodeRef::context`, `NodeMut::context` and `NodeMut::context_mut`. An extension may run nodes through the context, the handle's own included, but must not delete or restructure the handle's node: the handle's methods may panic afterwards.

**Borrowing constraint:** `Context` getters borrow the whole context, so a reference can't be held across another call that needs the context. Drop the borrow first, or `remove` the value and then `insert` it back around the call.

`Id` (`id.rs`) is a global atomic counter.

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
- Export new public items users need through `lib.rs`'s `prelude`.
- Keep changes focused and avoid manually editing generated files.

## TODOs

There are different kinds of TODOs; pick the one that matches how urgent it is:

- **`// TODO: ...` comment**: a note for later that nothing forces anyone to act on.
- **`todo!()`**: marks unfinished code. It compiles but panics when that code runs.
- **`compile_error!("TODO: ...")`**: a mandatory TODO. The build fails until someone deals with it and deletes the line. Use it when the user asks for a TODO they must not forget, for example a review they need to do before continuing work. Never add one unless the user asks for it, and never remove one without the user's approval.
