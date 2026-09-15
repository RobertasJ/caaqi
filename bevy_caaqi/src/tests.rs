use bevy::prelude::*;
use googletest::prelude::*;

use crate::{
    CaaqiPlugin,
    action::{
        context_builder::{action, defer_action_eval, rewind, synced_rewind},
        sync::create_sync_key,
    },
    tracked_value::create_ref,
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
