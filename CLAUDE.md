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

**`Context` is a type-map of resources** (`context.rs`): one value per `TypeId`, stored as `Box<dyn Any>`. Everything lives in it, including caaqi's own state. Each module defines a resource struct plus an extension trait implemented for `Context`:

- `ActionTree` + `ActionTreeExt` (`action_tree.rs`): the shape of the action tree, and nothing else.
- `CurrentAction` + `CurrentActionExt` (`current.rs`): which action is executing.
- `NodeObservers` + `LifecycleExt` (`lifecycle.rs`): subsystems registered to hear about nodes being added to or removed from the tree.
- `Groups` + `GroupingExt` (`grouping.rs`): sets of action nodes, each identified by a `GroupId`.

New subsystems should follow the same pattern: a resource type fetched lazily with `get_or_insert_with`, exposed through an `*Ext` trait. Every public operation on a resource, reads included, is a method of its `*Ext` trait; the resource struct's own methods are private, or `pub(crate)` when another module needs them (such as `ActionTree::node`). Do not add fields to `Context`.

**Action tree** (`action_tree.rs`): `nodes: SlotMap<ActionNodeKey, ActionNode>` holds only parent/`sub_actions` links. Per-node data belongs in other resources. Mutation (`create_root`, `create_child`, `clear_children`, `remove_node`) goes through `ActionTreeExt` so observers are notified; removal returns the removed keys bottom up.

**Action storage** (`action.rs`): `ActionStorage<O>` is one resource per output type `O`, holding a `BoxAction<O>` per `ActionNodeKey`. It registers itself as a `NodeObserver` so removed nodes drop their action. `create_root_action` / `create_child_action` create the tree node and store the action together, so storing always goes with a tree relationship. `run_action::<O>(key)` reruns a stored action: the caller names the output type, and `NoStoredAction` means that key has no action with output `O`, possibly because it stores one with a different output. A run clears the node's children first, takes the action out of storage while it runs, and puts it back afterwards, even on panic. `run_node` still runs an action once without storing it. Any `FnMut(&mut Context) -> T` is an `Action`; `Context::run` runs one inline without creating a node.

- **Executing** means "the current action or one of its ancestors". `remove_node` refuses executing nodes, and `clear_children` refuses nodes with an executing descendant.
- `with_current_action` uses `catch_unwind` / `resume_unwind`, so `CurrentAction` is restored even when an action panics.

**Lifecycle** (`lifecycle.rs`): a type implementing `NodeObserver` (usually the resource itself) gets `node_added` / `nodes_removed` callbacks after the tree changes. Register it with `observe_nodes::<T>()`; registration is keyed by type, so repeat calls are no-ops. `Groups` registers itself the first time its resource is created.

**Per-node data in other resources** is keyed by `ActionNodeKey` in a `SecondaryMap`. Keep it in sync by implementing `NodeObserver`. Stale entries are harmless anyway because slotmap keys are versioned.

**Grouping** (`grouping.rs`): `create_group`, `remove_group`, `add_to_group`, and `remove_from_group`. Membership is stored both ways, so a removed node leaves all its groups cheaply. `add_to_group` rejects nodes that aren't in the tree.

**Borrowing constraint:** `Context` getters borrow the whole context, so a reference can't be held across a nested run. Drop the borrow first, or `remove` the value and then `insert` it back around the nested call.

`Id` (`id.rs`) is a global atomic counter. `GroupId` wraps it.

## Domain Language

- **Action**: Code represented by a node in the action tree. An action can create and run sub-actions while it is running. When an action reruns, its descendants are recreated.
- **Action tree**: The runtime hierarchy of actions. Each action has at most one parent and can have multiple sub-actions.
- **Sub-action**: An action created and run by another action while the parent is executing.
- **Self adjustment** (planned, not yet implemented): Exiting an action early by returning `Err(SelfAdjust)`. `SelfAdjust` carries a `GroupId` explaining what caused the early exit.
- **Fine-grained reactivity** (planned): Rerunning the actions in a particular group.
- **Group**: A set of action nodes identified by a `GroupId`. Use `create_group`, `remove_group`, `add_to_group`, and `remove_from_group` terminology.
- **Rewinding**: Reversing the effects an action made on state before that action reruns.
- **Compute**: The broader process of evaluating actions and maintaining their relationships, tracked dependencies, and state effects.
- **Hooks**: Avoid hook-based APIs such as React, Dioxus, or Freya hooks. Prefer explicit `Context` operations and the action tree.

## Implementation Conventions

- Preserve the action-tree invariant: a child has one parent, and the parent's `sub_actions` list reflects that relationship. Only freshly inserted nodes get parented, which is what prevents cycles.
- Preserve the current action during nested execution by going through `with_current_action`, which restores the previous value on every exit path (including panics and, later, self-adjustment).
- No method may silently do nothing, or return an empty or default result, when its input is invalid (an unknown node key, an unknown group, and so on). Check inside the method that relies on the input, return a specific error, and let callers pass it up with `?`. Don't rely on a caller having validated the input first. Don't add standalone "ensure it exists" helpers either. Code in another module calls the owning resource's fallible accessor (such as `ActionTree::node`) and passes its error up with `?`. Panic (`expect` / `assert!`) only when a broken internal invariant makes the input impossible, never for bad caller input. The one exception is user-facing convenience methods: `foo` may panic on bad input when a `foo_checked` alongside it returns the error (for example `track` / `track_checked`).
- Keep public APIs explicit and context-based. Do not introduce hook lifecycle abstractions.
- Export new public items users need through `lib.rs`'s `prelude`.
- Keep changes focused and avoid manually editing generated files.

## TODOs

There are different kinds of TODOs; pick the one that matches how urgent it is:

- **`// TODO: ...` comment**: a note for later that nothing forces anyone to act on.
- **`todo!()`**: marks unfinished code. It compiles but panics when that code runs.
- **`compile_error!("TODO: ...")`**: a mandatory TODO. The build fails until someone deals with it and deletes the line. Use it when the user asks for a TODO they must not forget, for example a review they need to do before continuing work. Never add one unless the user asks for it, and never remove one without the user's approval.
