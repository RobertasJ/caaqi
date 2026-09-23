use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

use crate::{action_tree::ActionNodeKey, context::Context};

/// The current-action resource, shared by every runner so they agree on which
/// action is executing.
#[derive(Default)]
pub struct CurrentAction(Option<ActionNodeKey>);

pub trait CurrentActionExt {
    /// The executing action, or `None` outside execution.
    fn current_action(&self) -> Option<ActionNodeKey>;

    /// Runs `f` with `key` as the current action, restoring the previous one
    /// afterwards, even if `f` panics.
    fn with_current_action<R>(
        &mut self,
        key: ActionNodeKey,
        f: impl FnOnce(&mut Context) -> R,
    ) -> R;
}

impl CurrentActionExt for Context {
    fn current_action(&self) -> Option<ActionNodeKey> {
        self.get::<CurrentAction>().and_then(|current| current.0)
    }

    fn with_current_action<R>(
        &mut self,
        key: ActionNodeKey,
        f: impl FnOnce(&mut Context) -> R,
    ) -> R {
        let previous = self
            .get_or_insert_with(CurrentAction::default)
            .0
            .replace(key);
        let result = catch_unwind(AssertUnwindSafe(|| f(self)));
        self.get_or_insert_with(CurrentAction::default).0 = previous;
        match result {
            Ok(result) => result,
            Err(panic) => resume_unwind(panic),
        }
    }
}
