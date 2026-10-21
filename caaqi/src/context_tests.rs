use std::panic::{self, AssertUnwindSafe};

use super::*;

#[derive(Debug, Default, PartialEq)]
struct Count(u32);

#[derive(Debug, Default, PartialEq)]
struct Other(u32);

#[test]
fn resource_scope_lends_the_value_and_the_context() {
    let mut ctx = Context::new();
    ctx.insert(Count(1));
    ctx.insert(Other(10));

    let returned = ctx.resource_scope(|ctx, count: &mut Count| {
        assert_eq!(ctx.get::<Count>(), None, "the value is out while lent");
        count.0 += ctx.get::<Other>().unwrap().0;
        ctx.get_mut::<Other>().unwrap().0 = 0;
        "done"
    });

    assert_eq!(returned, "done");
    assert_eq!(ctx.get::<Count>(), Some(&Count(11)));
    assert_eq!(ctx.get::<Other>(), Some(&Other(0)));
}

#[test]
fn try_resource_scope_without_a_value_is_none() {
    let mut ctx = Context::new();
    let mut called = false;
    let result = ctx.try_resource_scope(|_, _: &mut Count| called = true);
    assert_eq!(result, None);
    assert!(!called);
}

#[test]
#[should_panic(expected = "no value of type")]
fn resource_scope_without_a_value_panics() {
    let mut ctx = Context::new();
    ctx.resource_scope(|_, _: &mut Count| {});
}

#[test]
fn a_panicking_scope_puts_the_value_back() {
    let mut ctx = Context::new();
    ctx.insert(Count(1));

    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        ctx.resource_scope(|_, count: &mut Count| {
            count.0 = 2;
            panic!("the scope panics");
        })
    }));

    assert!(result.is_err());
    assert_eq!(ctx.get::<Count>(), Some(&Count(2)));
}

#[test]
#[should_panic(expected = "was inserted during its resource_scope")]
fn inserting_the_value_inside_its_scope_panics() {
    let mut ctx = Context::new();
    ctx.insert(Count(1));
    ctx.resource_scope(|ctx, _: &mut Count| {
        ctx.insert(Count(2));
    });
}
