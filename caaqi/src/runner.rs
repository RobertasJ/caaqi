use crate::action::{Action, ActiveActionState, SelfAdjust};

pub fn run_action<A: Action<Output = ()> + 'static>(
    s: &mut ActiveActionState,
    mut action: A,
) -> Result<(), SelfAdjust> {
    let mut cx = ActiveActionState::new_with_parent(s);
    let res = action.run(&mut cx);
    let cx = cx.into_action_state(action);
    s.add_sub_action(cx);
    res
}
