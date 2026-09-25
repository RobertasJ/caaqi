use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

use crate::{
    action_tree::{ActionNodeKey, ActionTree, UnknownNode},
    context::Context,
};

/// The current-action resource, shared by every runner so they agree on which
/// action is executing.
#[derive(Default)]
pub struct CurrentAction(Option<ActionNodeKey>);

pub trait CurrentActionExt {
    /// The executing action, or `None` outside execution.
    fn current_action(&self) -> Option<ActionNodeKey>;

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
    fn current_action(&self) -> Option<ActionNodeKey> {
        self.get::<CurrentAction>().and_then(|current| current.0)
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
    use crate::action_tree::ActionTreeExt;

    #[gtest]
    fn unknown_keys_are_refused_without_running() {
        let mut ctx = Context::new();
        let removed = ctx.create_root();
        ctx.remove_node(removed).unwrap();

        let mut ran = false;
        let result = ctx.with_current_action(removed, |_| ran = true);

        expect_that!(result, err(eq(UnknownNode(removed))));
        expect_false!(ran);
        expect_that!(ctx.current_action(), none());
    }
}
