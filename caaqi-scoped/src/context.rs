//! The scope itself, and the context's own operations.
//!
//! [`scope`] puts a [`Context`] in a thread-local for the length of a
//! closure. Inside, this crate's functions reach it.
//!
//! Code built on this crate, like caaqi's own extension traits, can reach it
//! too with [`with_ctx`]. Wrap what you build in functions like this
//! crate's, so the code that uses it never sees the context.
//!
//! The getters take a closure instead of returning a reference, since a
//! reference into the context can't outlive the call. While that closure
//! runs, the context is out of scope, so it can't call this crate's
//! functions: they panic. To use the rest of the context alongside a value,
//! use [`resource_scope`], which keeps the context in scope.

pub use caaqi::context::Context;

scoped_tls_hkt::scoped_thread_local!(static mut CTX: Context);

/// Makes `ctx` reachable to this crate's functions while `f` runs, and
/// returns what `f` returns.
///
/// Scopes nest: inside, the innermost context is the one reachable, and the
/// outer one is reachable again once `f` returns, even if it panics.
pub fn scope<R>(ctx: &mut Context, f: impl FnOnce() -> R) -> R {
    CTX.set(ctx, f)
}

/// Whether this crate's functions would find a context: inside a [`scope`],
/// and not inside the closure of a getter such as [`get`].
pub fn in_scope() -> bool {
    CTX.is_set()
}

/// Calls `f` with the context in scope, and returns what `f` returns. Every
/// function in this crate is built on it; use it to build more, such as
/// scoped versions of other extension traits.
///
/// The context is out of scope while `f` runs, so `f` mustn't call this
/// crate's functions, or `with_ctx` itself. To run code that uses them
/// from inside `f`, put the context back in scope with
/// [`scope`]`(ctx, || ...)`.
///
/// # Panics
///
/// If no context is in scope: outside a [`scope`], or inside another
/// `with_ctx` closure.
#[track_caller]
pub fn with_ctx<R>(f: impl FnOnce(&mut Context) -> R) -> R {
    assert!(
        in_scope(),
        "no context in scope: caaqi_scoped's functions work only inside `scope`, \
         and not inside a `with_ctx` closure or a getter's, such as `get` or `get_stored`"
    );
    CTX.with(f)
}

/// [`Context::insert`]: stores `value`, replacing any value of the same
/// type, and returns the value it replaced.
pub fn insert<T: 'static>(value: T) -> Option<T> {
    with_ctx(|ctx| ctx.insert(value))
}

/// [`Context::get`]: calls `f` with the value of type `T`, or returns `None`
/// if there isn't one. `f` mustn't call this crate's functions.
pub fn get<T: 'static, R>(f: impl FnOnce(&T) -> R) -> Option<R> {
    with_ctx(|ctx| ctx.get().map(f))
}

/// [`Context::get_mut`]: calls `f` with the value of type `T` mutably, or
/// returns `None` if there isn't one. `f` mustn't call this crate's
/// functions.
pub fn get_mut<T: 'static, R>(f: impl FnOnce(&mut T) -> R) -> Option<R> {
    with_ctx(|ctx| ctx.get_mut().map(f))
}

/// [`Context::get_or_insert_with`]: calls `f` with the value of type `T`
/// mutably, creating it with `default` first if there isn't one. Neither
/// closure may call this crate's functions.
pub fn get_or_insert_with<T: 'static, R>(
    default: impl FnOnce() -> T,
    f: impl FnOnce(&mut T) -> R,
) -> R {
    with_ctx(|ctx| f(ctx.get_or_insert_with(default)))
}

/// [`Context::get_or_default`]: calls `f` with the value of type `T`
/// mutably, creating it with `T::default()` first if there isn't one. `f`
/// mustn't call this crate's functions.
pub fn get_or_default<T: Default + 'static, R>(f: impl FnOnce(&mut T) -> R) -> R {
    with_ctx(|ctx| f(ctx.get_or_default()))
}

/// [`Context::remove`]: takes the value of type `T` out of the context.
pub fn remove<T: 'static>() -> Option<T> {
    with_ctx(|ctx| ctx.remove())
}

/// [`Context::resource_scope`]: takes the value of type `T` out, calls `f`
/// with it, and puts it back, even if `f` panics. The context stays in
/// scope while `f` runs, so `f` can call this crate's functions, but there's
/// no `T` in it meanwhile.
///
/// # Panics
///
/// If there's no value of type `T`, or if `f` inserts one.
#[track_caller]
pub fn resource_scope<T: 'static, U>(f: impl FnOnce(&mut T) -> U) -> U {
    with_ctx(|ctx| ctx.resource_scope(|ctx, value| scope(ctx, || f(value))))
}

/// [`Context::try_resource_scope`]: like [`resource_scope`], but returns
/// `None`, without calling `f`, if there's no value of type `T`.
pub fn try_resource_scope<T: 'static, U>(f: impl FnOnce(&mut T) -> U) -> Option<U> {
    with_ctx(|ctx| ctx.try_resource_scope(|ctx, value| scope(ctx, || f(value))))
}

#[cfg(test)]
#[path = "context_tests.rs"]
mod tests;
