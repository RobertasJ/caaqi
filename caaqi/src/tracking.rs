use crate::{
    action::{ActiveActionState, SelfAdjust},
    id::Id,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TrackingId(Id);

impl TrackingId {
    pub fn new() -> Self {
        Self(Id::new())
    }

    pub fn notify(&self, s: &mut ActiveActionState) -> Result<(), SelfAdjust> {
        fn rec(s: &mut ActiveActionState, tracking_id: &TrackingId) -> Result<(), SelfAdjust> {
            for sub_action in s.sub_actions().iter().rev() {
                if sub_action.contains_tracking_id(&tracking_id) {
                    return Err(SelfAdjust::new(*tracking_id));
                }
            }

            Ok(())
        }

        rec(s, self)
    }
}
