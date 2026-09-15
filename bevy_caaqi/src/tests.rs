use bevy::prelude::*;
use googletest::prelude::*;

use crate::{
    CaaqiPlugin,
    action::{
        context_builder::{action, defer_action_eval, rewind, synced_rewind},
        sync::create_sync_key,
    },
    tracked_value::{create_ref, ref_, ref_action},
    var::var,
};

#[derive(Resource, Default)]
struct TestTrace(Vec<TestEvent>);

#[derive(Debug, PartialEq, Eq)]
enum TestEvent {
    Read(i32),
    Ran(&'static str),
}

fn record(world: &mut World, event: TestEvent) {
    world.resource_mut::<TestTrace>().0.push(event);
}

#[track_caller]
fn expect_trace(world: &mut World, expected: &[TestEvent]) {
    let trace = &world.resource::<TestTrace>().0;
    expect_eq!(trace, expected);
}

fn testing_app() -> App {
    init_test_logging();
    let mut app = App::new();
    app.add_plugins(CaaqiPlugin);
    app.init_resource::<TestTrace>();
    app
}

#[track_caller]
fn test_eval(world: &mut World, action: impl FnMut(&mut World) + Send + Sync + 'static) {
    defer_action_eval(world.commands(), action);
    world.flush();
}

fn init_test_logging() {
    static INIT: std::sync::Once = std::sync::Once::new();

    INIT.call_once(|| {
        tracing_subscriber::fmt()
            .with_max_level(bevy::log::Level::DEBUG)
            .with_test_writer()
            .with_ansi(true)
            .init();
    });
}

#[gtest]
fn test_empty() {
    let mut app = testing_app();
    let world = app.world_mut();

    test_eval(world, move |world| {
        record(world, TestEvent::Ran("root"));
    });

    expect_trace(world, &[TestEvent::Ran("root")]);
}

#[gtest]
fn test_order() {
    let mut app = testing_app();
    let world = app.world_mut();

    test_eval(world, move |world| {
        record(world, TestEvent::Ran("root"));

        action(world, move |world| {
            record(world, TestEvent::Ran("action 1"));
        });

        record(world, TestEvent::Ran("between"));

        action(world, move |world| {
            record(world, TestEvent::Ran("action 2"));
        });

        record(world, TestEvent::Ran("root end"));
    });

    expect_trace(
        world,
        &[
            TestEvent::Ran("root"),
            TestEvent::Ran("action 1"),
            TestEvent::Ran("between"),
            TestEvent::Ran("action 2"),
            TestEvent::Ran("root end"),
        ],
    );
}

#[gtest]
fn test_read_write() {
    let mut app = testing_app();
    let world = app.world_mut();

    test_eval(world, move |world| {
        let mut val = create_ref(&mut *world, 0);

        action(world, move |world| {
            let read = val.read(&mut *world);
            record(world, TestEvent::Read(*read));
        });

        action(world, move |world| {
            *val.write(&mut *world) += 1;
            record(world, TestEvent::Ran("write"));
        });
    });

    expect_trace(
        world,
        &[
            TestEvent::Read(0),
            TestEvent::Ran("write"),
            TestEvent::Read(1),
        ],
    );
}

#[gtest]
fn test_write_read() {
    let mut app = testing_app();
    let world = app.world_mut();

    test_eval(world, move |world| {
        let mut val = create_ref(&mut *world, 0);

        action(world, move |world| {
            *val.write(&mut *world) += 1;
            record(world, TestEvent::Ran("write"));
        });

        action(world, move |world| {
            let read = val.read(&mut *world);
            record(world, TestEvent::Read(*read));
        });
    });

    expect_trace(world, &[TestEvent::Ran("write"), TestEvent::Read(1)]);
}

#[gtest]
fn test_single_rewind() {
    let mut app = testing_app();
    let world = app.world_mut();

    test_eval(world, move |world| {
        let mut val = create_ref(&mut *world, 0);

        action(world, move |world| {
            let read = val.read(&mut *world);
            record(world, TestEvent::Read(*read));

            rewind(world, move |world| {
                record(world, TestEvent::Ran("rewound"));
            });
        });

        action(world, move |world| {
            *val.write(&mut *world) += 1;
            record(world, TestEvent::Ran("wrote val"));
        });
    });

    expect_trace(
        world,
        &[
            TestEvent::Read(0),
            TestEvent::Ran("wrote val"),
            TestEvent::Ran("rewound"),
            TestEvent::Read(1),
        ],
    );
}

#[gtest]
fn test_synced_rewinds() {
    let mut app = testing_app();
    let world = app.world_mut();

    let sync_key = create_sync_key(world);

    test_eval(world, move |world| {
        let mut val = create_ref(&mut *world, 0);

        action(world, move |world| {
            let read = *val.read(&mut *world);
            record(world, TestEvent::Read(read));

            synced_rewind(world, [sync_key], move |world| {
                record(world, TestEvent::Ran("rewound action 1"));
            });
        });

        action(world, move |world| {
            record(world, TestEvent::Ran("ran action 2"));
            synced_rewind(world, [sync_key], move |world| {
                record(world, TestEvent::Ran("rewound action 2"));
            });
        });

        action(world, move |world| {
            *val.write(&mut *world) += 1;
            record(world, TestEvent::Ran("wrote val"));
        });
    });

    expect_trace(
        world,
        &[
            TestEvent::Read(0),
            TestEvent::Ran("ran action 2"),
            TestEvent::Ran("wrote val"),
            TestEvent::Ran("rewound action 2"),
            TestEvent::Ran("rewound action 1"),
            TestEvent::Read(1),
            TestEvent::Ran("ran action 2"),
        ],
    );
}

#[gtest]
fn test_increment_until_above_three() {
    let mut app = testing_app();
    let world = app.world_mut();

    test_eval(world, move |world| {
        let mut count = create_ref(world, 0);

        action(world, move |world| {
            let value = *count.read(&mut *world);
            record(world, TestEvent::Read(value));
        });

        action(world, move |world| {
            if *count.read(&mut *world) <= 3 {
                *count.write(&mut *world) += 1;
                record(world, TestEvent::Ran("increment"));
            } else {
                record(world, TestEvent::Ran("done"));
            }
        });
    });

    expect_trace(
        world,
        &[
            TestEvent::Read(0),
            TestEvent::Ran("increment"),
            TestEvent::Read(1),
            TestEvent::Ran("increment"),
            TestEvent::Read(2),
            TestEvent::Ran("increment"),
            TestEvent::Read(3),
            TestEvent::Ran("increment"),
            TestEvent::Read(4),
            TestEvent::Ran("done"),
        ],
    );
}

#[gtest]
fn test_nested_rewinds() {
    let mut app = testing_app();
    let world = app.world_mut();

    test_eval(world, move |world| {
        record(world, TestEvent::Ran("root"));
        let mut count = create_ref(world, 0);

        action(world, move |world| {
            let value = *count.read(&mut *world);
            record(world, TestEvent::Read(value));

            rewind(world, move |world| {
                record(world, TestEvent::Ran("rewind 1"));
            });
            rewind(world, move |world| {
                record(world, TestEvent::Ran("rewind 2"));
            });

            action(world, move |world| {
                record(world, TestEvent::Ran("nested"));

                rewind(world, move |world| {
                    record(world, TestEvent::Ran("rewind 3"));
                });
                rewind(world, move |world| {
                    record(world, TestEvent::Ran("rewind 4"));
                });

                action(world, move |world| {
                    record(world, TestEvent::Ran("deep"));

                    rewind(world, move |world| {
                        record(world, TestEvent::Ran("rewind 5"));
                    });
                    rewind(world, move |world| {
                        record(world, TestEvent::Ran("rewind 6"));
                    });
                });

                rewind(world, move |world| {
                    record(world, TestEvent::Ran("rewind 7"));
                });
            });

            rewind(world, move |world| {
                record(world, TestEvent::Ran("rewind 8"));
            });
            rewind(world, move |world| {
                record(world, TestEvent::Ran("rewind 9"));
            });
        });

        action(world, move |world| {
            *count.write(&mut *world) = 1;
            record(world, TestEvent::Ran("write"));
        });
    });

    expect_trace(
        world,
        &[
            TestEvent::Ran("root"),
            TestEvent::Read(0),
            TestEvent::Ran("nested"),
            TestEvent::Ran("deep"),
            TestEvent::Ran("write"),
            TestEvent::Ran("rewind 9"),
            TestEvent::Ran("rewind 8"),
            TestEvent::Ran("rewind 7"),
            TestEvent::Ran("rewind 6"),
            TestEvent::Ran("rewind 5"),
            TestEvent::Ran("rewind 4"),
            TestEvent::Ran("rewind 3"),
            TestEvent::Ran("rewind 2"),
            TestEvent::Ran("rewind 1"),
            TestEvent::Read(1),
            TestEvent::Ran("nested"),
            TestEvent::Ran("deep"),
        ],
    );
}

fn traced_synced_action(
    world: &mut World,
    keys: Vec<crate::action::sync::SyncKey>,
    run_label: &'static str,
    rewind_label: &'static str,
) {
    action(world, move |world| {
        record(world, TestEvent::Ran(run_label));

        synced_rewind(world, keys.iter().copied(), move |world| {
            record(world, TestEvent::Ran(rewind_label));
        });
    });
}

#[gtest]
fn test_synced_rewind_branches() {
    let mut app = testing_app();
    let world = app.world_mut();

    let read_a = create_sync_key(world);
    let a_b = create_sync_key(world);
    let b_left = create_sync_key(world);
    let b_right = create_sync_key(world);
    let left_tail = create_sync_key(world);
    let right_tail = create_sync_key(world);
    let unrelated = create_sync_key(world);

    test_eval(world, move |world| {
        record(world, TestEvent::Ran("root"));
        let mut count = create_ref(world, 0);

        action(world, move |world| {
            let value = *count.read(&mut *world);
            record(world, TestEvent::Read(value));

            synced_rewind(world, [read_a], move |world| {
                record(world, TestEvent::Ran("undo read"));
            });
        });

        traced_synced_action(world, vec![unrelated], "unrelated 1", "undo unrelated 1");

        traced_synced_action(world, vec![read_a, a_b], "a", "undo a");
        traced_synced_action(world, vec![a_b, b_left, b_right], "b", "undo b");
        traced_synced_action(world, vec![b_left, left_tail], "left", "undo left");

        action(world, move |world| {
            record(world, TestEvent::Ran("independent"));

            rewind(world, move |world| {
                record(world, TestEvent::Ran("undo independent"));
            });
        });

        traced_synced_action(world, vec![left_tail], "left tail", "undo left tail");
        traced_synced_action(world, vec![b_right, right_tail], "right", "undo right");

        traced_synced_action(world, vec![unrelated], "unrelated 2", "undo unrelated 2");

        traced_synced_action(world, vec![right_tail], "right tail", "undo right tail");

        action(world, move |world| {
            *count.write(&mut *world) = 1;
            record(world, TestEvent::Ran("write"));
        });
    });

    expect_trace(
        world,
        &[
            TestEvent::Ran("root"),
            TestEvent::Read(0),
            TestEvent::Ran("unrelated 1"),
            TestEvent::Ran("a"),
            TestEvent::Ran("b"),
            TestEvent::Ran("left"),
            TestEvent::Ran("independent"),
            TestEvent::Ran("left tail"),
            TestEvent::Ran("right"),
            TestEvent::Ran("unrelated 2"),
            TestEvent::Ran("right tail"),
            TestEvent::Ran("write"),
            TestEvent::Ran("undo right tail"),
            TestEvent::Ran("undo right"),
            TestEvent::Ran("undo left tail"),
            TestEvent::Ran("undo left"),
            TestEvent::Ran("undo b"),
            TestEvent::Ran("undo a"),
            TestEvent::Ran("undo read"),
            TestEvent::Read(1),
            TestEvent::Ran("a"),
            TestEvent::Ran("b"),
            TestEvent::Ran("left"),
            TestEvent::Ran("left tail"),
            TestEvent::Ran("right"),
            TestEvent::Ran("right tail"),
        ],
    );
}

#[gtest]
fn test_synced_join_then_split() {
    let mut app = testing_app();
    let world = app.world_mut();

    let start = create_sync_key(world);
    let left_join = create_sync_key(world);
    let right_join = create_sync_key(world);
    let finish = create_sync_key(world);

    test_eval(world, move |world| {
        record(world, TestEvent::Ran("root"));
        let mut count = create_ref(world, 0);

        action(world, move |world| {
            let value = *count.read(&mut *world);
            record(world, TestEvent::Read(value));

            synced_rewind(world, [start], move |world| {
                record(world, TestEvent::Ran("undo read"));
            });
        });

        traced_synced_action(world, vec![start, left_join], "left", "undo left");
        traced_synced_action(world, vec![start, right_join], "right", "undo right");

        traced_synced_action(world, vec![], "unrelated", "undo unrelated");

        traced_synced_action(
            world,
            vec![left_join, right_join, finish],
            "join",
            "undo join",
        );
        traced_synced_action(world, vec![finish], "end 1", "undo end 1");
        traced_synced_action(world, vec![finish], "end 2", "undo end 2");

        action(world, move |world| {
            *count.write(&mut *world) = 1;
            record(world, TestEvent::Ran("write"));
        });
    });

    expect_trace(
        world,
        &[
            TestEvent::Ran("root"),
            TestEvent::Read(0),
            TestEvent::Ran("left"),
            TestEvent::Ran("right"),
            TestEvent::Ran("unrelated"),
            TestEvent::Ran("join"),
            TestEvent::Ran("end 1"),
            TestEvent::Ran("end 2"),
            TestEvent::Ran("write"),
            TestEvent::Ran("undo end 2"),
            TestEvent::Ran("undo end 1"),
            TestEvent::Ran("undo join"),
            TestEvent::Ran("undo right"),
            TestEvent::Ran("undo left"),
            TestEvent::Ran("undo read"),
            TestEvent::Read(1),
            TestEvent::Ran("left"),
            TestEvent::Ran("right"),
            TestEvent::Ran("join"),
            TestEvent::Ran("end 1"),
            TestEvent::Ran("end 2"),
        ],
    );
}

#[gtest]
fn test_shared_sync_key_block() {
    let mut app = testing_app();
    let world = app.world_mut();

    let key = create_sync_key(world);

    test_eval(world, move |world| {
        record(world, TestEvent::Ran("root"));
        let mut count = create_ref(world, 0);

        action(world, move |world| {
            let value = *count.read(&mut *world);
            record(world, TestEvent::Read(value));

            synced_rewind(world, [key], move |world| {
                record(world, TestEvent::Ran("undo read"));
            });
        });

        traced_synced_action(world, vec![key], "a", "undo a");
        traced_synced_action(world, vec![key], "b", "undo b");
        traced_synced_action(world, vec![key], "c", "undo c");

        action(world, move |world| {
            *count.write(&mut *world) = 1;
            record(world, TestEvent::Ran("write"));
        });
    });

    expect_trace(
        world,
        &[
            TestEvent::Ran("root"),
            TestEvent::Read(0),
            TestEvent::Ran("a"),
            TestEvent::Ran("b"),
            TestEvent::Ran("c"),
            TestEvent::Ran("write"),
            TestEvent::Ran("undo c"),
            TestEvent::Ran("undo b"),
            TestEvent::Ran("undo a"),
            TestEvent::Ran("undo read"),
            TestEvent::Read(1),
            TestEvent::Ran("a"),
            TestEvent::Ran("b"),
            TestEvent::Ran("c"),
        ],
    );
}

#[gtest]
fn test_repeated_synced_rewinds() {
    let mut app = testing_app();
    let world = app.world_mut();

    let key = create_sync_key(world);

    test_eval(world, move |world| {
        record(world, TestEvent::Ran("root"));
        let mut count = create_ref(world, 0);

        action(world, move |world| {
            let value = *count.read(&mut *world);
            record(world, TestEvent::Read(value));

            synced_rewind(world, [key], move |world| {
                record(world, TestEvent::Ran("undo read"));
            });
        });

        traced_synced_action(world, vec![key], "a", "undo a");
        traced_synced_action(world, vec![key], "b", "undo b");

        action(world, move |world| {
            let value = *count.read(&mut *world);

            if value < 4 {
                *count.write(&mut *world) = value + 1;
                record(world, TestEvent::Ran("increment"));
            } else {
                record(world, TestEvent::Ran("done"));
            }
        });
    });

    let mut expected = vec![
        TestEvent::Ran("root"),
        TestEvent::Read(0),
        TestEvent::Ran("a"),
        TestEvent::Ran("b"),
    ];

    for value in 1..=4 {
        expected.extend([
            TestEvent::Ran("increment"),
            TestEvent::Ran("undo b"),
            TestEvent::Ran("undo a"),
            TestEvent::Ran("undo read"),
            TestEvent::Read(value),
            TestEvent::Ran("a"),
            TestEvent::Ran("b"),
        ]);
    }

    expected.push(TestEvent::Ran("done"));
    expect_trace(world, &expected);
}

#[gtest]
fn test_synced_vec_push_pop() {
    fn push(
        world: &mut World,
        key: crate::action::sync::SyncKey,
        mut values: crate::tracked_value::Ref<Vec<i32>>,
        value: i32,
        run: &'static str,
        undo: &'static str,
    ) {
        values.silent_write(&mut *world).push(value);
        record(world, TestEvent::Ran(run));

        synced_rewind(world, [key], move |world| {
            let popped = values.silent_write(&mut *world).pop();
            expect_eq!(popped, Some(value));
            record(world, TestEvent::Ran(undo));
        });
    }

    let mut app = testing_app();
    let world = app.world_mut();

    let key = create_sync_key(world);
    let values = create_ref(world, Vec::<i32>::new());

    test_eval(world, move |world| {
        record(world, TestEvent::Ran("root"));
        let mut count = create_ref(world, 0);

        action(world, move |world| {
            push(world, key, values, 100, "prefix", "undo prefix");
        });

        action(world, move |world| {
            let value = *count.read(&mut *world);
            record(world, TestEvent::Read(value));

            // Only the untouched prefix remains before rebuilding.
            expect_eq!(values.silent_read(&mut *world).as_slice(), &[100],);

            push(world, key, values, value, "middle", "undo middle");

            action(world, move |world| {
                push(world, key, values, value + 10, "child", "undo child");
            });
        });

        action(world, move |world| {
            push(world, key, values, 99, "tail", "undo tail");
        });

        action(world, move |world| {
            let value = *count.read(&mut *world);

            expect_eq!(
                values.silent_read(&mut *world).as_slice(),
                &[100, value, value + 10, 99],
            );

            if value < 2 {
                *count.write(&mut *world) = value + 1;
                record(world, TestEvent::Ran("increment"));
            } else {
                record(world, TestEvent::Ran("done"));
            }
        });
    });

    let mut expected = vec![
        TestEvent::Ran("root"),
        TestEvent::Ran("prefix"),
        TestEvent::Read(0),
        TestEvent::Ran("middle"),
        TestEvent::Ran("child"),
        TestEvent::Ran("tail"),
    ];

    for value in 1..=2 {
        expected.extend([
            TestEvent::Ran("increment"),
            TestEvent::Ran("undo tail"),
            TestEvent::Ran("undo child"),
            TestEvent::Ran("undo middle"),
            TestEvent::Read(value),
            TestEvent::Ran("middle"),
            TestEvent::Ran("child"),
            TestEvent::Ran("tail"),
        ]);
    }

    expected.push(TestEvent::Ran("done"));
    expect_trace(world, &expected);

    expect_eq!(values.silent_read(world).as_slice(), &[100, 2, 12, 99],);
}

#[gtest]
fn test_sync_point_vec_push_pop() {
    fn push(
        world: &mut World,
        key: crate::action::sync::SyncKey,
        mut values: crate::tracked_value::Ref<Vec<i32>>,
        value: i32,
        run: &'static str,
        undo: &'static str,
    ) {
        values.silent_write(&mut *world).push(value);
        record(world, TestEvent::Ran(run));

        synced_rewind(world, [key], move |world| {
            let popped = values.silent_write(&mut *world).pop();
            expect_eq!(popped, Some(value));
            record(world, TestEvent::Ran(undo));
        });
    }

    let mut app = testing_app();
    let world = app.world_mut();

    let key = create_sync_key(world);
    let values = create_ref(world, Vec::<i32>::new());

    test_eval(world, move |world| {
        record(world, TestEvent::Ran("root"));
        let mut count = create_ref(world, 0);

        action(world, move |world| {
            push(world, key, values, 100, "prefix", "undo prefix");
        });

        action(world, move |world| {
            let value = *count.read(&mut *world);
            record(world, TestEvent::Read(value));

            crate::action::sync::sync_point(world, [key]);

            expect_eq!(values.silent_read(&mut *world).as_slice(), &[100],);
            record(world, TestEvent::Ran("synced read"));
        });

        action(world, move |world| {
            let value = *count.silent_read(&mut *world);
            push(world, key, values, value, "middle", "undo middle");

            action(world, move |world| {
                push(world, key, values, value + 10, "child", "undo child");
            });
        });

        action(world, move |world| {
            push(world, key, values, 99, "tail", "undo tail");
        });

        action(world, move |world| {
            let value = *count.read(&mut *world);

            expect_eq!(
                values.silent_read(&mut *world).as_slice(),
                &[100, value, value + 10, 99],
            );

            if value < 2 {
                *count.write(&mut *world) = value + 1;
                record(world, TestEvent::Ran("increment"));
            } else {
                record(world, TestEvent::Ran("done"));
            }
        });
    });

    let mut expected = vec![
        TestEvent::Ran("root"),
        TestEvent::Ran("prefix"),
        TestEvent::Read(0),
        TestEvent::Ran("synced read"),
        TestEvent::Ran("middle"),
        TestEvent::Ran("child"),
        TestEvent::Ran("tail"),
    ];

    for value in 1..=2 {
        expected.extend([
            TestEvent::Ran("increment"),
            TestEvent::Read(value),
            TestEvent::Ran("undo tail"),
            TestEvent::Ran("undo child"),
            TestEvent::Ran("undo middle"),
            TestEvent::Ran("synced read"),
            TestEvent::Ran("middle"),
            TestEvent::Ran("child"),
            TestEvent::Ran("tail"),
        ]);
    }

    expected.push(TestEvent::Ran("done"));
    expect_trace(world, &expected);

    expect_eq!(values.silent_read(world).as_slice(), &[100, 2, 12, 99],);
}

#[gtest]
fn test_nested_sync_point_vec_push_pop() {
    fn push(
        world: &mut World,
        key: crate::action::sync::SyncKey,
        mut values: crate::tracked_value::Ref<Vec<i32>>,
        value: i32,
        run: &'static str,
        undo: &'static str,
    ) {
        values.silent_write(&mut *world).push(value);
        record(world, TestEvent::Ran(run));

        synced_rewind(world, [key], move |world| {
            let popped = values.silent_write(&mut *world).pop();
            expect_eq!(popped, Some(value));
            record(world, TestEvent::Ran(undo));
        });
    }

    let mut app = testing_app();
    let world = app.world_mut();

    let key = create_sync_key(world);
    let values = create_ref(world, Vec::<i32>::new());

    test_eval(world, move |world| {
        record(world, TestEvent::Ran("root"));
        let mut count = create_ref(world, 0);

        action(world, move |world| {
            push(world, key, values, 100, "prefix", "undo prefix");
        });

        action(world, move |world| {
            let value = *count.read(&mut *world);
            record(world, TestEvent::Read(value));

            action(world, move |world| {
                record(world, TestEvent::Ran("nested"));

                crate::action::sync::sync_point(world, [key]);

                expect_eq!(values.silent_read(&mut *world).as_slice(), &[100],);
                record(world, TestEvent::Ran("synced read"));
            });

            record(world, TestEvent::Ran("parent continued"));
        });

        action(world, move |world| {
            let value = *count.silent_read(&mut *world);
            push(world, key, values, value, "middle", "undo middle");

            action(world, move |world| {
                push(world, key, values, value + 10, "child", "undo child");
            });
        });

        action(world, move |world| {
            push(world, key, values, 99, "tail", "undo tail");
        });

        action(world, move |world| {
            let value = *count.read(&mut *world);

            expect_eq!(
                values.silent_read(&mut *world).as_slice(),
                &[100, value, value + 10, 99],
            );

            if value < 2 {
                *count.write(&mut *world) = value + 1;
                record(world, TestEvent::Ran("increment"));
            } else {
                record(world, TestEvent::Ran("done"));
            }
        });
    });

    let mut expected = vec![
        TestEvent::Ran("root"),
        TestEvent::Ran("prefix"),
        TestEvent::Read(0),
        TestEvent::Ran("nested"),
        TestEvent::Ran("synced read"),
        TestEvent::Ran("parent continued"),
        TestEvent::Ran("middle"),
        TestEvent::Ran("child"),
        TestEvent::Ran("tail"),
    ];

    for value in 1..=2 {
        expected.extend([
            TestEvent::Ran("increment"),
            TestEvent::Read(value),
            TestEvent::Ran("nested"),
            TestEvent::Ran("undo tail"),
            TestEvent::Ran("undo child"),
            TestEvent::Ran("undo middle"),
            TestEvent::Ran("synced read"),
            TestEvent::Ran("parent continued"),
            TestEvent::Ran("middle"),
            TestEvent::Ran("child"),
            TestEvent::Ran("tail"),
        ]);
    }

    expected.push(TestEvent::Ran("done"));
    expect_trace(world, &expected);

    expect_eq!(values.silent_read(world).as_slice(), &[100, 2, 12, 99],);
}

#[gtest]
fn test_nested_sync_point_with_parent_synced_write() {
    fn push(
        world: &mut World,
        key: crate::action::sync::SyncKey,
        mut values: crate::tracked_value::Ref<Vec<i32>>,
        value: i32,
        run: &'static str,
        undo: &'static str,
    ) {
        values.silent_write(&mut *world).push(value);
        record(world, TestEvent::Ran(run));

        synced_rewind(world, [key], move |world| {
            let popped = values.silent_write(&mut *world).pop();
            expect_eq!(popped, Some(value));
            record(world, TestEvent::Ran(undo));
        });
    }

    let mut app = testing_app();
    let world = app.world_mut();

    let key = create_sync_key(world);
    let values = create_ref(world, Vec::<i32>::new());

    test_eval(world, move |world| {
        record(world, TestEvent::Ran("root"));
        let mut count = create_ref(world, 0);

        action(world, move |world| {
            push(world, key, values, 100, "prefix", "undo prefix");
        });

        action(world, move |world| {
            let value = *count.read(&mut *world);
            record(world, TestEvent::Read(value));

            action(world, move |world| {
                record(world, TestEvent::Ran("nested"));

                crate::action::sync::sync_point(world, [key]);

                expect_eq!(values.silent_read(&mut *world).as_slice(), &[100],);
                record(world, TestEvent::Ran("synced read"));
            });

            push(
                world,
                key,
                values,
                value + 20,
                "parent push",
                "undo parent push",
            );
        });

        action(world, move |world| {
            let value = *count.silent_read(&mut *world);
            push(world, key, values, value, "middle", "undo middle");

            action(world, move |world| {
                push(world, key, values, value + 10, "child", "undo child");
            });
        });

        action(world, move |world| {
            push(world, key, values, 99, "tail", "undo tail");
        });

        action(world, move |world| {
            let value = *count.read(&mut *world);

            expect_eq!(
                values.silent_read(&mut *world).as_slice(),
                &[100, value + 20, value, value + 10, 99],
            );

            if value < 2 {
                *count.write(&mut *world) = value + 1;
                record(world, TestEvent::Ran("increment"));
            } else {
                record(world, TestEvent::Ran("done"));
            }
        });
    });

    let mut expected = vec![
        TestEvent::Ran("root"),
        TestEvent::Ran("prefix"),
        TestEvent::Read(0),
        TestEvent::Ran("nested"),
        TestEvent::Ran("synced read"),
        TestEvent::Ran("parent push"),
        TestEvent::Ran("middle"),
        TestEvent::Ran("child"),
        TestEvent::Ran("tail"),
    ];

    for value in 1..=2 {
        expected.extend([
            TestEvent::Ran("increment"),
            TestEvent::Ran("undo tail"),
            TestEvent::Ran("undo child"),
            TestEvent::Ran("undo middle"),
            TestEvent::Ran("undo parent push"),
            TestEvent::Read(value),
            TestEvent::Ran("nested"),
            TestEvent::Ran("synced read"),
            TestEvent::Ran("parent push"),
            TestEvent::Ran("middle"),
            TestEvent::Ran("child"),
            TestEvent::Ran("tail"),
        ]);
    }

    expected.push(TestEvent::Ran("done"));
    expect_trace(world, &expected);

    expect_eq!(values.silent_read(world).as_slice(), &[100, 22, 2, 12, 99],);
}

#[gtest]
fn test_synced_ref_writer_stops_writing() {
    let mut app = testing_app();
    let world = app.world_mut();

    test_eval(world, move |world| {
        let mut enabled = create_ref(world, true);
        let mut values = var(world, Vec::<i32>::new());

        action(world, move |world| {
            if *enabled.read(&mut *world) {
                values.write(world).push(1);
                record(world, TestEvent::Ran("push"));
            } else {
                record(world, TestEvent::Ran("skip"));
            }
        });

        action(world, move |world| {
            let len = values.read(world).len() as i32;
            record(world, TestEvent::Read(len));
        });

        action(world, move |world| {
            *enabled.write(&mut *world) = false;
            record(world, TestEvent::Ran("disable"));
        });
    });

    expect_trace(
        world,
        &[
            TestEvent::Ran("push"),
            TestEvent::Read(1),
            TestEvent::Ran("disable"),
            TestEvent::Ran("skip"),
            TestEvent::Read(0),
        ],
    );
}

#[gtest]
fn test_synced_ref_writer_toggles() {
    let mut app = testing_app();
    let world = app.world_mut();

    test_eval(world, move |world| {
        let mut phase = create_ref(world, 0);
        let mut values = var(world, vec![100]);

        action(world, move |world| {
            expect_eq!(values.read(world).as_slice(), &[100]);
            record(world, TestEvent::Ran("prefix read"));
        });

        action(world, move |world| {
            let n = *phase.read(&mut *world);

            if n % 2 == 1 {
                values.write(world).push(n);
                record(world, TestEvent::Ran("push"));
            } else {
                record(world, TestEvent::Ran("skip"));
            }
        });

        action(world, move |world| {
            values.write(world).push(99);
            record(world, TestEvent::Ran("tail"));
        });

        action(world, move |world| {
            // Silent: this reader must be scheduled by values.
            let n = *phase.silent_read(&mut *world);
            let expected = if n % 2 == 1 {
                vec![100, n, 99]
            } else {
                vec![100, 99]
            };

            let actual = values.read(world);
            expect_eq!(actual.as_slice(), expected.as_slice());
            let len = actual.len() as i32;
            drop(actual);

            record(world, TestEvent::Read(len));
        });

        action(world, move |world| {
            let n = *phase.read(&mut *world);
            if n < 3 {
                *phase.write(&mut *world) = n + 1;
            }
        });
    });

    let mut expected = vec![TestEvent::Ran("prefix read")];

    for n in 0..=3 {
        expected.extend([
            TestEvent::Ran(if n % 2 == 1 { "push" } else { "skip" }),
            TestEvent::Ran("tail"),
            TestEvent::Read(if n % 2 == 1 { 3 } else { 2 }),
        ]);
    }

    expect_trace(world, &expected);
}

#[gtest]
fn test_synced_ref_multiple_writes_with_nested_action() {
    let mut app = testing_app();
    let world = app.world_mut();

    test_eval(world, move |world| {
        let mut phase = create_ref(world, 0);
        let mut values = var(world, vec![100]);

        action(world, move |world| {
            let n = *phase.read(&mut *world);

            expect_eq!(values.read(world).as_slice(), &[100]);

            values.write(world).push(n);
            record(world, TestEvent::Ran("parent first"));

            action(world, move |world| {
                expect_eq!(values.read(world).as_slice(), &[100, n],);

                values.write(world).push(n + 10);
                record(world, TestEvent::Ran("child"));
            });

            expect_eq!(values.read(world).as_slice(), &[100, n, n + 10],);

            values.write(world).push(n + 20);
            record(world, TestEvent::Ran("parent last"));
        });

        action(world, move |world| {
            let n = *phase.silent_read(&mut *world);
            let actual = values.read(world);

            expect_eq!(actual.as_slice(), &[100, n, n + 10, n + 20],);

            let observed = actual[1];
            drop(actual);
            record(world, TestEvent::Read(observed));
        });

        action(world, move |world| {
            let n = *phase.read(&mut *world);
            if n < 2 {
                *phase.write(&mut *world) = n + 1;
            }
        });
    });

    let mut expected = vec![];

    for n in 0..=2 {
        expected.extend([
            TestEvent::Ran("parent first"),
            TestEvent::Ran("child"),
            TestEvent::Ran("parent last"),
            TestEvent::Read(n),
        ]);
    }

    expect_trace(world, &expected);
}

#[gtest]
fn test_synced_ref_independent_values() {
    let mut app = testing_app();
    let world = app.world_mut();

    test_eval(world, move |world| {
        let mut phase = create_ref(world, 0);
        let mut left = var(world, Vec::<i32>::new());
        let mut right = var(world, Vec::<i32>::new());

        action(world, move |world| {
            let n = *phase.read(&mut *world);
            left.write(world).push(n);
            record(world, TestEvent::Ran("left write"));
        });

        action(world, move |world| {
            right.write(world).push(99);
            record(world, TestEvent::Ran("right write"));
        });

        action(world, move |world| {
            expect_eq!(right.read(world).as_slice(), &[99]);
            record(world, TestEvent::Ran("right read"));
        });

        action(world, move |world| {
            let n = *phase.silent_read(&mut *world);
            let actual = left.read(world);

            expect_eq!(actual.as_slice(), &[n]);

            let observed = actual[0];
            drop(actual);
            record(world, TestEvent::Read(observed));
        });

        action(world, move |world| {
            let n = *phase.read(&mut *world);
            if n < 2 {
                *phase.write(&mut *world) = n + 1;
            }
        });
    });

    expect_trace(
        world,
        &[
            TestEvent::Ran("left write"),
            TestEvent::Ran("right write"),
            TestEvent::Ran("right read"),
            TestEvent::Read(0),
            TestEvent::Ran("left write"),
            TestEvent::Read(1),
            TestEvent::Ran("left write"),
            TestEvent::Read(2),
        ],
    );
}

#[gtest]
fn test_synced_ref_nested_writer_disappears_and_returns() {
    #[derive(Resource, Default)]
    struct Snapshots {
        prefix: Vec<Vec<i32>>,
        end: Vec<Vec<i32>>,
    }

    let mut app = testing_app();
    app.init_resource::<Snapshots>();
    let world = app.world_mut();

    test_eval(world, move |world| {
        let mut count = ref_(world, 0);
        let mut numbers = var(world, vec![]);

        action(world, move |world| {
            if *count.read(&mut *world) != 4 {
                action(world, move |world| {
                    numbers.write(world).push(1);
                });
            }
        });

        action(world, move |world| {
            let snapshot = numbers.read(world).to_vec();
            world.resource_mut::<Snapshots>().prefix.push(snapshot);
            record(world, TestEvent::Ran("prefix"));
        });

        action(world, move |world| {
            let read = count.read(&mut *world);
            numbers.write(world).push(*read);
        });

        action(world, move |world| {
            numbers.write(world).push(69);

            let snapshot = numbers.read(world).to_vec();
            world.resource_mut::<Snapshots>().end.push(snapshot);
            record(world, TestEvent::Ran("end"));
        });

        action(world, move |world| {
            if *count.read(&mut *world) < 5 {
                *count.write(world) += 1;
            }
        });
    });

    let snapshots = world.resource::<Snapshots>();

    expect_eq!(
        &snapshots.prefix,
        &vec![vec![1], vec![1], vec![1], vec![1], vec![], vec![1],],
    );

    expect_eq!(
        &snapshots.end,
        &vec![
            vec![1, 0, 69],
            vec![1, 1, 69],
            vec![1, 2, 69],
            vec![1, 3, 69],
            vec![4, 69],
            vec![1, 5, 69],
        ],
    );

    let mut expected = vec![];
    for _ in 0..=5 {
        expected.extend([TestEvent::Ran("prefix"), TestEvent::Ran("end")]);
    }

    expect_trace(world, &expected);
}
