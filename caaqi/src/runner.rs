use smallvec::SmallVec;

use crate::{
    action::{Action, SelfAdjust},
    context::SUBSCRIBERS,
};

pub fn run_action<A: Action<Output = T>, T>(mut action: A) -> Result<T, SelfAdjust> {
    let mut subscribers = SmallVec::new();
    let res = SUBSCRIBERS.set(&mut subscribers, || action.run());

    res
}
