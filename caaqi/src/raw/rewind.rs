//! Rewinds: code registered on a node to undo what its run did, built only on
//! the trace's public API.
//!
//! A rewind is an `FnOnce(&mut Context, RewindKey)` registered on a node with
//! [`register_rewind`](RewindExt::register_rewind), and run by
//! [`rewind`](RewindExt::rewind) (all of a node's rewinds) or
//! [`run_rewind`](RewindExt::run_rewind) (one, by key). Each runs at most
//! once.
//!
//! # Ordering
//!
//! - The order `rewind` runs a node's rewinds in is unspecified, and code must
//!   not depend on it: debug builds shuffle it. To run one rewind before
//!   another **in the same node**, have the other call `run_rewind` on it.
//! - To reach across nodes, call `rewind` on the other node. Calling
//!   `run_rewind` on a rewind of **another** node is strongly discouraged: it
//!   leaves that node partially rewound.
//! - Calling `rewind` on your **own** node from inside one of its rewinds
//!   only delays your work until the others have run.
//! - `rewind` works on a snapshot of the node's rewinds: rewinds registered
//!   on the node while it runs aren't run by it, and stay registered.
//!
//! # Keys
//!
//! A rewind receives its own key. The key stays valid while the rewind runs,
//! so the rewind can clean up after itself (for example, remove its key from
//! sets it was added to); afterwards it's stale.
//!
//! # Nothing is enforced
//!
//! Rewinds can be registered at any time, on any node, running or not, and
//! inside other rewinds. The trace doesn't know about them, so:
//!
//! - Don't let rewinds accumulate: rewind a node before rerunning it.
//! - Rewind a node before deleting it. A deleted node's rewinds are never
//!   run, and their storage isn't reclaimed.
//!
//! A panic in a rewind consumes only that rewind; the rest stay registered,
//! and calling `rewind` again runs them.

use slotmap::{SecondaryMap, SlotMap, new_key_type};
use smallvec::SmallVec;

use crate::{
    context::Context,
    raw::trace::{NodeKey, TraceExt, UnknownNode},
};

new_key_type! {
    /// A rewind registered with [`RewindExt::register_rewind`]. Like
    /// [`NodeKey`], only valid in the `Context` that created it.
    pub struct RewindKey;
}

type Rewind = Box<dyn FnOnce(&mut Context, RewindKey)>;

struct RewindEntry {
    node: NodeKey,
    /// `None` while the rewind runs.
    rewind: Option<Rewind>,
}

/// The rewinds resource: every registered rewind that hasn't finished
/// running, and the node it belongs to.
///
/// Everything public goes through [`RewindExt`]. See the
/// [module docs](self) for the rules.
#[derive(Default)]
pub struct Rewinds {
    entries: SlotMap<RewindKey, RewindEntry>,
    by_node: SecondaryMap<NodeKey, SmallVec<[RewindKey; 2]>>,
    /// Shuffles the order [`RewindExt::rewind`] runs rewinds in.
    #[cfg(debug_assertions)]
    shuffle: Shuffle,
}

/// A xorshift generator, seeded differently for every [`Rewinds`], so that
/// code depending on the order [`RewindExt::rewind`] runs rewinds in fails in
/// debug builds.
#[cfg(debug_assertions)]
struct Shuffle(u64);

#[cfg(debug_assertions)]
impl Default for Shuffle {
    fn default() -> Self {
        use std::{collections::hash_map::RandomState, hash::BuildHasher};
        // xorshift never leaves 0, so the seed must not be 0.
        Self(RandomState::new().hash_one(0) | 1)
    }
}

#[cfg(debug_assertions)]
impl Shuffle {
    /// A number in `0..len`. `len` must not be 0.
    fn below(&mut self, len: usize) -> usize {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        (x % len as u64) as usize
    }
}

impl Rewinds {
    /// Registers `rewind` on `node`, which must be in the trace.
    fn insert(&mut self, node: NodeKey, rewind: Rewind) -> RewindKey {
        let key = self.entries.insert(RewindEntry {
            node,
            rewind: Some(rewind),
        });
        self.by_node
            .entry(node)
            .expect("a node in the trace has the newest key for its slot")
            .or_default()
            .push(key);
        key
    }

    /// Takes the closure of `key` out to run it, leaving its entry in place.
    /// Returns `None` if it's gone or already running.
    fn take(&mut self, key: RewindKey) -> Option<Rewind> {
        self.entries.get_mut(key)?.rewind.take()
    }

    /// Removes `key`'s entry, and its key from its node's list.
    fn remove(&mut self, key: RewindKey) {
        let Some(entry) = self.entries.remove(key) else {
            return;
        };
        let Some(keys) = self.by_node.get_mut(entry.node) else {
            return;
        };
        keys.retain(|&mut other| other != key);
        if keys.is_empty() {
            self.by_node.remove(entry.node);
        }
    }

    /// The keys of `node`'s rewinds, running ones included.
    fn keys(&self, node: NodeKey) -> Vec<RewindKey> {
        self.by_node
            .get(node)
            .map_or_else(Vec::new, |keys| keys.to_vec())
    }

    /// Shuffles `keys` in debug builds, and leaves them as they are
    /// otherwise.
    fn shuffle(&mut self, keys: &mut [RewindKey]) {
        #[cfg(debug_assertions)]
        for i in (1..keys.len()).rev() {
            keys.swap(i, self.shuffle.below(i + 1));
        }
        #[cfg(not(debug_assertions))]
        let _ = keys;
    }
}

fn rewinds_mut(ctx: &mut Context) -> &mut Rewinds {
    ctx.get_or_insert_with(Rewinds::default)
}

/// Registering and running rewinds. See the [module docs](self) for the
/// rules.
pub trait RewindExt {
    /// Registers `f` as a rewind of `node`, and returns its key, which `f`
    /// receives when it runs.
    ///
    /// Allowed at any time: on running nodes, and inside other rewinds. Fails
    /// only if `node` isn't in the trace.
    fn register_rewind(
        &mut self,
        node: NodeKey,
        f: impl FnOnce(&mut Context, RewindKey) + 'static,
    ) -> Result<RewindKey, UnknownNode>;

    /// A snapshot of the keys of `node`'s rewinds, running ones included, in
    /// unspecified order.
    fn rewind_keys(&self, node: NodeKey) -> Result<Vec<RewindKey>, UnknownNode>;

    /// Runs the rewind `key` now, and returns `true`. Returns `false` if it's
    /// gone because it already ran, or if it's running.
    ///
    /// Its entry stays in place while it runs, so the rewind can use its own
    /// key, and is removed afterwards, even if it panics.
    ///
    /// Use it to order rewinds **in the same node**: a rewind that must run
    /// after another calls `run_rewind` on it. Calling it on a rewind of
    /// another node is strongly discouraged, because that node is left
    /// partially rewound; call [`rewind`](Self::rewind) on it instead.
    ///
    /// A key that never existed looks the same as one that already ran.
    /// That's fine: rewind keys only come from
    /// [`register_rewind`](Self::register_rewind), and keys from other
    /// contexts are unsupported.
    fn run_rewind(&mut self, key: RewindKey) -> bool;

    /// Runs `node`'s rewinds, each with [`run_rewind`](Self::run_rewind).
    ///
    /// It works on a snapshot taken when it starts: rewinds registered on
    /// `node` meanwhile aren't run, and stay registered. Rewinds that are
    /// gone or running by the time their turn comes are skipped.
    ///
    /// The order is unspecified, and code must not depend on it: debug
    /// builds shuffle it. Called on a rewind's own node from inside that
    /// rewind, it runs the others, so the caller's work comes after them.
    ///
    /// If a rewind panics, the panic is passed on and the rewinds that
    /// hadn't run stay registered; calling `rewind` again runs them. Fails
    /// only if `node` isn't in the trace.
    fn rewind(&mut self, node: NodeKey) -> Result<(), UnknownNode>;

    /// The node the rewind `key` belongs to, or `None` once it's gone. It's
    /// still there while the rewind runs.
    fn rewind_node_of(&self, key: RewindKey) -> Option<NodeKey>;
}

impl RewindExt for Context {
    fn register_rewind(
        &mut self,
        node: NodeKey,
        f: impl FnOnce(&mut Context, RewindKey) + 'static,
    ) -> Result<RewindKey, UnknownNode> {
        self.node(node)?;
        Ok(rewinds_mut(self).insert(node, Box::new(f)))
    }

    fn rewind_keys(&self, node: NodeKey) -> Result<Vec<RewindKey>, UnknownNode> {
        // A deleted node's rewinds are left behind, so check the node.
        self.node(node)?;
        Ok(self
            .get::<Rewinds>()
            .map_or_else(Vec::new, |rewinds| rewinds.keys(node)))
    }

    fn run_rewind(&mut self, key: RewindKey) -> bool {
        /// Removes the rewind's entry when dropped, so a panicking one is
        /// removed too.
        struct Remove<'a>(&'a mut Context, RewindKey);

        impl Drop for Remove<'_> {
            fn drop(&mut self) {
                // A rewind may have removed the resource; there's nothing
                // left to clean up then.
                if let Some(rewinds) = self.0.get_mut::<Rewinds>() {
                    rewinds.remove(self.1);
                }
            }
        }

        let Some(rewind) = self
            .get_mut::<Rewinds>()
            .and_then(|rewinds| rewinds.take(key))
        else {
            return false;
        };
        let guard = Remove(self, key);
        rewind(&mut *guard.0, key);
        true
    }

    fn rewind(&mut self, node: NodeKey) -> Result<(), UnknownNode> {
        let mut keys = self.rewind_keys(node)?;
        rewinds_mut(self).shuffle(&mut keys);
        for key in keys {
            // `false` means something else already ran it, or it's running.
            self.run_rewind(key);
        }
        Ok(())
    }

    fn rewind_node_of(&self, key: RewindKey) -> Option<NodeKey> {
        Some(self.get::<Rewinds>()?.entries.get(key)?.node)
    }
}

#[cfg(test)]
mod tests {
    use std::panic::{self, AssertUnwindSafe};

    use googletest::prelude::*;

    use super::*;

    /// A runner for nodes whose runs don't matter.
    fn noop(_: &mut Context, _: NodeKey) {}

    /// What runners and rewinds did, in the order they did it.
    #[derive(Default)]
    struct Log(Vec<String>);

    fn log(ctx: &mut Context, entry: impl ToString) {
        ctx.get_or_insert_with(Log::default)
            .0
            .push(entry.to_string());
    }

    fn logged(ctx: &Context) -> Vec<String> {
        ctx.get::<Log>().map_or_else(Vec::new, |log| log.0.clone())
    }

    fn sorted<T: Ord>(mut items: Vec<T>) -> Vec<T> {
        items.sort();
        items
    }

    #[gtest]
    fn rewind_runs_every_rewind_once() {
        let mut ctx = Context::new();
        let node = ctx.create_node(noop).id();
        let keys: Vec<_> = (0..5)
            .map(|i| {
                ctx.register_rewind(node, move |ctx, _| log(ctx, i))
                    .unwrap()
            })
            .collect();
        expect_eq!(sorted(ctx.rewind_keys(node).unwrap()), sorted(keys.clone()));

        ctx.rewind(node).unwrap();
        expect_that!(ctx.rewind_keys(node).unwrap(), is_empty());
        ctx.rewind(node).unwrap();
        expect_eq!(sorted(logged(&ctx)), ["0", "1", "2", "3", "4"]);
        for key in keys {
            expect_false!(ctx.run_rewind(key));
            expect_that!(ctx.rewind_node_of(key), none());
        }
        expect_true!(ctx.get::<Rewinds>().unwrap().entries.is_empty());
        expect_true!(ctx.get::<Rewinds>().unwrap().by_node.is_empty());
    }

    #[gtest]
    fn rewinds_registered_during_rewind_are_left_registered() {
        let mut ctx = Context::new();
        let node = ctx.create_node(noop).id();
        for i in 0..3 {
            ctx.register_rewind(node, move |ctx, _| {
                log(ctx, i);
                ctx.register_rewind(node, move |ctx, _| log(ctx, format!("late {i}")))
                    .unwrap();
            })
            .unwrap();
        }

        ctx.rewind(node).unwrap();
        expect_eq!(sorted(logged(&ctx)), ["0", "1", "2"]);
        expect_that!(ctx.rewind_keys(node).unwrap().len(), eq(3));

        ctx.rewind(node).unwrap();
        expect_eq!(
            sorted(logged(&ctx)),
            ["0", "1", "2", "late 0", "late 1", "late 2"]
        );
        expect_that!(ctx.rewind_keys(node).unwrap(), is_empty());
    }

    #[gtest]
    fn a_rewind_gets_its_own_key_which_is_stale_afterwards() {
        /// The key the rewind got, and what it saw about it.
        struct Seen {
            key: RewindKey,
            node: Option<NodeKey>,
            listed: bool,
            ran_itself: bool,
        }

        let mut ctx = Context::new();
        let node = ctx.create_node(noop).id();
        let key = ctx
            .register_rewind(node, move |ctx, key| {
                let seen = Seen {
                    key,
                    node: ctx.rewind_node_of(key),
                    listed: ctx.rewind_keys(node).unwrap().contains(&key),
                    ran_itself: ctx.run_rewind(key),
                };
                ctx.insert(seen);
            })
            .unwrap();

        ctx.rewind(node).unwrap();
        let seen = ctx.get::<Seen>().unwrap();
        expect_eq!(seen.key, key);
        expect_that!(seen.node, some(eq(node)));
        expect_true!(seen.listed);
        expect_false!(seen.ran_itself);

        expect_that!(ctx.rewind_node_of(key), none());
        expect_false!(ctx.run_rewind(key));
    }

    #[cfg(debug_assertions)]
    #[gtest]
    fn rewind_order_varies_in_debug_builds() {
        let orders: std::collections::HashSet<_> = (0..8)
            .map(|_| {
                let mut ctx = Context::new();
                let node = ctx.create_node(noop).id();
                for i in 0..8 {
                    ctx.register_rewind(node, move |ctx, _| log(ctx, i))
                        .unwrap();
                }
                ctx.rewind(node).unwrap();
                logged(&ctx)
            })
            .collect();
        expect_that!(orders.len(), gt(1));
    }

    #[gtest]
    fn run_rewind_on_a_sibling_runs_it_first_and_only_once() {
        // The order `rewind` runs them in varies, so try it a few times.
        for _ in 0..8 {
            let mut ctx = Context::new();
            let node = ctx.create_node(noop).id();
            let inner = ctx
                .register_rewind(node, |ctx, _| log(ctx, "inner"))
                .unwrap();
            ctx.register_rewind(node, move |ctx, _| {
                let inner_ran = !logged(ctx).is_empty();
                expect_eq!(ctx.run_rewind(inner), !inner_ran);
                expect_eq!(logged(ctx), ["inner"]);
                expect_false!(ctx.run_rewind(inner));
                log(ctx, "outer");
            })
            .unwrap();

            ctx.rewind(node).unwrap();
            expect_eq!(logged(&ctx), ["inner", "outer"]);
        }
    }

    #[gtest]
    fn rewinding_its_own_node_runs_the_others_first() {
        for _ in 0..8 {
            let mut ctx = Context::new();
            let node = ctx.create_node(noop).id();
            for i in 0..3 {
                ctx.register_rewind(node, move |ctx, _| log(ctx, i))
                    .unwrap();
            }
            ctx.register_rewind(node, move |ctx, _| {
                ctx.rewind(node).unwrap();
                log(ctx, "last");
            })
            .unwrap();

            ctx.rewind(node).unwrap();
            let log = logged(&ctx);
            let (last, others) = log.split_last().unwrap();
            expect_eq!(sorted(others.to_vec()), ["0", "1", "2"]);
            expect_eq!(last, "last");
        }
    }

    #[gtest]
    fn a_rewind_can_rewind_another_node() {
        let mut ctx = Context::new();
        let other = ctx.create_node(noop).id();
        for i in 0..3 {
            ctx.register_rewind(other, move |ctx, _| log(ctx, i))
                .unwrap();
        }
        let node = ctx.create_node(noop).id();
        ctx.register_rewind(node, move |ctx, _| {
            ctx.rewind(other).unwrap();
            log(ctx, "outer");
        })
        .unwrap();

        ctx.rewind(node).unwrap();
        expect_that!(ctx.rewind_keys(other).unwrap(), is_empty());
        let log = logged(&ctx);
        let (outer, inner) = log.split_last().unwrap();
        expect_eq!(sorted(inner.to_vec()), ["0", "1", "2"]);
        expect_eq!(outer, "outer");
    }

    #[gtest]
    fn a_panicking_rewind_is_removed_and_the_rest_stay_registered() {
        let mut ctx = Context::new();
        let node = ctx.create_node(noop).id();
        let panicking = ctx
            .register_rewind(node, |_, _| panic!("the rewind panics"))
            .unwrap();
        for i in 0..4 {
            ctx.register_rewind(node, move |ctx, _| log(ctx, i))
                .unwrap();
        }

        let result = panic::catch_unwind(AssertUnwindSafe(|| ctx.rewind(node)));
        expect_true!(result.is_err());
        expect_that!(ctx.rewind_node_of(panicking), none());
        // The ones that didn't run before the panic are still registered.
        expect_eq!(ctx.rewind_keys(node).unwrap().len(), 4 - logged(&ctx).len());

        ctx.rewind(node).unwrap();
        expect_that!(ctx.rewind_keys(node).unwrap(), is_empty());
        expect_eq!(sorted(logged(&ctx)), ["0", "1", "2", "3"]);
    }

    #[gtest]
    fn a_rewind_can_delete_its_own_node() {
        let mut ctx = Context::new();
        let node = ctx.create_node(noop).id();
        let first = ctx
            .register_rewind(node, |ctx, _| log(ctx, "first"))
            .unwrap();
        ctx.register_rewind(node, move |ctx, _| {
            ctx.run_rewind(first);
            ctx.node_mut(node).unwrap().delete().unwrap();
            log(ctx, "deleted");
        })
        .unwrap();

        ctx.rewind(node).unwrap();
        expect_false!(ctx.contains_node(node));
        expect_eq!(logged(&ctx), ["first", "deleted"]);
    }

    #[gtest]
    fn a_node_with_rewinds_can_run() {
        let mut ctx = Context::new();
        let node = ctx
            .create_node(|ctx: &mut Context, _: NodeKey| log(ctx, "ran"))
            .id();
        let key = ctx
            .register_rewind(node, |ctx, _| log(ctx, "rewound"))
            .unwrap();

        expect_eq!(ctx.node_mut(node).unwrap().run(), Ok(()));
        expect_eq!(logged(&ctx), ["ran"]);
        expect_eq!(ctx.rewind_keys(node).unwrap(), [key]);
    }

    #[gtest]
    fn a_node_with_rewinds_can_be_deleted() {
        let mut ctx = Context::new();
        let node = ctx.create_node(noop).id();
        let key = ctx
            .register_rewind(node, |ctx, _| log(ctx, "rewound"))
            .unwrap();

        expect_eq!(ctx.node_mut(node).unwrap().delete(), Ok(()));
        expect_false!(ctx.contains_node(node));
        expect_that!(logged(&ctx), is_empty());
        // The rewind is left behind.
        expect_that!(ctx.rewind_node_of(key), some(eq(node)));
    }

    #[gtest]
    fn unknown_nodes_are_refused() {
        let mut ctx = Context::new();
        let node = ctx.create_node(noop).id();
        ctx.node_mut(node).unwrap().delete().unwrap();

        expect_that!(
            ctx.register_rewind(node, |_, _| {}).err(),
            some(eq(UnknownNode(node)))
        );
        expect_that!(ctx.rewind_keys(node).err(), some(eq(UnknownNode(node))));
        expect_that!(ctx.rewind(node).err(), some(eq(UnknownNode(node))));
    }
}
