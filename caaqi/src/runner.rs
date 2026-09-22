use crate::action::{Action, ActionContext, SelfAdjust};

pub fn run_action<A: Action<Output = T>, T>(mut action: A) -> Result<T, SelfAdjust> {
    let mut cx = ActionContext::new();
    action.run(&mut cx)
}
