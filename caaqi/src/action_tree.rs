use slotmap::{SlotMap, new_key_type};
use smallvec::SmallVec;

use crate::{
    context::Context,
    current::CurrentActionExt,
    lifecycle::{notify_node_added, notify_nodes_removed},
};

new_key_type! {
    pub struct ActionNodeKey;
}

/// The action tree resource. It holds only the shape of the tree; per-node
/// data lives in other resources keyed by [`ActionNodeKey`].
///
/// Its read methods are public so other modules can build on it; mutation goes
/// through [`ActionTreeExt`] so [node observers](crate::lifecycle::NodeObserver)
/// are notified.
#[derive(Default)]
pub struct ActionTree {
    nodes: SlotMap<ActionNodeKey, ActionNode>,
}

struct ActionNode {
    sub_actions: SmallVec<[ActionNodeKey; 1]>,
    parent: Option<ActionNodeKey>,
}

impl ActionTree {
    pub fn contains(&self, key: ActionNodeKey) -> bool {
        self.nodes.contains_key(key)
    }

    pub fn parent(&self, key: ActionNodeKey) -> Option<ActionNodeKey> {
        self.nodes.get(key).and_then(|node| node.parent)
    }

    pub fn children(&self, key: ActionNodeKey) -> &[ActionNodeKey] {
        self.nodes.get(key).map_or(&[], |node| &node.sub_actions)
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

    fn insert(&mut self, parent: Option<ActionNodeKey>) -> ActionNodeKey {
        // Only fresh nodes can be attached, so parenting cannot create cycles.
        let key = self.nodes.insert(ActionNode {
            sub_actions: SmallVec::new(),
            parent,
        });
        if let Some(parent) = parent {
            self.nodes[parent].sub_actions.push(key);
        }
        key
    }

    /// Removes every descendant of `key`, returning the removed keys bottom up.
    fn remove_descendants(&mut self, key: ActionNodeKey) -> Vec<ActionNodeKey> {
        let removed: Vec<_> = self.descendants_bottom_up(key).collect();
        for &key in &removed {
            self.nodes.remove(key);
        }
        if let Some(node) = self.nodes.get_mut(key) {
            node.sub_actions.clear();
        }
        removed
    }

    /// Removes `key` and its descendants, returning the removed keys bottom
    /// up, so `key` comes last.
    fn remove(&mut self, key: ActionNodeKey) -> Vec<ActionNodeKey> {
        let mut removed = self.remove_descendants(key);
        if let Some(node) = self.nodes.remove(key) {
            if let Some(parent) = node.parent {
                self.nodes[parent]
                    .sub_actions
                    .retain(|&mut child| child != key);
            }
            removed.push(key);
        }
        removed
    }
}

fn tree_mut(ctx: &mut Context) -> &mut ActionTree {
    ctx.get_or_insert_with(ActionTree::default)
}

pub trait ActionTreeExt {
    /// Creates a root, even when another action is executing.
    fn create_root(&mut self) -> ActionNodeKey;

    /// Creates a child of the executing action, or returns `None` outside execution.
    fn create_child(&mut self) -> Option<ActionNodeKey>;

    /// Whether `key` is the current action or one of its ancestors.
    fn has_executing(&self, key: ActionNodeKey) -> bool;

    /// Whether the current action is a strict descendant of `key`.
    fn has_executing_descendant(&self, key: ActionNodeKey) -> bool;

    /// Removes every descendant of `key`, returning the removed keys bottom
    /// up. Returns `None` for an unknown key or one with an executing
    /// descendant.
    fn clear_children(&mut self, key: ActionNodeKey) -> Option<Vec<ActionNodeKey>>;

    /// Removes `key` and its descendants, returning the removed keys bottom
    /// up. Returns `None` for an unknown key or an executing one.
    fn remove_node(&mut self, key: ActionNodeKey) -> Option<Vec<ActionNodeKey>>;
}

impl ActionTreeExt for Context {
    fn create_root(&mut self) -> ActionNodeKey {
        let key = tree_mut(self).insert(None);
        notify_node_added(self, key);
        key
    }

    fn create_child(&mut self) -> Option<ActionNodeKey> {
        let parent = self.current_action()?;
        let key = tree_mut(self).insert(Some(parent));
        notify_node_added(self, key);
        Some(key)
    }

    fn has_executing(&self, key: ActionNodeKey) -> bool {
        self.current_action() == Some(key) || self.has_executing_descendant(key)
    }

    fn has_executing_descendant(&self, key: ActionNodeKey) -> bool {
        // Actions aren't stored, so the only executing actions are the
        // current one and its ancestors.
        let Some(current) = self.current_action() else {
            return false;
        };
        self.get::<ActionTree>()
            .is_some_and(|tree| tree.ancestors(current).any(|ancestor| ancestor == key))
    }

    fn clear_children(&mut self, key: ActionNodeKey) -> Option<Vec<ActionNodeKey>> {
        if !self.get::<ActionTree>()?.contains(key) || self.has_executing_descendant(key) {
            return None;
        }
        let removed = tree_mut(self).remove_descendants(key);
        notify_nodes_removed(self, &removed);
        Some(removed)
    }

    fn remove_node(&mut self, key: ActionNodeKey) -> Option<Vec<ActionNodeKey>> {
        if !self.get::<ActionTree>()?.contains(key) || self.has_executing(key) {
            return None;
        }
        let removed = tree_mut(self).remove(key);
        notify_nodes_removed(self, &removed);
        Some(removed)
    }
}
