use std::collections::HashSet;

use slotmap::SecondaryMap;

use crate::{
    action::{ActionNodeKey, ActionTree, SelfAdjust},
    context::Context,
    id::Id,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TrackingId(Id);

impl TrackingId {
    pub fn new() -> Self {
        Self(Id::new())
    }
}

/// The tracking resource: which tracking ids each action node tracks.
///
/// Entries for removed nodes are left in place; their versioned keys never
/// match new nodes and are overwritten as slots are reused.
#[derive(Default)]
pub struct Tracked(SecondaryMap<ActionNodeKey, HashSet<TrackingId>>);

impl Tracked {
    pub fn contains(&self, key: ActionNodeKey, tracking_id: TrackingId) -> bool {
        self.0
            .get(key)
            .is_some_and(|tracked| tracked.contains(&tracking_id))
    }
}

pub trait TrackingExt {
    /// Returns a [`SelfAdjust`] if any ancestor of the executing action tracks
    /// `tracking_id`, so callers can bail out with `?`; otherwise `Ok(())`.
    fn adjust_if_ancestor_tracks(&self, tracking_id: TrackingId) -> Result<(), SelfAdjust>;
}

impl TrackingExt for Context {
    fn adjust_if_ancestor_tracks(&self, tracking_id: TrackingId) -> Result<(), SelfAdjust> {
        let (Some(tree), Some(tracked)) = (self.get::<ActionTree>(), self.get::<Tracked>()) else {
            return Ok(());
        };
        let Some(current) = tree.current_action() else {
            return Ok(());
        };
        if tree
            .ancestors(current)
            .any(|key| tracked.contains(key, tracking_id))
        {
            Err(SelfAdjust::new(tracking_id))
        } else {
            Ok(())
        }
    }
}
