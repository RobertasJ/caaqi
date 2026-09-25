use std::collections::HashSet;

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

pub trait TrackingExt {
    /// Creates a tracking id with nothing tracking it yet.
    fn create_tracking_id(&mut self) -> TrackingId;

    /// Runs `action` so that the [`track`](Self::track) calls it makes can
    /// [`rerun`](Self::rerun) it.
    fn run_tracked<A: Action<Output = Result<(), RerunNeeded>> + 'static>(
        &mut self,
        action: A,
    ) -> Result<(), RerunNeeded>;

    fn run_tracked_checked<A: Action<Output = Result<(), RerunNeeded>> + 'static>(
        &mut self,
        action: A,
    ) -> Result<Result<(), RerunNeeded>, RunTrackedError>;

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
    fn notify(&mut self, id: TrackingId) -> Result<(), RerunNeeded>;

    /// [`rerun`](Self::rerun), returning an unknown `id` as the outer error
    /// instead of panicking. The inner `Result` is the early return.
    fn notify_checked(&mut self, id: TrackingId) -> Result<Result<(), RerunNeeded>, TrackError>;
}

impl TrackingExt for Context {
    fn create_tracking_id(&mut self) -> TrackingId {
        let group = self.create_group();
        TrackingId(group)
    }

    fn run_tracked<A: Action<Output = Result<(), RerunNeeded>> + 'static>(
        &mut self,
        action: A,
    ) -> Result<(), RerunNeeded> {
        self.run_tracked_checked(action)
            .unwrap_or_else(|e| panic!("{e}"))
    }

    fn run_tracked_checked<A: Action<Output = Result<(), RerunNeeded>> + 'static>(
        &mut self,
        action: A,
    ) -> Result<Result<(), RerunNeeded>, RunTrackedError> {
        let action_node = self.create_child_action(action)?;
        let output = self.run_action::<Result<(), RerunNeeded>>(action_node)?;
        match output {
            Ok(()) => Ok(Ok(())),
            Err(rerun_needed) => {
                if self.current_action()? == rerun_needed.handler {
                    self.run_tracked(rerun_needed.id)?;

                    Ok(Ok(()))
                } else {
                    Ok(Err(rerun_needed))
                }
            }
        }
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

    fn notify(&mut self, id: TrackingId) -> Result<(), RerunNeeded> {
        match self.notify_checked(id) {
            Ok(result) => result,
            Err(error) => panic!("{error}"),
        }
    }

    fn notify_checked(&mut self, id: TrackingId) -> Result<Result<(), RerunNeeded>, TrackError> {
        let action = self.current_action()?;
        let ancestors = [action].into_iter().chain(self.ancestors(action)?);

        let mut rerun_needed = None;

        for ancestor in ancestors {
            if self
                .group_contains(id.0, ancestor)
                .map_err(|_| UnknownTrackingId(id))?
            {
                rerun_needed = Some(ancestor);
            }
        }

        if let Some(handler) = rerun_needed {
            Ok(Err(RerunNeeded { id, handler }))
        } else {
            Ok(Ok(()))
        }
    }
}

impl Context {
    fn run_tracked(&mut self, tracking_id: TrackingId) -> Result<(), RunTrackedError> {
        let tracked = self
            .group_members(tracking_id.0)
            .map_err(|_| UnknownTrackingId(tracking_id))?
            .collect::<HashSet<_>>();

        let root = self.current_root()?;
        let mut tree = self
            .subtree_top_down(root)
            .expect("the tree root doesnt exist")
            .into_cursor();

        Ok(while let Some(node) = tree.next(self) {
            if tracked.contains(&node) {
                self.clear_children(node)?;
                let res = self.run_action::<Result<(), RerunNeeded>>(node)?;

                match res {
                    Ok(()) => {}
                    Err(rerun_needed) => {
                        if self.current_action()? == rerun_needed.handler {
                            self.run_tracked(rerun_needed.id)?;
                        } else {
                            return Err(RunTrackedError::NotExecuting(current::NotExecuting));
                        }
                    }
                }

                tree.skip_children();
            }
        })
    }
}
