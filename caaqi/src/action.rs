use crate::{
    action_tree::{ActionNodeKey, ActionTreeExt, NotExecuting},
    context::Context,
    current::CurrentActionExt,
};

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

pub trait ActionExt {
    /// Creates a node under the executing action (or a root outside
    /// execution) and runs `action` with that node as the current action.
    ///
    /// The action body is dropped afterwards: actions aren't stored yet, so the
    /// node only records where the action ran and can't be rerun.
    fn run_node<A: Action>(&mut self, action: A) -> (ActionNodeKey, A::Output);
}

impl ActionExt for Context {
    fn run_node<A: Action>(&mut self, mut action: A) -> (ActionNodeKey, A::Output) {
        let key = self
            .create_child()
            .unwrap_or_else(|NotExecuting| self.create_root());
        let output = self
            .with_current_action(key, |ctx| action.run(ctx))
            .expect("the node was just created");
        (key, output)
    }
}
