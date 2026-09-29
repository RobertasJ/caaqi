//! Rewinds: code that undoes what a node's run did.
//!
//! When a node's run changes some state, it also registers a rewind on the
//! node with [`register_rewind`](RewindExt::register_rewind): code that puts
//! the state back. Before running the node again, call
//! [`rewind`](RewindExt::rewind) on it, which runs all its rewinds. Each
//! rewind runs at most once.
//!
//! ```
//! use caaqi::prelude::*;
//!
//! let mut ctx = Context::new();
//! ctx.insert(Vec::<&str>::new());
//! let node = ctx
//!     .create_node(|ctx: &mut Context, node| {
//!         ctx.get_mut::<Vec<&str>>().unwrap().push("hello");
//!         ctx.register_rewind(node, |ctx: &mut Context, _| {
//!             ctx.get_mut::<Vec<&str>>().unwrap().pop();
//!         })
//!         .unwrap();
//!     })
//!     .id();
//!
//! ctx.node_mut(node)?.run()?;
//! ctx.rewind(node)?;
//! ctx.node_mut(node)?.run()?;
//! assert_eq!(ctx.get::<Vec<&str>>().unwrap(), &["hello"]);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! # Order
//!
//! - The order in which [`rewind`](RewindExt::rewind) runs a node's rewinds
//!   is unspecified, and your code must not depend on it. Debug builds
//!   shuffle it on purpose, to catch code that does.
//! - To run one rewind before another **on the same node**, have the later
//!   one call [`run_rewind`](RewindExt::run_rewind) on the earlier one
//!   first.
//! - To rewind **another** node, call `rewind` on that node. Avoid calling
//!   `run_rewind` on another node's rewinds: it leaves that node only
//!   partly rewound.
//! - Calling `rewind` on a rewind's own node, from inside that rewind, runs
//!   the node's other rewinds first. Use it to do your work after theirs.
//!
//! # Keys
//!
//! Each rewind is given its own [`RewindKey`] when it runs. The key stays
//! valid while the rewind runs, so the rewind can use it to clean up after
//! itself, for example by removing the key from places it was stored. Once
//! the rewind has finished, the key no longer refers to anything.
//!
//! # Things to keep in mind
//!
//! Rewinds can be registered at any time, on any node, including while the
//! node is running and from inside other rewinds. Nothing enforces how
//! they're used, so:
//!
//! - Rewind a node before running it again. Otherwise its old rewinds pile
//!   up alongside the new ones.
//! - Rewind a node before deleting it. A deleted node's rewinds never run.
//! - `rewind` only runs the rewinds that were registered when it started.
//!   Rewinds registered on the node while it's rewinding stay registered
//!   for the next `rewind`.
//! - If a rewind panics, only that rewind is used up. The node's other
//!   rewinds stay registered, and calling `rewind` again runs them.

use slotmap::{SecondaryMap, SlotMap, new_key_type};
use smallvec::SmallVec;

use crate::{
    context::Context,
    trace::{NodeKey, TraceExt, UnknownNode},
};

new_key_type! {
    /// Identifies a rewind. Get one from [`RewindExt::register_rewind`].
    pub struct RewindKey;
}

/// Code that undoes what a node's run did. Register it on a node with
/// [`RewindExt::register_rewind`]; it runs once, and is given its own key.
///
/// Any `FnOnce(&mut Context, RewindKey)` closure is a rewind. Annotate the
/// closure's parameter types (`|ctx: &mut Context, key| ...`), because Rust
/// can't infer them here.
///
/// If your code doesn't need to be given its key, use a
/// [`current::Rewind`](crate::current::Rewind) instead.
pub trait RewindWithKey: 'static {
    fn rewind(self: Box<Self>, ctx: &mut Context, key: RewindKey);
}

impl<F: FnOnce(&mut Context, RewindKey) + 'static> RewindWithKey for F {
    fn rewind(self: Box<Self>, ctx: &mut Context, key: RewindKey) {
        (*self)(ctx, key)
    }
}

struct RewindEntry {
    node: NodeKey,
    /// `None` while the rewind runs.
    rewind: Option<Box<dyn RewindWithKey>>,
}

/// The rewinds resource: every rewind that hasn't finished running, and the
/// node it's registered on. Everything public goes through [`RewindExt`].
#[derive(Default)]
struct Rewinds {
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
    fn insert(&mut self, node: NodeKey, rewind: Box<dyn RewindWithKey>) -> RewindKey {
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
    fn take(&mut self, key: RewindKey) -> Option<Box<dyn RewindWithKey>> {
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
    fn keys(&self, node: NodeKey) -> impl Iterator<Item = RewindKey> + '_ {
        self.by_node.get(node).into_iter().flatten().copied()
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

/// Registering and running rewinds. See the [module docs](self) for an
/// example and for the rules.
pub trait RewindExt {
    /// Registers `f` as a rewind of `node`, and returns its key. `f` is given
    /// the same key when it runs.
    ///
    /// You can register rewinds at any time, including while `node` is
    /// running and from inside other rewinds. Fails only if `node` isn't in
    /// the trace.
    fn register_rewind(
        &mut self,
        node: NodeKey,
        f: impl RewindWithKey,
    ) -> Result<RewindKey, UnknownNode>;

    /// The keys of `node`'s rewinds that haven't finished running, in no
    /// particular order. The iterator borrows the context, so collect it
    /// first if you need the keys while changing the context.
    ///
    /// Fails if `node` isn't in the trace.
    fn rewind_keys(
        &self,
        node: NodeKey,
    ) -> Result<impl Iterator<Item = RewindKey> + '_, UnknownNode>;

    /// Runs one rewind now and returns `true`. Returns `false`, and does
    /// nothing, if the rewind has already run or is running right now.
    ///
    /// Use it to order rewinds **on the same node**: a rewind that must run
    /// after another calls `run_rewind` on that one first. Avoid using it on
    /// another node's rewinds, which leaves that node only partly rewound;
    /// call [`rewind`](Self::rewind) on that node instead.
    ///
    /// The rewind is used up even if it panics.
    fn run_rewind(&mut self, key: RewindKey) -> bool;

    /// Runs all of `node`'s rewinds, undoing what its runs did. Call it
    /// before running the node again, and before deleting it.
    ///
    /// Only the rewinds registered when it starts are run. Rewinds registered
    /// on `node` in the meantime stay registered. Rewinds that have already
    /// run, or are running, by the time their turn comes are skipped.
    ///
    /// The order is unspecified, and your code must not depend on it: debug
    /// builds shuffle it. See the [module docs](self#order) for how to order
    /// rewinds.
    ///
    /// If a rewind panics, the panic is passed on, and the rewinds that
    /// hadn't run yet stay registered; calling `rewind` again runs them.
    /// Fails only if `node` isn't in the trace.
    fn rewind(&mut self, node: NodeKey) -> Result<(), UnknownNode>;

    /// The node the rewind was registered on, or `None` once the rewind has
    /// finished running. While the rewind runs, this still returns its
    /// node.
    fn rewind_node_of(&self, key: RewindKey) -> Option<NodeKey>;
}

impl RewindExt for Context {
    fn register_rewind(
        &mut self,
        node: NodeKey,
        f: impl RewindWithKey,
    ) -> Result<RewindKey, UnknownNode> {
        self.node(node)?;
        Ok(rewinds_mut(self).insert(node, Box::new(f)))
    }

    fn rewind_keys(
        &self,
        node: NodeKey,
    ) -> Result<impl Iterator<Item = RewindKey> + '_, UnknownNode> {
        // A deleted node's rewinds are left behind, so check the node.
        self.node(node)?;
        Ok(self
            .get::<Rewinds>()
            .into_iter()
            .flat_map(move |rewinds| rewinds.keys(node)))
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
        rewind.rewind(&mut *guard.0, key);
        true
    }

    fn rewind(&mut self, node: NodeKey) -> Result<(), UnknownNode> {
        let mut keys: Vec<_> = self.rewind_keys(node)?.collect();
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
#[path = "rewind_tests.rs"]
mod tests;
