use std::{
    collections::{HashSet, hash_set},
    f32::consts::E,
};

use crate::{
    action::{Action, ActionExt, RunActionError},
    action_tree::{ActionNodeKey, ActionTreeExt, ClearChildrenError, UnknownNode},
    context::Context,
    current::{self, CurrentActionExt, NotExecuting},
    grouping::{GroupId, GroupingExt},
    tracking,
};

/// Identifies a set of tracked actions that can be rerun together. Backed by
/// a group, which stays private so tracking controls its membership.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TrackingId(GroupId);

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("tracking id {0:?} doesn't exist")]
pub struct UnknownTrackingId(pub TrackingId);

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TrackError {
    #[error(transparent)]
    NotExecuting(#[from] NotExecuting),
    #[error(transparent)]
    UnknownTrackingId(#[from] UnknownTrackingId),
    #[error(transparent)]
    UnknownNode(#[from] UnknownNode),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RunTrackedError {
    #[error(transparent)]
    NotExecuting(#[from] NotExecuting),
    #[error(transparent)]
    RunActionError(#[from] RunActionError),
    #[error(transparent)]
    UnknownTrackingId(#[from] UnknownTrackingId),
    #[error(transparent)]
    ClearChildrenError(#[from] ClearChildrenError),
}

/// Self adjustment: a [`rerun`](TrackingExt::rerun) of `id` reached an
/// executing action, so every action up to `handler` has to return early.
/// Once they have unwound, `handler`'s [`run_tracked`](TrackingExt::run_tracked)
/// reruns everything tracking `id`.
///
/// The fields are private so only `rerun` can create one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("tracking id {id:?} needs a rerun, handled by action node {handler:?}")]
pub struct RerunNeeded {
    id: TrackingId,
    /// The topmost executing action tracking `id`.
    handler: ActionNodeKey,
}

impl RerunNeeded {
    /// The tracking id whose rerun caused the early return.
    pub fn id(&self) -> TrackingId {
        self.id
    }
}

/// The tracking ids waiting to be notified.
#[derive(Debug, Default)]
pub struct ToNotify {
    ids: HashSet<TrackingId>,
}

impl ToNotify {
    fn insert(&mut self, id: TrackingId) {
        self.ids.insert(id);
    }

    fn remove(&mut self, id: TrackingId) -> bool {
        self.ids.remove(&id)
    }
}

impl Context {
    /// Queues `id` to be notified.
    fn queue_notify(&mut self, id: TrackingId) -> Result<(), UnknownTrackingId> {
        if !self.group_exists(id.0) {
            return Err(UnknownTrackingId(id));
        }
        self.get_or_insert_with(ToNotify::default).insert(id);
        Ok(())
    }

    /// The queued tracking ids, in no particular order. The iterator owns a
    /// copy of the queue, so it doesn't borrow the context.
    fn iter_to_notify(&self) -> hash_set::IntoIter<TrackingId> {
        self.get::<ToNotify>()
            .map(|to_notify| to_notify.ids.clone())
            .unwrap_or_default()
            .into_iter()
    }

    /// Removes `id` from the queue. Returns `Ok(false)` if it wasn't queued.
    fn unqueue_notify(&mut self, id: TrackingId) -> Result<bool, UnknownTrackingId> {
        if !self.group_exists(id.0) {
            return Err(UnknownTrackingId(id));
        }
        Ok(self
            .get_mut::<ToNotify>()
            .is_some_and(|to_notify| to_notify.remove(id)))
    }
}

pub trait TrackingExt {
    /// Creates a tracking id with nothing tracking it yet.
    fn create_tracking_id(&mut self) -> TrackingId;

    /// Runs `action` so that the [`track`](Self::track) calls it makes can
    /// [`rerun`](Self::rerun) it.
    fn run_tracked<A: Action<Output = ()> + 'static>(&mut self, action: A);

    fn run_tracked_checked<A: Action<Output = ()> + 'static>(
        &mut self,
        action: A,
    ) -> Result<(), RunTrackedError>;

    /// Marks the executing action as depending on `id`, so a
    /// [`rerun`](Self::rerun) of `id` reruns it.
    ///
    /// # Panics
    ///
    /// If no action is executing or `id` doesn't exist. Use
    /// [`track_checked`](Self::track_checked) to get the error instead.
    fn track(&mut self, id: TrackingId);

    /// [`track`](Self::track), returning the error instead of panicking.
    fn track_checked(&mut self, id: TrackingId) -> Result<(), TrackError>;

    /// Reruns every action tracking `id`.
    ///
    /// If one of them is executing, returns [`RerunNeeded`] instead. The caller
    /// must return it, so the rerun happens once the actions have unwound.
    ///
    /// # Panics
    ///
    /// If `id` doesn't exist. Use [`rerun_checked`](Self::rerun_checked) to get
    /// the error instead.
    fn notify(&mut self, id: TrackingId);

    /// [`rerun`](Self::rerun), returning an unknown `id` as the outer error
    /// instead of panicking. The inner `Result` is the early return.
    fn notify_checked(&mut self, id: TrackingId) -> Result<(), UnknownTrackingId>;
}

impl TrackingExt for Context {
    fn create_tracking_id(&mut self) -> TrackingId {
        let group = self.create_group();
        TrackingId(group)
    }

    fn run_tracked<A: Action<Output = ()> + 'static>(&mut self, action: A) {
        self.run_tracked_checked(action)
            .unwrap_or_else(|e| panic!("{e}"))
    }

    fn run_tracked_checked<A: Action<Output = ()> + 'static>(
        &mut self,
        action: A,
    ) -> Result<(), RunTrackedError> {
        let action_node = self.create_child_action(action)?;
        self.run_action::<()>(action_node)?;

        let action_branch = self
            .subtree_top_down(action_node)
            .expect("action should exist")
            .collect::<HashSet<_>>();

        let mut continue_notifying = true;

        while continue_notifying {
            continue_notifying = false;

            for id in self.iter_to_notify() {
                let tracked_actions = self
                    .group_members(id.0)
                    .map_err(|_| UnknownTrackingId(id))?
                    .collect::<HashSet<_>>();

                let are_all_inside_action_branch = tracked_actions
                    .iter()
                    .all(|action| action_branch.contains(&action));

                if are_all_inside_action_branch {
                    continue_notifying = true;

                    for action in tracked_actions {
                        self.clear_children(action)?;
                        self.run_action::<()>(action)?;
                    }
                }
            }
        }

        Ok(())
    }

    fn track(&mut self, id: TrackingId) {
        if let Err(error) = self.track_checked(id) {
            panic!("{error}");
        }
    }

    fn track_checked(&mut self, id: TrackingId) -> Result<(), TrackError> {
        self.add_to_group(id.0, self.current_action()?)
            .map_err(|_| UnknownTrackingId(id))?;
        Ok(())
    }

    fn notify(&mut self, id: TrackingId) {
        if let Err(error) = self.notify_checked(id) {
            panic!("{error}")
        }
    }

    fn notify_checked(&mut self, id: TrackingId) -> Result<(), UnknownTrackingId> {
        self.queue_notify(id)
    }
}
