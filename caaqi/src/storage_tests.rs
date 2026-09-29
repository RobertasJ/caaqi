use googletest::prelude::*;

use super::*;

#[gtest]
fn stored_values_can_be_read_written_and_removed() {
    let mut ctx = Context::new();
    let stored = ctx.store(String::from("a"));
    expect_that!(ctx.get_stored(stored), ok(eq("a")));

    ctx.get_stored_mut(stored).unwrap().push('b');
    expect_that!(ctx.clone_stored(stored), ok(eq("ab")));

    expect_that!(ctx.remove_stored(stored), ok(eq("ab")));
    let unknown = UnknownStored(stored.key);
    expect_eq!(ctx.get_stored(stored).unwrap_err(), unknown);
    expect_eq!(ctx.get_stored_mut(stored).unwrap_err(), unknown);
    expect_eq!(ctx.clone_stored(stored).unwrap_err(), unknown);
    expect_eq!(ctx.remove_stored(stored).unwrap_err(), unknown);
}

#[gtest]
fn values_of_different_types_are_kept_apart() {
    let mut ctx = Context::new();
    let number = ctx.store(1_u32);
    let text = ctx.store("one");
    expect_that!(ctx.get_stored(number), ok(eq(&1)));
    expect_that!(ctx.get_stored(text), ok(eq(&"one")));

    ctx.remove_stored(number).unwrap();
    expect_that!(ctx.get_stored(text), ok(eq(&"one")));
}

#[gtest]
fn a_handle_is_copy_whatever_its_value() {
    #[derive(Debug)]
    struct NotClone;
    let mut ctx = Context::new();
    let stored = ctx.store(NotClone);
    let copy = stored;
    expect_eq!(copy, stored);
    expect_that!(ctx.get_stored(stored), ok(anything()));
}

#[gtest]
fn a_removed_value_frees_its_key() {
    let mut ctx = Context::new();
    let stored = ctx.store(0_u8);
    ctx.remove_stored(stored).unwrap();
    expect_that!(ctx.get::<StorageKeys>().unwrap().keys.len(), eq(0));

    // The slot is reused, but the old handle doesn't reach the new value.
    let _new = ctx.store(1_u8);
    expect_that!(ctx.get_stored(stored), err(anything()));
}
