use crate::id::Id;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TrackingId(Id);

impl TrackingId {
    pub fn new() -> Self {
        Self(Id::new())
    }
}
