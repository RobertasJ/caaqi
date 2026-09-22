# Project Instructions

## Purpose

caaqi is a Rust library for rewindable, self-adjusting computation with fine-grained reactivity. It models computation as a tree of actions that can be rerun without relying on framework hook systems.

## Domain Language

- **Action**: Code represented by a node in the action tree. An action can create and run sub-actions while it is running. When an action reruns, its descendants are recreated.
- **Action tree**: The runtime hierarchy of actions. Each action has at most one parent and can have multiple sub-actions.
- **Sub-action**: An action created and run by another action while the parent is executing.
- **Self adjustment**: Exiting an action early by returning `Err(SelfAdjust)`. `SelfAdjust` carries a `TrackingId` explaining what caused the early exit.
- **Fine-grained reactivity**: Rerunning actions that track a particular `TrackingId`.
- **Tracked**: The set of `TrackingId`s associated with an action. Use `track`, `untrack`, `tracked`, and `contains_tracked` terminology.
- **Rewinding**: Reversing the effects an action made on state before that action reruns.
- **Compute**: The broader process of evaluating actions and maintaining their relationships, tracked dependencies, and state effects.
- **Hooks**: Avoid hook-based APIs such as React, Dioxus, or Freya hooks. Prefer explicit `ActionContext` operations and the action tree.

## Implementation Conventions

- Preserve the action-tree invariant: a child has one parent, and the parent's `sub_actions` list reflects that relationship.
- Preserve `ActionContext::current_action` while adding nested execution; restore the previous value after an action completes, including self-adjustment paths.
- Use `Result<_, SelfAdjust>` for action execution. `finish()` and `finish_with()` represent successful completion.
- Keep public APIs explicit and context-based; do not introduce hook lifecycle abstractions.
- Keep changes focused and avoid manually editing generated files.

## Commands

Use `jj` instead of `git` for version-control operations, including status, diffs, and history.

Use the repository's `justfile` commands from the workspace root:

```sh
just run
just push
```

Use direct Cargo commands only when there is no corresponding `just` recipe. The Nix development environment is described by `flake.nix` and `.envrc` when available.
