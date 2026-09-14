use bevy::{log::LogPlugin, prelude::*};
use googletest::prelude::*;

use crate::{
    CaaqiPlugin,
    action::context_builder::{action, defer_action_eval},
    tracked_value::create_ref,
};

fn testing_app() -> App {
    init_test_logging();
    let mut app = App::new();
    app.add_plugins(CaaqiPlugin);
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
            .with_ansi(false)
            .init();
    });
}

#[gtest]
fn test_empty() {
    let mut app = testing_app();
    let world = app.world_mut();

    let mut root_run_count = create_ref(&mut *world, 0);

    test_eval(world, move |world| {
        *root_run_count.silent_write(&mut *world) += 1;
    });

    expect_eq!(*root_run_count.silent_read(&mut *world), 1);
}

#[gtest]
fn test_order() {
    let mut app = testing_app();
    let world = app.world_mut();

    let mut root_run_count = create_ref(world, 0);
    let mut first_run_count = create_ref(world, 0);
    let mut second_run_count = create_ref(world, 0);

    test_eval(world, move |world| {
        *root_run_count.silent_write(&mut *world) += 1;

        action(world, move |world| {
            expect_eq!(*root_run_count.silent_read(&mut *world), 1);
            *first_run_count.silent_write(&mut *world) += 1;
        });

        action(world, move |world| {
            expect_eq!(*first_run_count.silent_read(&mut *world), 1);
            *second_run_count.silent_write(&mut *world) += 1;
        });

        expect_eq!(*second_run_count.silent_read(&mut *world), 1);
        expect_eq!(*first_run_count.silent_read(&mut *world), 1);
    });

    expect_eq!(*root_run_count.silent_read(&mut *world), 1);
    expect_eq!(*first_run_count.silent_read(&mut *world), 1);
    expect_eq!(*second_run_count.silent_read(&mut *world), 1);
}

#[gtest]
fn test_read_write() {
    let mut app = testing_app();
    let world = app.world_mut();

    let mut root_run_count = create_ref(world, 0);
    let mut read_run_count = create_ref(world, 0);
    let mut write_run_count = create_ref(world, 0);

    test_eval(world, move |world| {
        *root_run_count.silent_write(&mut *world) += 1;

        let mut val = create_ref(&mut *world, 0);

        action(world, move |world| {
            println!("{}", *val.read(&mut *world));
            *read_run_count.silent_write(&mut *world) += 1;

            match *read_run_count.silent_read(&mut *world) {
                1 => expect_eq!(*val.silent_read(&mut *world), 0),
                2 => expect_eq!(*val.silent_read(&mut *world), 1),
                _ => {}
            }
        });

        action(world, move |world| {
            *val.write(&mut *world) += 1;
            println!("wrote val");
            *write_run_count.silent_write(&mut *world) += 1;
        });
    });

    expect_eq!(*root_run_count.silent_read(&mut *world), 1);
    expect_eq!(*read_run_count.silent_read(&mut *world), 2);
    expect_eq!(*write_run_count.silent_read(&mut *world), 1);
}

#[gtest]
fn test_write_read() {
    let mut app = testing_app();
    let world = app.world_mut();

    let mut root_run_count = create_ref(world, 0);
    let mut read_run_count = create_ref(world, 0);
    let mut write_run_count = create_ref(world, 0);

    test_eval(world, move |world| {
        *root_run_count.silent_write(&mut *world) += 1;

        let mut val = create_ref(&mut *world, 0);

        action(world, move |world| {
            *val.write(&mut *world) += 1;
            println!("wrote val");
            *write_run_count.silent_write(&mut *world) += 1;
        });

        action(world, move |world| {
            println!("{}", *val.read(&mut *world));
            *read_run_count.silent_write(&mut *world) += 1;

            if *read_run_count.silent_read(&mut *world) == 1 {
                expect_eq!(*val.silent_read(&mut *world), 1);
            }
        });
    });

    expect_eq!(*root_run_count.silent_read(&mut *world), 1);
    expect_eq!(*read_run_count.silent_read(&mut *world), 1);
    expect_eq!(*write_run_count.silent_read(&mut *world), 1);
}
