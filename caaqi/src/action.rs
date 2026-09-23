use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

use slotmap::{SecondaryMap, SlotMap, new_key_type};
use smallvec::SmallVec;

use crate::{context::Context, tracking::TrackingId};

struct AnyAction<O> {
    action: Box<dyn Action<Output = O> + 'static>,
}

impl<O> AnyAction<O> {
    pub fn new(action: impl Action<Output = O> + 'static) -> Self {
        Self {
            action: Box::new(action),
        }
    }

    fn run(&mut self, ctx: &mut Context) -> Result<O, SelfAdjust> {
        self.action.run(ctx)
    }
}

pub trait Action {
    type Output;

    fn run(&mut self, ctx: &mut Context) -> Result<Self::Output, SelfAdjust>;
}

impl<F: FnMut(&mut Context) -> Result<T, SelfAdjust>, T> Action for F {
    type Output = T;

    fn run(&mut self, ctx: &mut Context) -> Result<Self::Output, SelfAdjust> {
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

/// The action tree resource. Its read methods are public so other modules can
/// build on it; mutation goes through [`ActionTreeExt`].
#[derive(Default)]
pub struct ActionTree {
    relationships: SlotMap<ActionNodeKey, ActionRelationship>,
    action_nodes: SecondaryMap<ActionNodeKey, AnyAction<()>>,
    current_action: Option<ActionNodeKey>,
}

struct ActionRelationship {
    sub_actions: SmallVec<[ActionNodeKey; 1]>,
    parent: Option<ActionNodeKey>,
}

impl ActionTree {
    pub fn current_action(&self) -> Option<ActionNodeKey> {
        self.current_action
    }

    pub fn parent(&self, key: ActionNodeKey) -> Option<ActionNodeKey> {
        self.relationships
            .get(key)
            .and_then(|relationship| relationship.parent)
    }

    pub fn children(&self, key: ActionNodeKey) -> &[ActionNodeKey] {
        self.relationships
            .get(key)
            .map_or(&[], |relationship| &relationship.sub_actions)
    }

    /// Walks up from the parent of `key` to its root.
    pub fn ancestors(&self, key: ActionNodeKey) -> impl Iterator<Item = ActionNodeKey> + '_ {
        std::iter::successors(self.parent(key), |&key| self.parent(key))
    }

    /// Iterates the descendants of `key`, each before its own descendants.
    pub fn descendants_top_down(
        &self,
        key: ActionNodeKey,
    ) -> impl Iterator<Item = ActionNodeKey> + '_ {
        let mut pending: Vec<_> = self.children(key).iter().rev().copied().collect();
        std::iter::from_fn(move || {
            let key = pending.pop()?;
            pending.extend(self.children(key).iter().rev());
            Some(key)
        })
    }

    /// Iterates the descendants of `key`, each after its own descendants.
    pub fn descendants_bottom_up(
        &self,
        key: ActionNodeKey,
    ) -> impl Iterator<Item = ActionNodeKey> + '_ {
        // `true` once a node's children have been pushed above it.
        let mut pending: Vec<_> = self
            .children(key)
            .iter()
            .rev()
            .map(|&key| (key, false))
            .collect();
        std::iter::from_fn(move || {
            loop {
                let (key, expanded) = pending.pop()?;
                if expanded {
                    return Some(key);
                }
                pending.push((key, true));
                pending.extend(self.children(key).iter().rev().map(|&key| (key, false)));
            }
        })
    }

    pub fn is_executing(&self, key: ActionNodeKey) -> bool {
        // An executing action's body is borrowed out of `action_nodes`.
        self.relationships.contains_key(key) && !self.action_nodes.contains_key(key)
    }

    pub fn has_executing_descendant(&self, key: ActionNodeKey) -> bool {
        self.descendants_top_down(key)
            .any(|key| self.is_executing(key))
    }

    /// Removes every descendant of `key`, returning the removed keys bottom up.
    pub fn remove_descendants(&mut self, key: ActionNodeKey) -> Vec<ActionNodeKey> {
        let removed: Vec<_> = self.descendants_bottom_up(key).collect();
        for &key in &removed {
            self.relationships.remove(key);
            self.action_nodes.remove(key);
        }
        if let Some(relationship) = self.relationships.get_mut(key) {
            relationship.sub_actions.clear();
        }
        removed
    }

    fn insert(
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
        if let Some(parent) = parent {
            self.relationships[parent].sub_actions.push(key);
        }
        key
    }
}

fn tree_mut(ctx: &mut Context) -> &mut ActionTree {
    ctx.get_or_insert_with(ActionTree::default)
}

pub trait ActionTreeExt {
    /// Creates a root, even when another action is executing.
    fn create_root(&mut self, action: impl Action<Output = ()> + 'static) -> ActionNodeKey;

    /// Creates a child of the executing action, or returns `None` outside execution.
    fn create_child(&mut self, action: impl Action<Output = ()> + 'static)
    -> Option<ActionNodeKey>;

    /// Creates and immediately runs an action as a child of the executing
    /// action, or as a root outside execution.
    fn run(&mut self, action: impl Action<Output = ()> + 'static) -> Result<(), SelfAdjust>;

    /// Whether `key` exists, isn't executing, and has no executing descendant.
    fn can_run(&self, key: ActionNodeKey) -> bool;

    /// Removes every descendant of `key`, returning the removed keys so
    /// callers can clean up their own per-node data.
    fn clear_children(&mut self, key: ActionNodeKey) -> Vec<ActionNodeKey>;

    /// Runs an existing action without clearing it first. Returns `None` for
    /// an unknown key, an executing action, or one that still has children.
    fn execute(&mut self, key: ActionNodeKey) -> Option<Result<(), SelfAdjust>>;

    /// Reruns an existing action: clears its children, then executes it.
    /// Returns `None` unless [`can_run`](Self::can_run).
    fn run_action(&mut self, key: ActionNodeKey) -> Option<Result<(), SelfAdjust>>;
}

impl ActionTreeExt for Context {
    fn create_root(&mut self, action: impl Action<Output = ()> + 'static) -> ActionNodeKey {
        tree_mut(self).insert(action, None)
    }

    fn create_child(
        &mut self,
        action: impl Action<Output = ()> + 'static,
    ) -> Option<ActionNodeKey> {
        let tree = tree_mut(self);
        let parent = tree.current_action?;
        Some(tree.insert(action, Some(parent)))
    }

    fn run(&mut self, action: impl Action<Output = ()> + 'static) -> Result<(), SelfAdjust> {
        let tree = tree_mut(self);
        let key = tree.insert(action, tree.current_action);
        self.execute(key).expect("new action is available to run")
    }

    fn can_run(&self, key: ActionNodeKey) -> bool {
        self.get::<ActionTree>().is_some_and(|tree| {
            tree.action_nodes.contains_key(key) && !tree.has_executing_descendant(key)
        })
    }

    fn clear_children(&mut self, key: ActionNodeKey) -> Vec<ActionNodeKey> {
        tree_mut(self).remove_descendants(key)
    }

    fn execute(&mut self, key: ActionNodeKey) -> Option<Result<(), SelfAdjust>> {
        let tree = tree_mut(self);
        // No children also means no executing descendant.
        if !tree.children(key).is_empty() {
            return None;
        }
        let mut action = tree.action_nodes.remove(key)?;
        let previous_action = tree.current_action.replace(key);
        let result = catch_unwind(AssertUnwindSafe(|| action.run(self)));
        let tree = tree_mut(self);
        tree.current_action = previous_action;
        tree.action_nodes.insert(key, action);
        match result {
            Ok(result) => Some(result),
            Err(panic) => resume_unwind(panic),
        }
    }

    fn run_action(&mut self, key: ActionNodeKey) -> Option<Result<(), SelfAdjust>> {
        if !self.can_run(key) {
            return None;
        }
        self.clear_children(key);
        self.execute(key)
    }
}

pub fn finish_with<T>(val: T) -> Result<T, SelfAdjust> {
    Ok(val)
}

pub fn finish() -> Result<(), SelfAdjust> {
    finish_with(())
}
