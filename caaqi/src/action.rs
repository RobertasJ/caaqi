use std::{
    any::{Any, TypeId},
    collections::{HashMap, HashSet},
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
};

use slotmap::{SecondaryMap, SlotMap, new_key_type};
use smallvec::SmallVec;

use crate::tracking::TrackingId;

struct AnyAction<O> {
    action: Box<dyn Action<Output = O> + 'static>,
}

impl<O> AnyAction<O> {
    pub fn new(action: impl Action<Output = O> + 'static) -> Self {
        Self {
            action: Box::new(action),
        }
    }

    fn run(&mut self, ctx: &mut ActionContext) -> Result<O, SelfAdjust> {
        self.action.run(ctx)
    }
}

pub trait Action {
    type Output;

    fn run(&mut self, ctx: &mut ActionContext) -> Result<Self::Output, SelfAdjust>;
}

impl<F: FnMut(&mut ActionContext) -> Result<T, SelfAdjust>, T> Action for F {
    type Output = T;

    fn run(&mut self, ctx: &mut ActionContext) -> Result<Self::Output, SelfAdjust> {
        self(ctx)
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
    data: HashMap<TypeId, Box<dyn Any>>,
}

struct ActionRelationship {
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
            data: HashMap::new(),
        }
    }

    // User data is a type-map: one value per type, global to the context. It
    // survives reruns and descendant removal, and is dropped only by `remove`.
    // Returned references borrow the context, so they can't be held across
    // `run`; drop the borrow first, or `remove` + `insert` around a nested run.

    /// Stores `value`, returning the previous value of the same type.
    pub fn insert<T: 'static>(&mut self, value: T) -> Option<T> {
        self.data
            .insert(TypeId::of::<T>(), Box::new(value))
            .map(downcast)
    }

    pub fn get<T: 'static>(&self) -> Option<&T> {
        self.data
            .get(&TypeId::of::<T>())
            .map(|value| value.downcast_ref().expect("data is keyed by its type"))
    }

    pub fn get_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.data
            .get_mut(&TypeId::of::<T>())
            .map(|value| value.downcast_mut().expect("data is keyed by its type"))
    }

    pub fn get_or_insert_with<T: 'static>(&mut self, f: impl FnOnce() -> T) -> &mut T {
        self.data
            .entry(TypeId::of::<T>())
            .or_insert_with(|| Box::new(f()))
            .downcast_mut()
            .expect("data is keyed by its type")
    }

    pub fn remove<T: 'static>(&mut self) -> Option<T> {
        self.data.remove(&TypeId::of::<T>()).map(downcast)
    }

    /// Creates a root, even when another action is executing.
    pub fn create_root(&mut self, action: impl Action<Output = ()> + 'static) -> ActionNodeKey {
        self.insert_action(action, None)
    }

    /// Creates a child of the executing action, or returns `None` outside execution.
    pub fn create_child(
        &mut self,
        action: impl Action<Output = ()> + 'static,
    ) -> Option<ActionNodeKey> {
        let parent = self.current_action?;
        Some(self.insert_action(action, Some(parent)))
    }

    fn insert_action(
        &mut self,
        action: impl Action<Output = ()> + 'static,
        parent: Option<ActionNodeKey>,
    ) -> ActionNodeKey {
        // Only fresh nodes can be attached, so parenting cannot create cycles.
        let key = self.relationships.insert(ActionRelationship {
            sub_actions: SmallVec::new(),
            parent,
        });
        self.action_nodes.insert(key, AnyAction::new(action));
        self.tracked.insert(key, HashSet::new());
        if let Some(parent) = parent {
            self.relationships[parent].sub_actions.push(key);
        }
        key
    }

    fn contains_tracked(&self, key: ActionNodeKey, tracking_id: TrackingId) -> bool {
        self.tracked
            .get(key)
            .is_some_and(|tracked| tracked.contains(&tracking_id))
    }

    /// Returns a [`SelfAdjust`] if any ancestor of the executing action tracks
    /// `tracking_id`, so callers can bail out with `?`; otherwise `Ok(())`.
    pub fn adjust_if_ancestor_tracks(&self, tracking_id: TrackingId) -> Result<(), SelfAdjust> {
        if self.any_ancestor_contains_tracked(tracking_id) {
            Err(SelfAdjust::new(tracking_id))
        } else {
            Ok(())
        }
    }

    fn any_ancestor_contains_tracked(&self, tracking_id: TrackingId) -> bool {
        let mut ancestor = self
            .current_action
            .and_then(|key| self.relationships.get(key))
            .and_then(|relationship| relationship.parent);

        while let Some(key) = ancestor {
            if self.contains_tracked(key, tracking_id) {
                return true;
            }

            ancestor = self
                .relationships
                .get(key)
                .and_then(|relationship| relationship.parent);
        }

        false
    }

    /// Creates and immediately runs an action as a child of the executing
    /// action, or as a root outside execution.
    pub fn run<A: Action<Output = ()> + 'static>(
        &mut self,
        action: A,
    ) -> Result<(), SelfAdjust> {
        let key = self.insert_action(action, self.current_action);
        self.run_action(key)
            .expect("new action is available to run")
    }

    /// Runs an existing action. Returns `None` for an unknown key or an action
    /// that is already executing (its body is temporarily borrowed by that run),
    /// or one with an executing descendant.
    ///
    /// All descendants from a previous run are removed first, so the rerun
    /// rebuilds its children from scratch.
    pub fn run_action(&mut self, key: ActionNodeKey) -> Option<Result<(), SelfAdjust>> {
        if self.has_executing_descendant(key) {
            return None;
        }
        let mut action = self.action_nodes.remove(key)?;
        self.remove_descendants(key);
        let previous_action = self.current_action.replace(key);
        let result = catch_unwind(AssertUnwindSafe(|| action.run(self)));
        self.current_action = previous_action;
        self.action_nodes.insert(key, action);
        match result {
            Ok(result) => Some(result),
            Err(panic) => resume_unwind(panic),
        }
    }

    fn has_executing_descendant(&self, key: ActionNodeKey) -> bool {
        let Some(relationship) = self.relationships.get(key) else {
            return false;
        };
        let mut pending: Vec<_> = relationship.sub_actions.to_vec();
        while let Some(key) = pending.pop() {
            // An executing action's body is borrowed out of `action_nodes`.
            if !self.action_nodes.contains_key(key) {
                return true;
            }
            if let Some(relationship) = self.relationships.get(key) {
                pending.extend_from_slice(&relationship.sub_actions);
            }
        }
        false
    }

    fn remove_descendants(&mut self, key: ActionNodeKey) {
        let Some(relationship) = self.relationships.get_mut(key) else {
            return;
        };
        let mut pending: Vec<_> = std::mem::take(&mut relationship.sub_actions).into_vec();
        while let Some(key) = pending.pop() {
            if let Some(relationship) = self.relationships.remove(key) {
                pending.extend(relationship.sub_actions);
            }
            self.action_nodes.remove(key);
            self.tracked.remove(key);
        }
    }
}

fn downcast<T: 'static>(value: Box<dyn Any>) -> T {
    *value.downcast().expect("data is keyed by its type")
}

pub fn finish_with<T>(val: T) -> Result<T, SelfAdjust> {
    Ok(val)
}

pub fn finish() -> Result<(), SelfAdjust> {
    finish_with(())
}
