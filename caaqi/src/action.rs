use std::collections::HashSet;

use slotmap::{SecondaryMap, SlotMap, new_key_type};
use smallvec::SmallVec;

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

    fn run(&mut self, s: &mut ActionContext) -> Result<O, SelfAdjust> {
        self.action.run(s)
    }
}

pub trait Action {
    type Output;

    fn run(&mut self, s: &mut ActionContext) -> Result<Self::Output, SelfAdjust>;
}

impl<F: FnMut(&mut ActionContext) -> Result<T, SelfAdjust>, T> Action for F {
    type Output = T;

    fn run(&mut self, s: &mut ActionContext) -> Result<Self::Output, SelfAdjust> {
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

new_key_type! {
    pub struct ActionNodeKey;
}

pub struct ActionContext {
    relationships: SlotMap<ActionNodeKey, ActionRelationship>,
    action_nodes: SecondaryMap<ActionNodeKey, AnyAction<()>>,
    tracked: SecondaryMap<ActionNodeKey, HashSet<TrackingId>>,
    current_action: Option<ActionNodeKey>,
}

pub struct ActionRelationship {
    sub_actions: SmallVec<[ActionNodeKey; 1]>,
    parent: Option<ActionNodeKey>,
}

impl ActionContext {
    pub fn new() -> Self {
        Self {
            relationships: SlotMap::with_key(),
            action_nodes: SecondaryMap::new(),
            tracked: SecondaryMap::new(),
            current_action: None,
        }
    }

    pub fn current_action(&self) -> Option<ActionNodeKey> {
        self.current_action
    }

    pub(crate) fn set_current_action(&mut self, key: Option<ActionNodeKey>) {
        self.current_action = key;
    }

    pub fn add_action(&mut self, action: impl Action<Output = ()> + 'static) -> ActionNodeKey {
        let key = self.relationships.insert(ActionRelationship {
            sub_actions: SmallVec::new(),
            parent: None,
        });
        self.action_nodes.insert(key, AnyAction::new(action));
        self.tracked.insert(key, HashSet::new());
        key
    }

    pub fn remove_action(&mut self, key: ActionNodeKey) -> Option<AnyAction<()>> {
        let relationship = self.relationships.remove(key)?;

        if self.current_action == Some(key) {
            self.current_action = None;
        }

        if let Some(parent) = relationship.parent {
            if let Some(parent_relationship) = self.relationships.get_mut(parent) {
                parent_relationship
                    .sub_actions
                    .retain(|child| *child != key);
            }
        }

        for child in relationship.sub_actions {
            if let Some(child_relationship) = self.relationships.get_mut(child) {
                child_relationship.parent = None;
            }
        }

        self.tracked.remove(key);
        self.action_nodes.remove(key)
    }

    pub fn add_sub_action(&mut self, parent: ActionNodeKey, child: ActionNodeKey) -> bool {
        if parent == child
            || !self.relationships.contains_key(parent)
            || !self.relationships.contains_key(child)
        {
            return false;
        }

        if let Some(old_parent) = self.relationships[child].parent {
            if let Some(old_parent_relationship) = self.relationships.get_mut(old_parent) {
                old_parent_relationship
                    .sub_actions
                    .retain(|key| *key != child);
            }
        }

        let parent_relationship = self.relationships.get_mut(parent).unwrap();
        if !parent_relationship.sub_actions.contains(&child) {
            parent_relationship.sub_actions.push(child);
        }
        self.relationships[child].parent = Some(parent);
        true
    }

    pub fn relationship(&self, key: ActionNodeKey) -> Option<&ActionRelationship> {
        self.relationships.get(key)
    }

    pub fn parent(&self, key: ActionNodeKey) -> Option<Option<ActionNodeKey>> {
        self.relationships
            .get(key)
            .map(|relationship| relationship.parent)
    }

    pub fn sub_actions(&self, key: ActionNodeKey) -> Option<&[ActionNodeKey]> {
        self.relationships
            .get(key)
            .map(|relationship| relationship.sub_actions.as_slice())
    }

    pub fn track(&mut self, key: ActionNodeKey, tracking_id: TrackingId) -> bool {
        match self.tracked.get_mut(key) {
            Some(tracked) => tracked.insert(tracking_id),
            None => false,
        }
    }

    pub fn untrack(&mut self, key: ActionNodeKey, tracking_id: TrackingId) -> bool {
        match self.tracked.get_mut(key) {
            Some(tracked) => tracked.remove(&tracking_id),
            None => false,
        }
    }

    pub fn tracked(&self, key: ActionNodeKey) -> Option<&HashSet<TrackingId>> {
        self.tracked.get(key)
    }

    pub fn contains_tracked(&self, key: ActionNodeKey, tracking_id: TrackingId) -> bool {
        self.tracked
            .get(key)
            .is_some_and(|tracked| tracked.contains(&tracking_id))
    }

    pub fn run<A: Action<Output = ()> + 'static>(&mut self, action: A) -> Result<(), SelfAdjust> {
        let action_key = self.add_action(action);
        let previous_action = self.current_action();

        if let Some(parent_key) = previous_action {
            self.add_sub_action(parent_key, action_key);
        }

        self.set_current_action(Some(action_key));
        let result = self
            .run_action(action_key)
            .expect("action was just added to the context");
        self.set_current_action(previous_action);
        result
    }

    pub fn run_action(&mut self, key: ActionNodeKey) -> Option<Result<(), SelfAdjust>> {
        let mut action = self.action_nodes.remove(key)?;
        let result = action.run(self);
        self.action_nodes.insert(key, action);
        Some(result)
    }
}

pub fn finish_with<T>(val: T) -> Result<T, SelfAdjust> {
    Ok(val)
}

pub fn finish() -> Result<(), SelfAdjust> {
    finish_with(())
}
