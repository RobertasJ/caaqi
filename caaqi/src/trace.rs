use std::panic::{self, AssertUnwindSafe};

use slotmap::{SlotMap, new_key_type};
use smallvec::SmallVec;

use crate::context::Context;

new_key_type! {
    /// Only valid in the `Context` that created it. Using a key with another
    /// context is unsupported and may refer to an unrelated node.
    pub struct NodeKey;

    /// A rewind registered with [`NodeMut::register_rewind`]. Like
    /// [`NodeKey`], only valid in the `Context` that created it.
    pub struct RewindKey;
}

/// The code that runs a node, given to [`TraceExt::create_node`] and called
/// by [`NodeMut::run`] with the node's key. Closures taking
/// `(&mut Context, NodeKey)` are runners.
pub trait Runner: 'static {
    fn run(&mut self, ctx: &mut Context, node: NodeKey);
}

impl<F: FnMut(&mut Context, NodeKey) + 'static> Runner for F {
    fn run(&mut self, ctx: &mut Context, node: NodeKey) {
        self(ctx, node)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("node {0:?} isn't in the trace")]
pub struct UnknownNode(pub NodeKey);

/// The parent passed to [`NodeMut::set_parent`] isn't in the trace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("parent {:?} isn't in the trace", .0.0)]
pub struct UnknownParent(#[source] pub UnknownNode);

/// The child passed to [`NodeMut::add_child`] isn't in the trace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("child {:?} isn't in the trace", .0.0)]
pub struct UnknownChild(#[source] pub UnknownNode);

/// The child already has a parent. Reparenting is explicit: call
/// [`detach`](NodeMut::detach) first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error(
    "node {child:?} already has parent {current_parent:?}; \
     call `detach` before attaching it to {requested_parent:?}"
)]
pub struct AlreadyParented {
    pub child: NodeKey,
    pub current_parent: NodeKey,
    pub requested_parent: NodeKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("node {0:?} can't be its own parent")]
pub struct SelfParent(pub NodeKey);

/// The requested parent is a descendant of the child, `depth` levels below
/// it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error(
    "parenting {child:?} under {parent:?} would create a cycle; \
     {parent:?} is {depth} level(s) below {child:?}"
)]
pub struct WouldCycle {
    pub child: NodeKey,
    pub parent: NodeKey,
    pub depth: usize,
}

/// [`NodeMut::delete`] only deletes leaves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error(
    "node {key:?} has {children} child(ren); \
     delete or detach them first"
)]
pub struct HasChildren {
    pub key: NodeKey,
    pub children: usize,
}

/// [`NodeMut::delete`] refuses nodes whose rewinds haven't run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("node {0:?} has rewinds registered; call `rewind` before deleting it")]
pub struct HasRewinds(pub NodeKey);

/// [`NodeMut::run`] refuses nodes whose rewinds haven't run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("node {0:?} has rewinds registered; call `rewind` before running it")]
pub struct NotRewound(pub NodeKey);

/// The node's runner is taken out because the node is running.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("node {0:?} is running")]
pub struct RunnerInUse(pub NodeKey);

/// [`NodeMut::register_rewind`] is refused while any rewind is running.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("can't register a rewind while a rewind is running")]
pub struct InsideRewind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SetParentError {
    #[error(transparent)]
    UnknownParent(#[from] UnknownParent),
    #[error(transparent)]
    AlreadyParented(#[from] AlreadyParented),
    #[error(transparent)]
    SelfParent(#[from] SelfParent),
    #[error(transparent)]
    WouldCycle(#[from] WouldCycle),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AddChildError {
    #[error(transparent)]
    UnknownChild(#[from] UnknownChild),
    #[error(transparent)]
    AlreadyParented(#[from] AlreadyParented),
    #[error(transparent)]
    SelfParent(#[from] SelfParent),
    #[error(transparent)]
    WouldCycle(#[from] WouldCycle),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RunError {
    #[error(transparent)]
    RunnerInUse(#[from] RunnerInUse),
    #[error(transparent)]
    NotRewound(#[from] NotRewound),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DeleteError {
    #[error(transparent)]
    RunnerInUse(#[from] RunnerInUse),
    #[error(transparent)]
    HasRewinds(#[from] HasRewinds),
    #[error(transparent)]
    HasChildren(#[from] HasChildren),
}

/// The trace resource. It holds the shape of the trace and each node's
/// runner and rewinds; other per-node data lives in other resources keyed by
/// [`NodeKey`].
///
/// Everything public goes through [`TraceExt`] and the node handles it
/// returns, [`NodeRef`] and [`NodeMut`].
///
/// Besides parent and children links, every node is linked to its `prev` and
/// `next` node in trace order: each node before its descendants, siblings
/// first to last. So a node's `prev` is its previous sibling's last
/// descendant (or that sibling itself, when it's a leaf), or its parent when
/// it's a first child. Each root's tree has its own order: a root has no
/// `prev`, the last node of its tree has no `next`, and the orders of two
/// roots never link.
///
/// # Running and rewinding
///
/// Every node has a [`Runner`], the code that runs it. Anything a run needs
/// undone is registered as a rewind on a node with
/// [`register_rewind`](NodeMut::register_rewind).
/// [`rewind`](NodeMut::rewind) runs a node's rewinds, one at a time, in
/// unspecified order, until it has none left. A rewind may run other rewinds
/// by key with [`run_rewind`](TraceExt::run_rewind), or rewind other nodes:
/// that's how nesting and ordering are expressed.
///
/// - Code must not depend on the order `rewind` picks rewinds in. Debug
///   builds pick at random.
/// - [`run`](NodeMut::run) refuses a node that still has rewinds.
/// - A running node can't be deleted, and neither can one with rewinds.
/// - No rewind can be registered while a rewind is running.
#[derive(Default)]
pub struct Trace {
    nodes: SlotMap<NodeKey, Node>,
    rewinds: SlotMap<RewindKey, RewindEntry>,
    /// The number of rewinds running, nested ones included.
    rewind_depth: usize,
    /// Picks the rewind [`NodeMut::rewind`] runs next.
    #[cfg(debug_assertions)]
    shuffle: Shuffle,
}

struct Node {
    children: SmallVec<[NodeKey; 1]>,
    parent: Option<NodeKey>,
    prev: Option<NodeKey>,
    next: Option<NodeKey>,
    /// `None` only while the runner is taken out to run.
    runner: Option<Box<dyn Runner>>,
    /// The keys of the node's registered rewinds, in `Trace::rewinds`.
    rewinds: SmallVec<[RewindKey; 2]>,
}

type Rewind = Box<dyn FnOnce(&mut Context)>;

struct RewindEntry {
    node: NodeKey,
    rewind: Rewind,
}

/// A xorshift generator, seeded differently for every [`Trace`], so that code
/// depending on the order [`NodeMut::rewind`] picks rewinds in fails in
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

impl Trace {
    fn contains(&self, key: NodeKey) -> bool {
        self.nodes.contains_key(key)
    }

    /// The last node of `key`'s subtree in trace order: its last child's last
    /// child, and so on, or `key` itself when it's a leaf. `key` must be in
    /// the trace.
    fn subtree_last(&self, key: NodeKey) -> NodeKey {
        std::iter::successors(Some(key), |&key| {
            self.nodes
                .get(key)
                .expect("`key` and its descendants are in the trace")
                .children
                .last()
                .copied()
        })
        .last()
        .expect("the walk starts at `key`")
    }

    /// Links `first` and `second` to each other. A `None` side ends the order
    /// there.
    fn link(&mut self, first: Option<NodeKey>, second: Option<NodeKey>) {
        if let Some(prev) = first {
            self.nodes
                .get_mut(prev)
                .expect("linked nodes are in the trace")
                .next = second;
        }
        if let Some(next) = second {
            self.nodes
                .get_mut(next)
                .expect("linked nodes are in the trace")
                .prev = first;
        }
    }

    /// Takes the span from `span_start` to `span_end` out of its tree's order, linking
    /// the nodes on either side of it to each other. The span is left as an
    /// order of its own.
    fn cut(&mut self, span_start: NodeKey, span_end: NodeKey) {
        let start = self
            .nodes
            .get(span_start)
            .expect("the span is in the trace");
        let end = self.nodes.get(span_end).expect("the span is in the trace");
        let (prev, next) = (start.prev, end.next);
        self.link(prev, next);
        self.link(None, Some(span_start));
        self.link(Some(span_end), None);
    }

    /// Puts the span from `span_start` to `span_end`, which must be an order of its
    /// own, right after `put_after`.
    fn splice(&mut self, put_after: NodeKey, span_start: NodeKey, span_end: NodeKey) {
        let next = self
            .nodes
            .get(put_after)
            .expect("`put_after` is in the trace")
            .next;
        self.link(Some(put_after), Some(span_start));
        self.link(Some(span_end), next);
    }

    /// Adds an unparented node without children, alone in its own order.
    fn insert_root(&mut self, runner: Box<dyn Runner>) -> NodeKey {
        self.nodes.insert(Node {
            children: SmallVec::new(),
            parent: None,
            prev: None,
            next: None,
            runner: Some(runner),
            rewinds: SmallVec::new(),
        })
    }

    /// Takes `key`'s runner out to run it. `key` must be in the trace.
    fn take_runner(&mut self, key: NodeKey) -> Result<Box<dyn Runner>, RunError> {
        let node = self
            .nodes
            .get_mut(key)
            .expect("the handle's node is in the trace");
        if node.runner.is_none() {
            return Err(RunnerInUse(key).into());
        }
        if !node.rewinds.is_empty() {
            return Err(NotRewound(key).into());
        }
        Ok(node.runner.take().expect("checked above"))
    }

    /// Puts back the runner [`take_runner`](Self::take_runner) took out.
    fn put_runner(&mut self, key: NodeKey, runner: Box<dyn Runner>) {
        self.nodes
            .get_mut(key)
            .expect("a running node can't be deleted")
            .runner = Some(runner);
    }

    /// Registers `rewind` on `key`, which must be in the trace.
    fn add_rewind(&mut self, key: NodeKey, rewind: Rewind) -> Result<RewindKey, InsideRewind> {
        if self.rewind_depth > 0 {
            return Err(InsideRewind);
        }

        let node = self
            .nodes
            .get_mut(key)
            .expect("the handle's node is in the trace");
        let rewind = self.rewinds.insert(RewindEntry { node: key, rewind });
        node.rewinds.push(rewind);
        Ok(rewind)
    }

    /// Takes the rewind `key` out, or returns `None` if it's gone.
    fn take_rewind(&mut self, key: RewindKey) -> Option<Rewind> {
        let entry = self.rewinds.remove(key)?;
        self.nodes
            .get_mut(entry.node)
            .expect("a node with rewinds can't be deleted")
            .rewinds
            .retain(|&mut rewind| rewind != key);
        Some(entry.rewind)
    }

    /// Takes one of `key`'s rewinds out, or returns `None` when it has none
    /// left or isn't in the trace anymore. Which one is unspecified: a random
    /// one in debug builds, the last one otherwise.
    fn take_any_rewind(&mut self, key: NodeKey) -> Option<Rewind> {
        let rewinds = &mut self.nodes.get_mut(key)?.rewinds;
        if rewinds.is_empty() {
            return None;
        }

        #[cfg(debug_assertions)]
        let index = self.shuffle.below(rewinds.len());
        #[cfg(not(debug_assertions))]
        let index = rewinds.len() - 1;

        let rewind = rewinds.swap_remove(index);
        Some(
            self.rewinds
                .remove(rewind)
                .expect("a node's rewinds are stored")
                .rewind,
        )
    }

    /// Makes `child` the last child of `parent`, moving `child`'s subtree
    /// right after `parent`'s.
    /// `child` must be in the trace.
    fn attach(&mut self, parent: NodeKey, child: NodeKey) -> Result<(), SetParentError> {
        // check for an unknown parent
        if !self.contains(parent) {
            return Err(UnknownParent(UnknownNode(parent)).into());
        }

        // check if parent exists already
        if let Some(current_parent) = self
            .nodes
            .get(child)
            .expect("the handle's node is in the trace")
            .parent
        {
            return Err(AlreadyParented {
                child,
                current_parent,
                requested_parent: parent,
            }
            .into());
        }

        // check for cycles
        if parent == child {
            return Err(SelfParent(child).into());
        }
        if let Some(depth) = std::iter::successors(Some(parent), |&key| {
            self.nodes
                .get(key)
                .expect("`parent` and its ancestors are in the trace")
                .parent
        })
        .position(|key| key == child)
        {
            return Err(WouldCycle {
                child,
                parent,
                depth,
            }
            .into());
        }

        // new last node of the parent after adding the child
        let last = self.subtree_last(child);
        // previous last node of the parent
        let prev = self.subtree_last(parent);

        // attach the span
        self.splice(prev, child, last);

        // update the tree
        self.nodes
            .get_mut(child)
            .expect("the handle's node is in the trace")
            .parent = Some(parent);
        self.nodes
            .get_mut(parent)
            .expect("checked above")
            .children
            .push(child);
        Ok(())
    }

    /// Detaches `key` from its parent, cutting its subtree out into an order
    /// of its own, like a new root. Returns whether it had a parent.
    fn detach(&mut self, key: NodeKey) -> bool {
        // parent check
        let Some(parent) = self
            .nodes
            .get_mut(key)
            .expect("the handle's node is in the trace")
            .parent
            .take()
        else {
            return false;
        };

        // update the tree
        self.nodes
            .get_mut(parent)
            .expect("a node's parent is in the trace")
            .children
            .retain(|&mut child| child != key);

        // cut the subtree out of its tree's order
        let last = self.subtree_last(key);
        self.cut(key, last);
        true
    }

    /// Removes `key` from the trace. It must be a leaf that isn't running and
    /// has no rewinds.
    fn remove_leaf(&mut self, key: NodeKey) -> Result<(), DeleteError> {
        let node = self
            .nodes
            .get(key)
            .expect("the handle's node is in the trace");

        // running and rewinds checks
        if node.runner.is_none() {
            return Err(RunnerInUse(key).into());
        }
        if !node.rewinds.is_empty() {
            return Err(HasRewinds(key).into());
        }

        // children check
        let children = node.children.len();
        if children > 0 {
            return Err(HasChildren { key, children }.into());
        }

        let node = self.nodes.remove(key).expect("checked above");
        if let Some(parent) = node.parent {
            self.nodes
                .get_mut(parent)
                .expect("a node's parent is in the trace")
                .children
                .retain(|&mut child| child != key);
        }
        self.link(node.prev, node.next);
        Ok(())
    }
}

fn trace_mut(ctx: &mut Context) -> &mut Trace {
    ctx.get_or_insert_with(Trace::default)
}

/// Runs `rewind`, already taken out of the trace, counted in
/// `Trace::rewind_depth` until it returns or panics.
fn run_taken_rewind(ctx: &mut Context, rewind: Rewind) {
    /// Uncounts the rewind when dropped, so a panicking one is uncounted too.
    struct Depth<'a>(&'a mut Context);

    impl Drop for Depth<'_> {
        fn drop(&mut self) {
            self.0
                .get_mut::<Trace>()
                .expect("a rewind can't remove the trace")
                .rewind_depth -= 1;
        }
    }

    trace_mut(ctx).rewind_depth += 1;
    let depth = Depth(ctx);
    rewind(&mut *depth.0);
}

/// Read access to a node in the trace, returned by [`TraceExt::node`].
#[derive(Clone, Copy)]
pub struct NodeRef<'a> {
    ctx: &'a Context,
    key: NodeKey,
}

impl<'a> NodeRef<'a> {
    fn data(&self) -> &'a Node {
        self.ctx
            .get::<Trace>()
            .expect("a node exists, so the trace does")
            .nodes
            .get(self.key)
            .expect("the handle's node is in the trace")
    }

    pub fn id(&self) -> NodeKey {
        self.key
    }

    /// The context this handle reads from, for extension traits that add
    /// per-node methods.
    pub fn context(&self) -> &'a Context {
        self.ctx
    }

    /// The parent of this node, or `None` for a root.
    pub fn parent(&self) -> Option<NodeKey> {
        self.data().parent
    }

    /// The children of this node, first to last.
    pub fn children(&self) -> &'a [NodeKey] {
        &self.data().children
    }

    /// The node before this one in trace order, or `None` for a root.
    /// See [`Trace`] for the order.
    pub fn prev(&self) -> Option<NodeKey> {
        self.data().prev
    }

    /// The node after this one in trace order, or `None` for the last node of
    /// its tree.
    /// See [`Trace`] for the order.
    pub fn next(&self) -> Option<NodeKey> {
        self.data().next
    }

    /// Whether this node is running: [`NodeMut::run`] has its runner taken
    /// out.
    pub fn is_running(&self) -> bool {
        self.data().runner.is_none()
    }

    /// Whether this node has rewinds registered that haven't run yet.
    pub fn has_rewinds(&self) -> bool {
        !self.data().rewinds.is_empty()
    }
}

/// Write access to a node in the trace, returned by [`TraceExt::node_mut`]
/// and [`TraceExt::create_node`].
///
/// Reparenting is explicit: a node that has a parent must be detached with
/// [`detach`](Self::detach) before it's attached elsewhere. A
/// node's whole subtree moves with it.
pub struct NodeMut<'a> {
    ctx: &'a mut Context,
    key: NodeKey,
}

impl NodeMut<'_> {
    fn trace_mut(&mut self) -> &mut Trace {
        self.ctx
            .get_mut::<Trace>()
            .expect("a node exists, so the trace does")
    }

    pub fn id(&self) -> NodeKey {
        self.key
    }

    /// The context this handle writes to, for extension traits that add
    /// per-node methods.
    pub fn context(&self) -> &Context {
        self.ctx
    }

    /// Mutable access to the context this handle writes to, for extension
    /// traits that add per-node methods.
    ///
    /// # Danger
    ///
    /// Code using the context directly can change the trace behind the
    /// handle's back, and may also run or rewind nodes, this one included. It
    /// must not delete this node: the handle's methods may panic afterwards.
    /// Extension traits must leave the handle's node in place.
    pub fn context_mut(&mut self) -> &mut Context {
        self.ctx
    }

    pub fn as_ref(&self) -> NodeRef<'_> {
        NodeRef {
            ctx: self.ctx,
            key: self.key,
        }
    }

    /// The parent of this node, or `None` for a root.
    pub fn parent(&self) -> Option<NodeKey> {
        self.as_ref().parent()
    }

    /// The children of this node, first to last.
    pub fn children(&self) -> &[NodeKey] {
        self.as_ref().children()
    }

    /// The node before this one in trace order, or `None` for a root.
    pub fn prev(&self) -> Option<NodeKey> {
        self.as_ref().prev()
    }

    /// The node after this one in trace order, or `None` for the last node of
    /// its tree.
    pub fn next(&self) -> Option<NodeKey> {
        self.as_ref().next()
    }

    /// Whether this node is running: [`run`](Self::run) has its runner taken
    /// out.
    pub fn is_running(&self) -> bool {
        self.as_ref().is_running()
    }

    /// Whether this node has rewinds registered that haven't run yet.
    pub fn has_rewinds(&self) -> bool {
        self.as_ref().has_rewinds()
    }

    /// Calls this node's runner with this node's key.
    ///
    /// Fails if the node is already running, or if it still has rewinds
    /// registered: [`rewind`](Self::rewind) it first. If the runner panics,
    /// it's put back before the panic is passed on.
    pub fn run(&mut self) -> Result<(), RunError> {
        let key = self.key;
        let mut runner = self.trace_mut().take_runner(key)?;
        let result = panic::catch_unwind(AssertUnwindSafe(|| runner.run(self.ctx, key)));
        self.trace_mut().put_runner(key, runner);
        if let Err(payload) = result {
            panic::resume_unwind(payload);
        }
        Ok(())
    }

    /// Registers `f` as a rewind of this node, to be run by
    /// [`rewind`](Self::rewind) or [`run_rewind`](TraceExt::run_rewind).
    /// Any node can have rewinds registered, running or not.
    ///
    /// Fails while any rewind is running.
    pub fn register_rewind(
        &mut self,
        f: impl FnOnce(&mut Context) + 'static,
    ) -> Result<RewindKey, InsideRewind> {
        let key = self.key;
        self.trace_mut().add_rewind(key, Box::new(f))
    }

    /// Runs this node's rewinds, one at a time, until it has none left.
    ///
    /// Which of the remaining rewinds runs next is unspecified, and code must
    /// not depend on it: debug builds pick one at random. To run a rewind
    /// before another, have the other one call
    /// [`run_rewind`](TraceExt::run_rewind) on it.
    ///
    /// Each rewind is taken out just before it runs, and the rest stay
    /// registered, so it can run them by key or rewind other nodes. A rewind
    /// that's taken out is gone even if it panics; the rest stay registered,
    /// and calling `rewind` again continues. A rewind may delete this node
    /// once it has no rewinds left; `rewind` then returns, and the handle's
    /// other methods may panic.
    pub fn rewind(&mut self) {
        let key = self.key;
        // The node is read again every time, in case a rewind deleted it.
        while let Some(rewind) = self.trace_mut().take_any_rewind(key) {
            run_taken_rewind(self.ctx, rewind);
        }
    }

    /// Makes this node the last child of `parent`.
    ///
    /// Fails if `parent` isn't in the trace, if this node already has a
    /// parent, or if `parent` is this node or one of its descendants.
    pub fn set_parent(&mut self, parent: NodeKey) -> Result<&mut Self, SetParentError> {
        let key = self.key;
        self.trace_mut().attach(parent, key)?;
        Ok(self)
    }

    /// Makes `child` this node's last child.
    ///
    /// Fails if `child` isn't in the trace, if it already has a parent, or if
    /// it's this node or one of its ancestors.
    pub fn add_child(&mut self, child: NodeKey) -> Result<&mut Self, AddChildError> {
        let parent = self.key;
        self.ctx
            .node_mut(child)
            .map_err(UnknownChild)?
            .set_parent(parent)
            .map_err(|error| match error {
                SetParentError::UnknownParent(_) => unreachable!("this node is in the trace"),
                SetParentError::AlreadyParented(error) => AddChildError::from(error),
                SetParentError::SelfParent(error) => AddChildError::from(error),
                SetParentError::WouldCycle(error) => AddChildError::from(error),
            })?;
        Ok(self)
    }

    /// Detaches this node from its parent, making it a root whose subtree is
    /// an order of its own. Returns whether it had one.
    pub fn detach(&mut self) -> bool {
        let key = self.key;
        self.trace_mut().detach(key)
    }

    /// Deletes this node, which must be a leaf that isn't running and has no
    /// rewinds. Detaches it from its parent first if it has one.
    pub fn delete(self) -> Result<(), DeleteError> {
        let Self { ctx, key } = self;
        trace_mut(ctx).remove_leaf(key)
    }
}

pub trait TraceExt {
    /// Whether `key` is in the trace.
    fn contains_node(&self, key: NodeKey) -> bool;

    /// Read access to `key`.
    fn node(&self, key: NodeKey) -> Result<NodeRef<'_>, UnknownNode>;

    /// Write access to `key`.
    fn node_mut(&mut self, key: NodeKey) -> Result<NodeMut<'_>, UnknownNode>;

    /// Creates a node run by `runner` that is not parented and has no
    /// children. It's a root until attached with
    /// [`set_parent`](NodeMut::set_parent) or
    /// [`add_child`](NodeMut::add_child), alone in its own trace order.
    fn create_node(&mut self, runner: impl Runner) -> NodeMut<'_>;

    /// Runs the rewind `key` now, taking it off its node, and returns
    /// `true`. Returns `false` if it's gone because it already ran.
    ///
    /// A key that never existed looks the same as one that already ran.
    /// That's fine: rewind keys only come from
    /// [`register_rewind`](NodeMut::register_rewind), and keys from other
    /// contexts are unsupported.
    fn run_rewind(&mut self, key: RewindKey) -> bool;
}

impl TraceExt for Context {
    fn contains_node(&self, key: NodeKey) -> bool {
        self.get::<Trace>().is_some_and(|trace| trace.contains(key))
    }

    fn node(&self, key: NodeKey) -> Result<NodeRef<'_>, UnknownNode> {
        if !self.contains_node(key) {
            return Err(UnknownNode(key));
        }
        Ok(NodeRef { ctx: self, key })
    }

    fn node_mut(&mut self, key: NodeKey) -> Result<NodeMut<'_>, UnknownNode> {
        if !self.contains_node(key) {
            return Err(UnknownNode(key));
        }
        Ok(NodeMut { ctx: self, key })
    }

    fn create_node(&mut self, runner: impl Runner) -> NodeMut<'_> {
        let key = trace_mut(self).insert_root(Box::new(runner));
        NodeMut { ctx: self, key }
    }

    fn run_rewind(&mut self, key: RewindKey) -> bool {
        let Some(rewind) = self
            .get_mut::<Trace>()
            .and_then(|trace| trace.take_rewind(key))
        else {
            return false;
        };
        run_taken_rewind(self, rewind);
        true
    }
}

#[cfg(test)]
mod tests {
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

    fn child(ctx: &mut Context, parent: NodeKey) -> NodeKey {
        let mut node = ctx.create_node(noop);
        node.set_parent(parent).unwrap();
        node.id()
    }

    /// ```text
    /// root
    /// ├── a
    /// │   ├── a1
    /// │   └── a2
    /// └── b
    ///     └── b1
    /// ```
    fn tree() -> (Context, NodeKey, [NodeKey; 5]) {
        let mut ctx = Context::new();
        let root = ctx.create_node(noop).id();
        let a = child(&mut ctx, root);
        let a1 = child(&mut ctx, a);
        let a2 = child(&mut ctx, a);
        let b = child(&mut ctx, root);
        let b1 = child(&mut ctx, b);
        (ctx, root, [a, a1, a2, b, b1])
    }

    /// `root`'s tree following `next` from `root`, after checking that the
    /// order starts at `root`, stays inside its tree, and that `prev` links it
    /// backwards.
    fn tree_order(ctx: &Context, root: NodeKey) -> Vec<NodeKey> {
        assert_eq!(ctx.node(root).unwrap().parent(), None, "not a root");
        assert_eq!(ctx.node(root).unwrap().prev(), None, "a root has no prev");
        let forward: Vec<_> =
            std::iter::successors(Some(root), |&key| ctx.node(key).unwrap().next()).collect();
        for &key in &forward {
            let key_root = std::iter::successors(Some(key), |&key| ctx.node(key).unwrap().parent())
                .last()
                .unwrap();
            assert_eq!(key_root, root, "{key:?} is in another tree");
        }
        let last = *forward.last().unwrap();
        let mut backward: Vec<_> =
            std::iter::successors(Some(last), |&key| ctx.node(key).unwrap().prev()).collect();
        backward.reverse();
        assert_eq!(forward, backward, "prev and next disagree");
        forward
    }

    #[gtest]
    fn trace_order_is_parents_first() {
        let (ctx, root, [a, a1, a2, b, b1]) = tree();
        expect_eq!(tree_order(&ctx, root), [root, a, a1, a2, b, b1]);
        // A first child's prev is its parent; any other node's prev is its
        // previous sibling's last descendant.
        expect_that!(ctx.node(a1).unwrap().prev(), some(eq(a)));
        expect_that!(ctx.node(b).unwrap().prev(), some(eq(a2)));
    }

    #[gtest]
    fn roots_have_separate_orders() {
        let (mut ctx, root, [a, a1, a2, b, b1]) = tree();
        let root2 = ctx.create_node(noop).id();
        let c = child(&mut ctx, root2);
        let a3 = child(&mut ctx, a);
        expect_eq!(tree_order(&ctx, root), [root, a, a1, a2, a3, b, b1]);
        expect_eq!(tree_order(&ctx, root2), [root2, c]);
        expect_that!(ctx.node(b1).unwrap().next(), none());
    }

    #[gtest]
    fn deletion_keeps_the_trace_order_linked() {
        let (mut ctx, root, [a, a1, a2, b, b1]) = tree();
        let root2 = ctx.create_node(noop).id();

        ctx.node_mut(a1).unwrap().delete().unwrap();
        expect_eq!(tree_order(&ctx, root), [root, a, a2, b, b1]);
        expect_eq!(ctx.node(a).unwrap().children(), &[a2]);

        ctx.node_mut(b1).unwrap().delete().unwrap();
        expect_eq!(tree_order(&ctx, root), [root, a, a2, b]);

        ctx.node_mut(a2).unwrap().delete().unwrap();
        ctx.node_mut(a).unwrap().delete().unwrap();
        expect_eq!(tree_order(&ctx, root), [root, b]);

        ctx.node_mut(b).unwrap().delete().unwrap();
        ctx.node_mut(root).unwrap().delete().unwrap();
        expect_false!(ctx.contains_node(root));
        expect_eq!(tree_order(&ctx, root2), [root2]);
    }

    #[gtest]
    fn set_parent_and_add_child_attach_as_last_child() {
        let (mut ctx, root, [a, a1, a2, b, b1]) = tree();
        let c = ctx.create_node(noop).id();
        let d = ctx.create_node(noop).id();

        ctx.node_mut(c).unwrap().set_parent(a).unwrap();
        expect_eq!(ctx.node(a).unwrap().children(), &[a1, a2, c]);
        expect_that!(ctx.node(c).unwrap().parent(), some(eq(a)));

        ctx.node_mut(a).unwrap().add_child(d).unwrap();
        expect_eq!(ctx.node(a).unwrap().children(), &[a1, a2, c, d]);
        expect_that!(ctx.node(d).unwrap().parent(), some(eq(a)));

        expect_eq!(tree_order(&ctx, root), [root, a, a1, a2, c, d, b, b1]);
    }

    #[gtest]
    fn attaching_a_parented_node_is_refused() {
        let (mut ctx, root, [a, a1, _, b, _]) = tree();
        let a1_under_b = AlreadyParented {
            child: a1,
            current_parent: a,
            requested_parent: b,
        };

        expect_that!(
            ctx.node_mut(a1).unwrap().set_parent(b).err(),
            some(eq(SetParentError::AlreadyParented(a1_under_b)))
        );
        expect_that!(
            ctx.node_mut(b).unwrap().add_child(a1).err(),
            some(eq(AddChildError::AlreadyParented(a1_under_b)))
        );
        // Even under its own parent.
        expect_that!(
            ctx.node_mut(root).unwrap().add_child(b).err(),
            some(eq(AddChildError::AlreadyParented(AlreadyParented {
                child: b,
                current_parent: root,
                requested_parent: root,
            })))
        );
        expect_that!(ctx.node(a1).unwrap().parent(), some(eq(a)));
    }

    #[gtest]
    fn self_parenting_is_refused() {
        let (mut ctx, root, _) = tree();

        expect_that!(
            ctx.node_mut(root).unwrap().set_parent(root).err(),
            some(eq(SetParentError::SelfParent(SelfParent(root))))
        );
        expect_that!(
            ctx.node_mut(root).unwrap().add_child(root).err(),
            some(eq(AddChildError::SelfParent(SelfParent(root))))
        );
        expect_that!(ctx.node(root).unwrap().parent(), none());
    }

    #[gtest]
    fn cycles_are_refused_with_their_depth() {
        let (mut ctx, root, [a, a1, ..]) = tree();

        expect_that!(
            ctx.node_mut(root).unwrap().set_parent(a).err(),
            some(eq(SetParentError::WouldCycle(WouldCycle {
                child: root,
                parent: a,
                depth: 1,
            })))
        );
        expect_that!(
            ctx.node_mut(root).unwrap().set_parent(a1).err(),
            some(eq(SetParentError::WouldCycle(WouldCycle {
                child: root,
                parent: a1,
                depth: 2,
            })))
        );
        expect_that!(
            ctx.node_mut(a1).unwrap().add_child(root).err(),
            some(eq(AddChildError::WouldCycle(WouldCycle {
                child: root,
                parent: a1,
                depth: 2,
            })))
        );
        expect_that!(ctx.node(root).unwrap().parent(), none());
        expect_eq!(tree_order(&ctx, root)[..3], [root, a, a1]);
    }

    #[gtest]
    fn unknown_other_keys_are_refused_by_role() {
        let (mut ctx, root, [a, ..]) = tree();
        let deleted = ctx.create_node(noop).id();
        ctx.node_mut(deleted).unwrap().delete().unwrap();

        expect_that!(
            ctx.node_mut(a).unwrap().add_child(deleted).err(),
            some(eq(AddChildError::UnknownChild(UnknownChild(UnknownNode(
                deleted
            )))))
        );
        ctx.node_mut(a).unwrap().detach();
        expect_that!(
            ctx.node_mut(a).unwrap().set_parent(deleted).err(),
            some(eq(SetParentError::UnknownParent(UnknownParent(
                UnknownNode(deleted)
            ))))
        );
        expect_that!(ctx.node(a).unwrap().parent(), none());
        expect_that!(ctx.node(root).unwrap().children().len(), eq(1));
    }

    #[gtest]
    fn deleted_nodes_are_unknown() {
        let (mut ctx, _, [a, a1, a2, ..]) = tree();
        for key in [a1, a2, a] {
            ctx.node_mut(key).unwrap().delete().unwrap();
        }

        expect_false!(ctx.contains_node(a1));
        expect_that!(ctx.node(a1).err(), some(eq(UnknownNode(a1))));
        expect_that!(ctx.node_mut(a).err(), some(eq(UnknownNode(a))));
    }

    #[gtest]
    fn detach_reports_whether_there_was_a_parent() {
        let (mut ctx, root, [a, _, _, b, _]) = tree();
        let mut node = ctx.node_mut(a).unwrap();

        expect_true!(node.detach());
        expect_false!(node.detach());
        expect_that!(node.parent(), none());
        expect_eq!(ctx.node(root).unwrap().children(), &[b]);
    }

    #[gtest]
    fn delete_refuses_nodes_with_children() {
        let (mut ctx, _, [a, a1, a2, ..]) = tree();

        expect_that!(
            ctx.node_mut(a).unwrap().delete().err(),
            some(eq(DeleteError::HasChildren(HasChildren {
                key: a,
                children: 2
            })))
        );
        expect_eq!(ctx.node(a).unwrap().children(), &[a1, a2]);
    }

    #[gtest]
    fn moving_subtrees_keeps_the_trace_order_linked() {
        let (mut ctx, root, [a, a1, a2, b, b1]) = tree();
        let root2 = ctx.create_node(noop).id();

        // Detaching cuts the whole subtree out into an order of its own.
        ctx.node_mut(a).unwrap().detach();
        expect_eq!(tree_order(&ctx, root), [root, b, b1]);
        expect_eq!(tree_order(&ctx, a), [a, a1, a2]);

        // Attaching puts it right after the new parent's last descendant.
        ctx.node_mut(a).unwrap().set_parent(b).unwrap();
        expect_eq!(tree_order(&ctx, root), [root, b, b1, a, a1, a2]);

        ctx.node_mut(a1).unwrap().add_child(root2).unwrap();
        expect_eq!(tree_order(&ctx, root), [root, b, b1, a, a1, root2, a2]);

        ctx.node_mut(b).unwrap().detach();
        expect_eq!(tree_order(&ctx, root), [root]);
        expect_eq!(tree_order(&ctx, b), [b, b1, a, a1, root2, a2]);

        ctx.node_mut(b).unwrap().set_parent(root).unwrap();
        ctx.node_mut(b1).unwrap().detach();
        expect_eq!(tree_order(&ctx, root), [root, b, a, a1, root2, a2]);
        expect_eq!(tree_order(&ctx, b1), [b1]);
    }

    #[gtest]
    fn rewind_runs_every_rewind_once() {
        let mut ctx = Context::new();
        let mut node = ctx.create_node(noop);
        let keys: Vec<_> = (0..5)
            .map(|i| node.register_rewind(move |ctx| log(ctx, i)).unwrap())
            .collect();
        expect_true!(node.has_rewinds());

        node.rewind();
        expect_false!(node.has_rewinds());
        node.rewind();
        let mut ran = logged(&ctx);
        ran.sort();
        expect_eq!(ran, ["0", "1", "2", "3", "4"]);
        for key in keys {
            expect_false!(ctx.run_rewind(key));
        }
        expect_true!(ctx.get::<Trace>().unwrap().rewinds.is_empty());
    }

    #[cfg(debug_assertions)]
    #[gtest]
    fn rewind_order_varies_in_debug_builds() {
        let orders: std::collections::HashSet<_> = (0..8)
            .map(|_| {
                let mut ctx = Context::new();
                let mut node = ctx.create_node(noop);
                for i in 0..8 {
                    node.register_rewind(move |ctx| log(ctx, i)).unwrap();
                }
                node.rewind();
                logged(&ctx)
            })
            .collect();
        expect_that!(orders.len(), gt(1));
    }

    #[gtest]
    fn run_rewind_runs_its_target_now_and_only_once() {
        // Which rewind `rewind` picks first varies, so try it a few times.
        for _ in 0..8 {
            let mut ctx = Context::new();
            let mut node = ctx.create_node(noop);
            let inner = node.register_rewind(|ctx| log(ctx, "inner")).unwrap();
            node.register_rewind(move |ctx| {
                let inner_ran = !logged(ctx).is_empty();
                expect_eq!(ctx.run_rewind(inner), !inner_ran);
                expect_eq!(logged(ctx), ["inner"]);
                expect_false!(ctx.run_rewind(inner));
                log(ctx, "outer");
            })
            .unwrap();

            node.rewind();
            expect_eq!(logged(&ctx), ["inner", "outer"]);
        }
    }

    #[gtest]
    fn a_rewind_can_rewind_another_node() {
        let mut ctx = Context::new();
        let other = ctx.create_node(noop).id();
        for i in 0..3 {
            ctx.node_mut(other)
                .unwrap()
                .register_rewind(move |ctx| log(ctx, i))
                .unwrap();
        }
        let mut node = ctx.create_node(noop);
        node.register_rewind(move |ctx| {
            ctx.node_mut(other).unwrap().rewind();
            log(ctx, "outer");
        })
        .unwrap();

        node.rewind();
        expect_false!(ctx.node(other).unwrap().has_rewinds());
        let log = logged(&ctx);
        let (outer, inner) = log.split_last().unwrap();
        let mut inner = inner.to_vec();
        inner.sort();
        expect_eq!(inner, ["0", "1", "2"]);
        expect_eq!(outer, "outer");
    }

    #[gtest]
    fn registering_inside_a_rewind_is_refused() {
        let mut ctx = Context::new();
        let outer = ctx.create_node(noop).id();
        let inner = ctx.create_node(noop).id();
        ctx.node_mut(inner)
            .unwrap()
            .register_rewind(move |ctx| {
                expect_that!(
                    ctx.node_mut(outer).unwrap().register_rewind(|_| {}).err(),
                    some(eq(InsideRewind))
                );
                log(ctx, "inner");
            })
            .unwrap();
        ctx.node_mut(outer)
            .unwrap()
            .register_rewind(move |ctx| {
                ctx.node_mut(inner).unwrap().rewind();
                // The nested rewind is over, but this one is still running.
                expect_that!(
                    ctx.node_mut(inner).unwrap().register_rewind(|_| {}).err(),
                    some(eq(InsideRewind))
                );
                log(ctx, "outer");
            })
            .unwrap();

        ctx.node_mut(outer).unwrap().rewind();
        expect_eq!(logged(&ctx), ["inner", "outer"]);
        expect_false!(ctx.node(inner).unwrap().has_rewinds());
        expect_false!(ctx.node(outer).unwrap().has_rewinds());
        expect_true!(ctx.node_mut(outer).unwrap().register_rewind(|_| {}).is_ok());
    }

    #[gtest]
    fn run_needs_the_node_rewound() {
        let mut ctx = Context::new();
        let mut node = ctx.create_node(|ctx: &mut Context, _: NodeKey| log(ctx, "ran"));
        let key = node.id();
        node.register_rewind(|ctx| log(ctx, "rewound")).unwrap();

        expect_that!(
            node.run().err(),
            some(eq(RunError::NotRewound(NotRewound(key))))
        );
        node.rewind();
        expect_eq!(node.run(), Ok(()));
        expect_eq!(logged(&ctx), ["rewound", "ran"]);
    }

    #[gtest]
    fn a_runner_gets_its_node_and_cant_run_it_again() {
        struct RanAs(NodeKey);

        let mut ctx = Context::new();
        let key = ctx
            .create_node(|ctx: &mut Context, key: NodeKey| {
                expect_true!(ctx.node(key).unwrap().is_running());
                expect_that!(
                    ctx.node_mut(key).unwrap().run().err(),
                    some(eq(RunError::RunnerInUse(RunnerInUse(key))))
                );
                ctx.insert(RanAs(key));
            })
            .id();

        expect_false!(ctx.node(key).unwrap().is_running());
        expect_eq!(ctx.node_mut(key).unwrap().run(), Ok(()));
        expect_that!(ctx.get::<RanAs>().map(|ran| ran.0), some(eq(key)));
        expect_false!(ctx.node(key).unwrap().is_running());
    }

    #[gtest]
    fn delete_refuses_running_nodes_and_nodes_with_rewinds() {
        let mut ctx = Context::new();
        let running = ctx
            .create_node(|ctx: &mut Context, key: NodeKey| {
                expect_that!(
                    ctx.node_mut(key).unwrap().delete().err(),
                    some(eq(DeleteError::RunnerInUse(RunnerInUse(key))))
                );
            })
            .id();
        ctx.node_mut(running).unwrap().run().unwrap();
        expect_true!(ctx.contains_node(running));

        let mut node = ctx.create_node(noop);
        let rewound = node.id();
        node.register_rewind(|_| {}).unwrap();
        expect_that!(
            node.delete().err(),
            some(eq(DeleteError::HasRewinds(HasRewinds(rewound))))
        );
        ctx.node_mut(rewound).unwrap().rewind();
        expect_eq!(ctx.node_mut(rewound).unwrap().delete(), Ok(()));
    }

    #[gtest]
    fn a_panicking_runner_is_put_back() {
        let mut ctx = Context::new();
        let key = ctx
            .create_node(|ctx: &mut Context, _: NodeKey| {
                log(ctx, "ran");
                if logged(ctx).len() == 1 {
                    panic!("the first run panics");
                }
            })
            .id();

        let result = panic::catch_unwind(AssertUnwindSafe(|| ctx.node_mut(key).unwrap().run()));
        expect_true!(result.is_err());
        expect_false!(ctx.node(key).unwrap().is_running());
        expect_eq!(ctx.node_mut(key).unwrap().run(), Ok(()));
        expect_eq!(logged(&ctx), ["ran", "ran"]);
    }

    #[gtest]
    fn a_panicking_rewind_leaves_the_rest_registered() {
        let mut ctx = Context::new();
        let mut node = ctx.create_node(noop);
        let key = node.id();
        node.register_rewind(|_| panic!("the rewind panics"))
            .unwrap();
        for i in 0..4 {
            node.register_rewind(move |ctx| log(ctx, i)).unwrap();
        }

        let result = panic::catch_unwind(AssertUnwindSafe(|| ctx.node_mut(key).unwrap().rewind()));
        expect_true!(result.is_err());
        expect_eq!(ctx.get::<Trace>().unwrap().rewind_depth, 0);
        // The ones that didn't run before the panic are still registered.
        expect_eq!(ctx.node(key).unwrap().has_rewinds(), logged(&ctx).len() < 4);

        ctx.node_mut(key).unwrap().rewind();
        expect_false!(ctx.node(key).unwrap().has_rewinds());
        let mut ran = logged(&ctx);
        ran.sort();
        expect_eq!(ran, ["0", "1", "2", "3"]);
    }

    #[gtest]
    fn a_rewind_can_delete_its_own_node() {
        let mut ctx = Context::new();
        let key = ctx.create_node(noop).id();
        let mut node = ctx.node_mut(key).unwrap();
        let first = node.register_rewind(|ctx| log(ctx, "first")).unwrap();
        node.register_rewind(move |ctx| {
            // Deleting needs the node's other rewinds to have run.
            ctx.run_rewind(first);
            ctx.node_mut(key).unwrap().delete().unwrap();
            log(ctx, "deleted");
        })
        .unwrap();

        node.rewind();
        expect_false!(ctx.contains_node(key));
        expect_eq!(logged(&ctx), ["first", "deleted"]);
    }
}
