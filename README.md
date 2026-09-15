# caaqi

caaqi is an experimental self-adjusting computation framework built on Bevy ECS. It tracks dependencies between values and actions, then reruns affected code when those values change. The examples explore using this model for reactive UI.

The API and execution model are still evolving. The examples and tests cover a growing set of behaviors, but untested cases may expose bugs. This is a project to explore and experiment with, rather than a finished framework.

## Examples

- [Reactive UI](examples/simple_reactive_ui.rs) — UI properties and structure responding to hover state.
- [Basic reactivity](examples/simple_reactivity.rs) — tracked reads, writes, and action reexecution.
- [Variables](examples/synced_value.rs) — sequential mutations across reruns and changing branches. The filename still uses the earlier terminology.
- [Tests](bevy_caaqi/src/tests.rs) — concrete assertions about values, execution order, and rewinding.

The examples include the Bevy application setup. The snippets below assume these imports and an application with `CaaqiPlugin` installed:

```rust
use bevy_caaqi::{
    CaaqiPlugin,
    action::context_builder::{action, defer_action_eval},
    tracked_value::ref_,
    var::var,
};
```

World access is currently explicit. Removing the repeated `world` argument is a planned ergonomic improvement.

## Actions and reactivity

An `action` is rerunnable code. Actions execute immediately in source order and may contain nested actions.

Reading a ref records a dependency for the current action. Writing to that ref notifies its readers, scheduling them for reexecution. Before an affected action runs again, caaqi rewinds its previous execution and removes its old children.

```rust
defer_action_eval(commands, move |world| {
    let mut count = ref_(world, 0);

    action(world, move |world| {
        println!("Count: {}", *count.read(&mut *world));
    });

    action(world, move |world| {
        for _ in 0..10 {
            *count.write(&mut *world) += 1;
        }
    });
});
```

Expected output:

```text
Count: 0
Count: 10
```

The reader runs initially, then runs again after the later action changes `count`. Calling `Ref::write()` does not itself subscribe the writer, so the incrementing action does not schedule itself again in this example.

## Refs and vars

Refs and vars serve different purposes:

| Type | Behavior |
| --- | --- |
| `Ref<T>` | Holds state that drives adjustment. Writes notify readers and are not automatically undone when the writing action rewinds. |
| `Var<T>` | Preserves ordinary sequential variable behavior across action reruns. Writes register rollback state, and accesses synchronize with the surrounding execution. |

The intended invariant for a var is that each access observes the value a fresh execution of the action root would produce up to that point.

```rust
defer_action_eval(commands, move |world| {
    let mut numbers = var(world, Vec::<i32>::new());

    action(world, move |world| {
        numbers.write(world).push(1);
    });

    action(world, move |world| {
        println!("{:?}", *numbers.read(world));
    });
});
```

If the first action reruns, its previous push is undone before the new execution. If a branch containing that action disappears, the push is removed as well. Previous executions should not leave accumulated entries behind.

Use vars for mutations that should behave this way. The lower-level synchronization APIs support this behavior, but most application code should not need to use them directly. Preserving familiar variable semantics is one of the more involved parts of the implementation.

## Usage rules

### Adjustments must converge

Execution continues until no actions need to run again. An action that reads a ref and unconditionally writes to it on every execution can keep scheduling itself indefinitely.

Reading and writing the same ref is useful when the write eventually stops:

```rust
action(world, move |world| {
    let value = *count.read(&mut *world);

    if value < 5 {
        *count.write(&mut *world) = value + 1;
    }
});
```

### Writes notify even when the value is unchanged

`Ref::write()` emits a notification without checking equality. If notifications should depend on a value change, perform that check explicitly:

```rust
action(world, move |world| {
    let previous = *count.silent_read(&mut *world);

    if previous != 5 {
        *count.write(&mut *world) = 5;
    }
});
```

Refs provide `silent_read()`, `silent_write()`, `subscribe()`, and `notify()` for manual control. Silent access neither records a dependency nor emits a notification. In the example above, the silent read deliberately does not subscribe this action to future changes in `count`.

Forward-only notification methods are used internally by vars and should generally be left to those abstractions.

### Keep value access within action execution

For normal use, read and write refs and vars inside actions. Event handlers require additional flushing and scheduling; the reactive UI example demonstrates that integration.

### Allocations belong to the execution that creates them

Values allocated with `ref_()` and `var()` are dropped when the action execution that created them is rewound. Reexecution creates new allocations. Both types have copyable handles, but copying a handle does not extend the allocation's lifetime.

### External side effects need explicit cleanup

caaqi does not automatically track or undo arbitrary changes to world state, resources, or external systems. Register a `rewind` callback when an effect needs to be reversed before reexecution.

Read and write guards also enforce runtime borrowing rules. Release a guard before attempting conflicting access, including access performed by rewind callbacks during synchronization.

## Project status

The examples demonstrate the current API, and the tests document specific behaviors more precisely than this overview. Neither should be taken as evidence that every combination of actions, mutations, and rewinds has been covered.

Bug reports and small reproductions are welcome. If something behaves unexpectedly, [open an issue](https://github.com/RobertasJ/caaqi/issues) with the example, its output, and the behavior you expected.