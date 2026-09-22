use crate::{
    action::{ActionContext, SelfAdjust},
    id::Id,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TrackingId(Id);

impl TrackingId {
    pub fn new() -> Self {
        Self(Id::new())
    }

    pub fn notify(&self, s: &mut ActionContext) -> Result<(), SelfAdjust> {
        todo!()
    }
}
