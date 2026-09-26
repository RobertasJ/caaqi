use std::{
    any::type_name,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
};

use slotmap::SecondaryMap;

use crate::{
    action_tree::{ActionNodeKey, ActionTree, ActionTreeExt, NodeExecuting, UnknownNode, tree_ref},
    context::Context,
    current::CurrentActionExt,
    lifecycle::{LifecycleExt, NodeObserver},
};

/// Storage is split by output type, so a node's action can only be found by
/// asking for the output it was stored with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error(
    "action node {key:?} has no stored action with output `{output}`; \
     it may store one with a different output"
)]
pub struct NoStoredAction {
    pub key: ActionNodeKey,
    pub output: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RunActionError {
    #[error(transparent)]
    UnknownNode(#[from] UnknownNode),
    #[error(transparent)]
    NodeExecuting(#[from] NodeExecuting),
    #[error(transparent)]
    NoStoredAction(#[from] NoStoredAction),
}

pub struct BoxAction<O> {
    action: Box<dyn Action<Output = O>>,
}

impl<O> BoxAction<O> {
    pub fn new(action: impl Action<Output = O> + 'static) -> Self {
        Self {
            action: Box::new(action),
        }
    }
}

impl<O> Action for BoxAction<O> {
    type Output = O;

    fn run(&mut self, ctx: &mut Context) -> Self::Output {
        self.action.run(ctx)
    }
}

pub trait Action {
    type Output;

    fn run(&mut self, ctx: &mut Context) -> Self::Output;
}

impl<F: FnMut(&mut Context) -> T, T> Action for F {
    type Output = T;

    fn run(&mut self, ctx: &mut Context) -> Self::Output {
        self(ctx)
    }
}

/// The stored actions whose output is `O`, one resource per output type.
/// Entries leave when their node is removed from the tree.
pub struct ActionStorage<O> {
    actions: SecondaryMap<ActionNodeKey, BoxAction<O>>,
}

impl<O> Default for ActionStorage<O> {
    fn default() -> Self {
        Self {
            actions: SecondaryMap::new(),
        }
    }
}

impl<O> ActionStorage<O> {
    fn contains(&self, key: ActionNodeKey) -> bool {
        self.actions.contains_key(key)
    }
}

impl<O: 'static> NodeObserver for ActionStorage<O> {
    fn nodes_removed(ctx: &mut Context, keys: &[ActionNodeKey]) {
        if let Some(storage) = ctx.get_mut::<ActionStorage<O>>() {
            for &key in keys {
                storage.actions.remove(key);
            }
        }
    }
}

fn storage_mut<O: 'static>(ctx: &mut Context) -> &mut ActionStorage<O> {
    if ctx.get::<ActionStorage<O>>().is_none() {
        ctx.observe_nodes::<ActionStorage<O>>();
    }
    ctx.get_or_insert_with(ActionStorage::<O>::default)
}

fn store<A: Action + 'static>(ctx: &mut Context, key: ActionNodeKey, action: A) {
    let replaced = storage_mut::<A::Output>(ctx)
        .actions
        .insert(key, BoxAction::new(action));
    assert!(replaced.is_none(), "the node was just created");
}

pub trait ActionExt {
    /// Creates a node under the executing action (or a root outside
    /// execution) and runs `action` with that node as the current action.
    ///
    /// The action body is dropped afterwards, so the node only records where
    /// the action ran and can't be rerun. Use
    /// [`create_root_action`](Self::create_root_action) or
    /// [`create_branch_action`](Self::create_branch_action) for rerunnable
    /// actions.
    fn run_node<A: Action>(&mut self, action: A) -> (ActionNodeKey, A::Output);

    /// Creates a root storing `action`, without running it.
    fn create_root_action<A: Action + 'static>(&mut self, action: A) -> ActionNodeKey;

    /// Creates a child of the executing action (or a root outside execution)
    /// storing `action`, without running it.
    fn create_branch_action<A: Action + 'static>(&mut self, action: A) -> ActionNodeKey;

    /// Whether `key` stores an action with output `O`. Actions taken out to
    /// run aren't stored until they finish.
    fn has_stored_action<O: 'static>(&self, key: ActionNodeKey) -> Result<bool, UnknownNode>;

    /// Reruns the action stored at `key`, whose output must be `O`, with `key`
    /// as the current action. Its descendants are removed first, so the run
    /// recreates them.
    fn run_action<O: 'static>(&mut self, key: ActionNodeKey) -> Result<O, RunActionError>;
}

impl ActionExt for Context {
    fn has_stored_action<O: 'static>(&self, key: ActionNodeKey) -> Result<bool, UnknownNode> {
        tree_ref(self, key)?.node(key)?;
        Ok(self
            .get::<ActionStorage<O>>()
            .is_some_and(|storage| storage.contains(key)))
    }

    fn run_node<A: Action>(&mut self, mut action: A) -> (ActionNodeKey, A::Output) {
        let key = self.create_branch();
        let output = self
            .with_current_action(key, |ctx| action.run(ctx))
            .expect("the node was just created");
        (key, output)
    }

    fn create_root_action<A: Action + 'static>(&mut self, action: A) -> ActionNodeKey {
        let key = self.create_root();
        store(self, key, action);
        key
    }

    fn create_branch_action<A: Action + 'static>(&mut self, action: A) -> ActionNodeKey {
        let key = self.create_branch();
        store(self, key, action);
        key
    }

    fn run_action<O: 'static>(&mut self, key: ActionNodeKey) -> Result<O, RunActionError> {
        self.get_or_insert_with(ActionTree::default).node(key)?;
        // Also rules out executing descendants, which would make `key` an
        // ancestor of the current action.
        if self.has_executing(key) {
            return Err(NodeExecuting(key).into());
        }
        // Taken out so the action can use the whole context while it runs.
        let mut action = storage_mut::<O>(self)
            .actions
            .remove(key)
            .ok_or(NoStoredAction {
                key,
                output: type_name::<O>(),
            })?;
        self.clear_children(key)
            .expect("the node is in the tree and nothing below it is executing");
        let result = catch_unwind(AssertUnwindSafe(|| {
            self.with_current_action(key, |ctx| action.run(ctx))
                .expect("the node is in the tree")
        }));
        // Executing nodes can't be removed, so `key` is still in the tree.
        storage_mut::<O>(self).actions.insert(key, action);
        match result {
            Ok(output) => Ok(output),
            Err(panic) => resume_unwind(panic),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, rc::Rc};

    use googletest::prelude::*;

    use super::*;
    use crate::current::NotExecuting;

    #[gtest]
    fn create_root_action_stores_without_running() {
        let mut ctx = Context::new();
        let runs = Rc::new(Cell::new(0));
        let key = ctx.create_root_action({
            let runs = runs.clone();
            move |_: &mut Context| runs.set(runs.get() + 1)
        });

        expect_eq!(runs.get(), 0);
        expect_that!(ctx.has_stored_action::<()>(key), ok(eq(true)));
        expect_that!(ctx.parent(key), ok(none()));
    }

    #[gtest]
    fn create_branch_action_parents_under_the_current_action_if_any() {
        let mut ctx = Context::new();
        let root = ctx.create_branch_action(|_: &mut Context| ());
        expect_that!(ctx.parent(root), ok(none()));
        expect_that!(ctx.has_stored_action::<()>(root), ok(eq(true)));

        let (parent, child) =
            ctx.run_node(|ctx: &mut Context| ctx.create_branch_action(|_: &mut Context| 1));
        expect_that!(ctx.parent(child), ok(some(eq(parent))));
        expect_that!(ctx.has_stored_action::<i32>(child), ok(eq(true)));
    }

    #[gtest]
    fn run_action_reruns_and_keeps_the_action() {
        let mut ctx = Context::new();
        let mut count = 0;
        let key = ctx.create_root_action(move |_: &mut Context| {
            count += 1;
            count
        });

        expect_that!(ctx.run_action::<i32>(key), ok(eq(1)));
        expect_that!(ctx.run_action::<i32>(key), ok(eq(2)));
        expect_that!(ctx.has_stored_action::<i32>(key), ok(eq(true)));
    }

    #[gtest]
    fn run_action_sets_the_current_action() {
        let mut ctx = Context::new();
        let key = ctx.create_root_action(|ctx: &mut Context| ctx.current_action());

        expect_that!(
            ctx.run_action::<Result<ActionNodeKey, NotExecuting>>(key),
            ok(ok(eq(key)))
        );
        expect_that!(ctx.current_action(), err(eq(NotExecuting)));
    }

    #[gtest]
    fn run_action_recreates_descendants() {
        let mut ctx = Context::new();
        let key = ctx.create_root_action(|ctx: &mut Context| ctx.create_branch());

        let first = ctx.run_action::<ActionNodeKey>(key).unwrap();
        let second = ctx.run_action::<ActionNodeKey>(key).unwrap();
        expect_false!(ctx.contains_node(first));
        expect_that!(ctx.children(key), ok(eq(&[second][..])));
    }

    #[gtest]
    fn run_action_rejects_the_wrong_output_type() {
        let mut ctx = Context::new();
        let key = ctx.create_root_action(|_: &mut Context| 1_i32);

        expect_that!(
            ctx.run_action::<u8>(key),
            err(eq(RunActionError::NoStoredAction(NoStoredAction {
                key,
                output: type_name::<u8>(),
            })))
        );
        expect_that!(ctx.has_stored_action::<i32>(key), ok(eq(true)));
    }

    #[gtest]
    fn run_action_rejects_nodes_without_an_action() {
        let mut ctx = Context::new();
        let key = ctx.create_root();

        expect_that!(
            ctx.run_action::<()>(key),
            err(eq(RunActionError::NoStoredAction(NoStoredAction {
                key,
                output: type_name::<()>(),
            })))
        );
    }

    #[gtest]
    fn run_action_rejects_unknown_nodes() {
        let foreign = Context::new().create_root();
        let mut ctx = Context::new();
        expect_that!(
            ctx.run_action::<()>(foreign),
            err(eq(RunActionError::UnknownNode(UnknownNode(foreign))))
        );
    }

    #[gtest]
    fn run_action_rejects_executing_nodes() {
        let mut ctx = Context::new();
        let key = ctx.create_root_action(|ctx: &mut Context| {
            let key = ctx.current_action().unwrap();
            ctx.run_action::<Result<(), RunActionError>>(key).map(drop)
        });

        expect_that!(
            ctx.run_action::<Result<(), RunActionError>>(key),
            ok(err(eq(RunActionError::NodeExecuting(NodeExecuting(key)))))
        );
    }

    #[gtest]
    fn run_action_keeps_the_action_after_a_panic() {
        let mut ctx = Context::new();
        let key = ctx.create_root_action(|_: &mut Context| -> i32 { panic!("boom") });

        let result = catch_unwind(AssertUnwindSafe(|| ctx.run_action::<i32>(key)));
        expect_true!(result.is_err());
        expect_that!(ctx.has_stored_action::<i32>(key), ok(eq(true)));
        expect_that!(ctx.current_action(), err(eq(NotExecuting)));
    }

    #[gtest]
    fn removing_the_node_drops_its_action() {
        let mut ctx = Context::new();
        let key = ctx.create_root_action(|_: &mut Context| ());
        ctx.remove_node(key).unwrap();

        expect_that!(ctx.has_stored_action::<()>(key), err(eq(UnknownNode(key))));
        expect_false!(ctx.get::<ActionStorage<()>>().unwrap().contains(key));
    }
}
