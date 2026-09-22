use crate::action::{Action, ActionContext, SelfAdjust};

pub fn run_action<A: Action<Output = ()> + 'static>(
    ctx: &mut ActionContext,
    action: A,
) -> Result<(), SelfAdjust> {
    ctx.run(action)
}
