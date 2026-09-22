use std::{any::Any, collections::HashSet};

use crate::tracking::TrackingId;

pub struct AnyAction<O> {
    action: Box<dyn Action<Output = O> + 'static>,
}

impl<O> AnyAction<O> {
    pub fn new(action: impl Action<Output = O> + 'static) -> Self {
        Self {
            action: Box::new(action),
        }
    }
}

pub trait Action {
    type Output;

    fn run<'a, 'parent>(
        &mut self,
        s: &'a mut ActiveActionState<'parent>,
    ) -> Result<Self::Output, SelfAdjust>;
}

impl<F: FnMut(&mut ActiveActionState) -> Result<T, SelfAdjust>, T> Action for F {
    type Output = T;

    fn run<'a, 'parent>(
        &mut self,
        s: &'a mut ActiveActionState<'parent>,
    ) -> Result<Self::Output, SelfAdjust> {
        self(s)
    }
}

#[derive(Debug)]
pub struct SelfAdjust(TrackingId);

impl SelfAdjust {
    pub fn new(tracking_id: TrackingId) -> Self {
        Self(tracking_id)
    }

    pub fn tracking_id(&self) -> TrackingId {
        self.0
    }
}

pub struct ActionState {
    subscribers: HashSet<TrackingId>,
    sub_actions: Vec<ActionState>,
    action: AnyAction<()>,
}

pub struct ActiveActionState<'parent> {
    subscribers: HashSet<TrackingId>,
    sub_actions: Vec<ActionState>,
    parent_action: Option<&'parent ActiveActionState<'parent>>,
}

impl ActionState {
    pub fn into_active(self) -> ActiveActionState<'static> {
        ActiveActionState {
            subscribers: self.subscribers,
            sub_actions: self.sub_actions,
            parent_action: None,
        }
    }

    pub fn track(&mut self, tracking_id: TrackingId) {
        self.subscribers.insert(tracking_id);
    }

    pub fn contains_tracking_id(&self, tracking_id: &TrackingId) -> bool {
        self.subscribers.contains(tracking_id)
    }

    pub fn add_sub_action(&mut self, sub_action: ActionState) {
        self.sub_actions.push(sub_action);
    }

    pub fn sub_actions(&self) -> &[ActionState] {
        &self.sub_actions
    }
}

impl ActiveActionState<'static> {
    pub fn new() -> Self {
        Self {
            subscribers: HashSet::new(),
            sub_actions: Vec::new(),
            parent_action: None,
        }
    }
}

impl<'parent> ActiveActionState<'parent> {
    pub fn new_with_parent<'a>(parent: &'parent mut ActiveActionState<'a>) -> Self {
        Self {
            subscribers: HashSet::new(),
            sub_actions: Vec::new(),
            parent_action: Some(parent),
        }
    }

    pub fn into_action_state(self, action: impl Action<Output = ()> + 'static) -> ActionState {
        ActionState {
            subscribers: self.subscribers,
            sub_actions: self.sub_actions,
            action: AnyAction::new(action),
        }
    }

    pub fn track(&mut self, tracking_id: TrackingId) {
        self.subscribers.insert(tracking_id);
    }

    pub fn contains_tracking_id(&self, tracking_id: &TrackingId) -> bool {
        self.subscribers.contains(tracking_id)
    }

    pub fn add_sub_action(&mut self, sub_action: ActionState) {
        self.sub_actions.push(sub_action);
    }

    pub fn sub_actions(&self) -> &[ActionState] {
        &self.sub_actions
    }

    pub fn parent(&self) -> Option<&ActiveActionState<'_>> {
        self.parent_action
    }
}

pub fn finish_with<T>(val: T) -> Result<T, SelfAdjust> {
    Ok(val)
}

pub fn finish() -> Result<(), SelfAdjust> {
    finish_with(())
}
