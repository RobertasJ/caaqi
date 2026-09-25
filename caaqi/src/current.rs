use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

use crate::{
    action_tree::{ActionNodeKey, ActionTree, ActionTreeExt, UnknownNode},
    context::Context,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("no action is executing")]
pub struct NotExecuting;

/// The current-action resource, shared by every runner so they agree on which
/// action is executing.
#[derive(Default)]
pub struct CurrentAction(Option<ActionNodeKey>);

pub trait CurrentActionExt {
    /// The executing action, or [`NotExecuting`] outside execution.
    fn current_action(&self) -> Result<ActionNodeKey, NotExecuting>;

    /// The root of the executing action's tree, which is the executing action
    /// itself when it's a root.
    fn current_root(&self) -> Result<ActionNodeKey, NotExecuting>;

    /// Runs `f` with `key` as the current action, restoring the previous one
    /// afterwards, even if `f` panics.
    ///
    /// Refusing unknown keys keeps the current action in the tree, since
    /// executing nodes can't be removed.
    fn with_current_action<R>(
        &mut self,
        key: ActionNodeKey,
        f: impl FnOnce(&mut Context) -> R,
    ) -> Result<R, UnknownNode>;
}

impl CurrentActionExt for Context {
    fn current_action(&self) -> Result<ActionNodeKey, NotExecuting> {
        self.get::<CurrentAction>()
            .and_then(|current| current.0)
            .ok_or(NotExecuting)
    }

    fn current_root(&self) -> Result<ActionNodeKey, NotExecuting> {
        let current = self.current_action()?;
        Ok(self
            .ancestors(current)
            .expect("the current action is in the tree")
            .last()
            .unwrap_or(current))
    }

    fn with_current_action<R>(
        &mut self,
        key: ActionNodeKey,
        f: impl FnOnce(&mut Context) -> R,
    ) -> Result<R, UnknownNode> {
        self.get_or_insert_with(ActionTree::default).node(key)?;
        let previous = self
            .get_or_insert_with(CurrentAction::default)
            .0
            .replace(key);
        let result = catch_unwind(AssertUnwindSafe(|| f(self)));
        self.get_or_insert_with(CurrentAction::default).0 = previous;
        match result {
            Ok(result) => Ok(result),
            Err(panic) => resume_unwind(panic),
        }
    }
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;
    use crate::action::ActionExt;

    #[gtest]
    fn current_root_walks_up_to_the_root() {
        let mut ctx = Context::new();
        expect_that!(ctx.current_root(), err(eq(NotExecuting)));

        let (root, (root_seen, grandchild_seen)) = ctx.run_node(|ctx: &mut Context| {
            let root_seen = ctx.current_root();
            let (_, (_, grandchild_seen)) = ctx
                .run_node(|ctx: &mut Context| ctx.run_node(|ctx: &mut Context| ctx.current_root()));
            (root_seen, grandchild_seen)
        });

        expect_that!(root_seen, ok(eq(root)));
        expect_that!(grandchild_seen, ok(eq(root)));
    }

    #[gtest]
    fn unknown_keys_are_refused_without_running() {
        let mut ctx = Context::new();
        let removed = ctx.create_root();
        ctx.remove_node(removed).unwrap();

        let mut ran = false;
        let result = ctx.with_current_action(removed, |_| ran = true);

        expect_that!(result, err(eq(UnknownNode(removed))));
        expect_false!(ran);
        expect_that!(ctx.current_action(), err(eq(NotExecuting)));
    }
}
