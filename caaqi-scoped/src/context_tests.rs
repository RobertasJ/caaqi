use std::panic::{self, AssertUnwindSafe};

use super::*;

#[derive(Debug, Default, PartialEq)]
struct Count(u32);

#[test]
fn functions_reach_the_scoped_context() {
    let mut ctx = Context::new();
    scope(&mut ctx, || {
        assert_eq!(insert(Count(1)), None);
        get_mut(|count: &mut Count| count.0 += 1);
        assert_eq!(get(|count: &Count| count.0), Some(2));
        get_or_default(|count: &mut Count| count.0 += 1);
    });
    assert_eq!(ctx.get::<Count>(), Some(&Count(3)));
}

#[test]
fn in_scope_only_inside_scope() {
    let mut ctx = Context::new();
    assert!(!in_scope());
    scope(&mut ctx, || {
        assert!(in_scope());
        get_or_default(|_: &mut Count| assert!(!in_scope()));
        assert!(in_scope());
    });
    assert!(!in_scope());
}

#[test]
#[should_panic(expected = "no context in scope")]
fn functions_outside_a_scope_panic() {
    insert(Count(1));
}

#[test]
#[should_panic(expected = "no context in scope")]
fn functions_inside_a_getter_panic() {
    let mut ctx = Context::new();
    scope(&mut ctx, || get_or_default(|_: &mut Count| insert(1_u8)));
}

#[test]
fn resource_scope_keeps_the_context_in_scope() {
    let mut ctx = Context::new();
    scope(&mut ctx, || {
        insert(Count(1));
        resource_scope(|count: &mut Count| {
            assert_eq!(get(|_: &Count| ()), None, "the value is out while lent");
            insert(10_u32);
            count.0 += remove::<u32>().unwrap();
        });
        assert_eq!(remove::<Count>(), Some(Count(11)));
        assert_eq!(try_resource_scope(|_: &mut Count| ()), None);
    });
}

#[test]
fn a_panicking_scope_is_left() {
    let mut ctx = Context::new();
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        scope(&mut ctx, || panic!("the scope panics"));
    }));
    assert!(result.is_err());
    assert!(!in_scope());
}

#[test]
fn with_ctx_lends_the_context_and_can_scope_it_again() {
    let mut ctx = Context::new();
    scope(&mut ctx, || {
        with_ctx(|ctx| {
            assert!(!in_scope());
            ctx.insert(Count(1));
            scope(ctx, || get_mut(|count: &mut Count| count.0 += 1));
        });
        assert_eq!(get(|count: &Count| count.0), Some(2));
    });
}

#[test]
#[should_panic(expected = "no context in scope")]
fn nested_with_ctx_panics() {
    let mut ctx = Context::new();
    scope(&mut ctx, || with_ctx(|_| with_ctx(|_| {})));
}
