//! Groups: sets of nodes that have something in common.
//!
//! A typical use is tracking which nodes depend on a piece of state, so that
//! you know which nodes to rerun when it changes. Create a group with
//! [`create_group`](GroupingExt::create_group), then add and remove nodes:
//!
//! ```
//! use caaqi::{group::GroupingExt, prelude::*};
//!
//! let mut ctx = Context::new();
//! let node = ctx.create_node(|_: &mut Context, _| {}).id();
//! let readers = ctx.create_group();
//!
//! ctx.add_to_group(readers, node)?;
//! assert!(ctx.group_contains(readers, node)?);
//!
//! ctx.remove_from_group(readers, node)?;
//! assert!(!ctx.group_contains(readers, node)?);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! # Deleted nodes stay in their groups
//!
//! Groups aren't told when a node is deleted, so a node stays in its groups
//! until you remove it with
//! [`remove_from_group`](GroupingExt::remove_from_group), or remove the
//! group. Remove a node from its groups before you delete it.
//!
//! If you don't, [`group_members`](GroupingExt::group_members) and
//! [`remove_group`](GroupingExt::remove_group) still return the deleted
//! node's key. You can still remove that key with `remove_from_group`.

use std::collections::HashSet;

use slotmap::{SecondaryMap, SlotMap, new_key_type};

use crate::{
    context::Context,
    trace::{NodeKey, TraceExt, UnknownNode},
};

new_key_type! {
    /// Identifies a group. Get one from [`GroupingExt::create_group`].
    pub struct GroupId;
}

/// Returned when a [`GroupId`] doesn't refer to an existing group, for
/// example because the group was removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("group {0:?} doesn't exist")]
pub struct UnknownGroup(pub GroupId);

/// Why [`GroupingExt::add_to_group`] failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AddToGroupError {
    #[error(transparent)]
    UnknownGroup(#[from] UnknownGroup),
    #[error(transparent)]
    UnknownNode(#[from] UnknownNode),
}

/// Why [`GroupingExt::group_contains`] failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum GroupContainsError {
    #[error(transparent)]
    UnknownGroup(#[from] UnknownGroup),
    #[error(transparent)]
    UnknownNode(#[from] UnknownNode),
}

/// The groups resource: every group and its members. Everything public goes
/// through [`GroupingExt`].
///
/// Membership is stored both ways, group to nodes and node to groups, and
/// the two mirror each other exactly for nodes in the trace. A deleted node's
/// key stays in `members` until it's removed; its `memberships` entry stays
/// too, until then or until its slot is reused, which replaces the entry.
#[derive(Debug, Default)]
struct Groups {
    members: SlotMap<GroupId, HashSet<NodeKey>>,
    memberships: SecondaryMap<NodeKey, HashSet<GroupId>>,
}

impl Groups {
    fn exists(&self, group: GroupId) -> bool {
        self.members.contains_key(group)
    }

    fn group(&self, group: GroupId) -> Result<&HashSet<NodeKey>, UnknownGroup> {
        self.members.get(group).ok_or(UnknownGroup(group))
    }

    fn contains(&self, group: GroupId, node: NodeKey) -> Result<bool, UnknownGroup> {
        Ok(self.group(group)?.contains(&node))
    }

    fn members(&self, group: GroupId) -> Result<impl Iterator<Item = NodeKey> + '_, UnknownGroup> {
        Ok(self.group(group)?.iter().copied())
    }

    fn groups_of(&self, node: NodeKey) -> impl Iterator<Item = GroupId> + '_ {
        self.memberships.get(node).into_iter().flatten().copied()
    }

    /// Adds `node`, which must be in the trace, to `group` in both
    /// directions. Returns `Ok(false)` if it was already a member.
    fn add_membership(&mut self, group: GroupId, node: NodeKey) -> Result<bool, UnknownGroup> {
        let members = self.members.get_mut(group).ok_or(UnknownGroup(group))?;
        let added = members.insert(node);
        if added {
            self.memberships
                .entry(node)
                .expect("a node in the trace has the newest key for its slot")
                .or_default()
                .insert(group);
        }
        Ok(added)
    }

    /// Removes `node` from `group` in both directions. Returns `Ok(false)` if
    /// it wasn't a member. `in_trace` says whether `node` is in the trace:
    /// only then are both directions sure to mirror each other.
    fn remove_membership(
        &mut self,
        group: GroupId,
        node: NodeKey,
        in_trace: bool,
    ) -> Result<bool, UnknownGroup> {
        let members = self.members.get_mut(group).ok_or(UnknownGroup(group))?;
        let removed = members.remove(&node);
        if removed {
            let unlinked = self.unlink(node, group);
            assert!(unlinked || !in_trace, "memberships mirror members");
        }
        Ok(removed)
    }

    /// Removes `group` from the groups of `node`: the `memberships` half of
    /// [`remove_membership`](Self::remove_membership), for callers that have
    /// already updated `members`. Returns whether it was there, which it
    /// always is for a node in the trace. A stale key's entry may be gone,
    /// replaced by a node that reused its slot.
    fn unlink(&mut self, node: NodeKey, group: GroupId) -> bool {
        let Some(groups) = self.memberships.get_mut(node) else {
            return false;
        };
        let removed = groups.remove(&group);
        if groups.is_empty() {
            self.memberships.remove(node);
        }
        removed
    }
}

fn groups_mut(ctx: &mut Context) -> &mut Groups {
    ctx.get_or_insert_with(Groups::default)
}

/// The groups resource, for a read about `group`. Without it no group exists
/// yet.
fn groups(ctx: &Context, group: GroupId) -> Result<&Groups, UnknownGroup> {
    ctx.get::<Groups>().ok_or(UnknownGroup(group))
}

/// Creating groups and managing their members. See the [module docs](self)
/// for an example, and for what happens to deleted nodes.
pub trait GroupingExt {
    /// Whether `group` exists: it was created and hasn't been removed.
    fn group_exists(&self, group: GroupId) -> bool;

    /// Whether `node` is in `group`.
    ///
    /// Fails if `group` doesn't exist, or if `node` isn't in the trace.
    fn group_contains(&self, group: GroupId, node: NodeKey) -> Result<bool, GroupContainsError>;

    /// The nodes in `group`, in no particular order. This includes deleted
    /// nodes that weren't removed from the group.
    ///
    /// Fails if `group` doesn't exist.
    fn group_members(
        &self,
        group: GroupId,
    ) -> Result<impl Iterator<Item = NodeKey> + '_, UnknownGroup>;

    /// The groups `node` is in, in no particular order.
    ///
    /// Fails if `node` isn't in the trace.
    fn groups_of(&self, node: NodeKey) -> Result<impl Iterator<Item = GroupId> + '_, UnknownNode>;

    /// Creates an empty group and returns its id.
    fn create_group(&mut self) -> GroupId;

    /// Removes `group` and returns the nodes that were in it, including
    /// deleted nodes that weren't removed from it.
    ///
    /// Fails if `group` doesn't exist.
    fn remove_group(&mut self, group: GroupId) -> Result<HashSet<NodeKey>, UnknownGroup>;

    /// Adds `node` to `group`. Returns `Ok(false)` if it was already in it.
    ///
    /// The node stays in the group until you remove it with
    /// [`remove_from_group`](Self::remove_from_group) or remove the group,
    /// even if the node is deleted.
    ///
    /// Fails if `group` doesn't exist or `node` isn't in the trace.
    fn add_to_group(&mut self, group: GroupId, node: NodeKey) -> Result<bool, AddToGroupError>;

    /// Removes `node` from `group`. Returns `Ok(false)` if it wasn't in it.
    ///
    /// This works for deleted nodes too, so you can clean up after a node
    /// that was deleted while still in a group. Fails only if `group` doesn't
    /// exist.
    fn remove_from_group(&mut self, group: GroupId, node: NodeKey) -> Result<bool, UnknownGroup>;
}

impl GroupingExt for Context {
    fn group_exists(&self, group: GroupId) -> bool {
        self.get::<Groups>()
            .is_some_and(|groups| groups.exists(group))
    }

    fn group_contains(&self, group: GroupId, node: NodeKey) -> Result<bool, GroupContainsError> {
        self.node(node)?;
        Ok(groups(self, group)?.contains(group, node)?)
    }

    fn group_members(
        &self,
        group: GroupId,
    ) -> Result<impl Iterator<Item = NodeKey> + '_, UnknownGroup> {
        groups(self, group)?.members(group)
    }

    fn groups_of(&self, node: NodeKey) -> Result<impl Iterator<Item = GroupId> + '_, UnknownNode> {
        // A deleted node's entry may be left behind, so check the node.
        self.node(node)?;
        Ok(self
            .get::<Groups>()
            .into_iter()
            .flat_map(move |groups| groups.groups_of(node)))
    }

    fn create_group(&mut self) -> GroupId {
        groups_mut(self).members.insert(HashSet::new())
    }

    fn remove_group(&mut self, group: GroupId) -> Result<HashSet<NodeKey>, UnknownGroup> {
        let members = groups_mut(self)
            .members
            .remove(group)
            .ok_or(UnknownGroup(group))?;
        let in_trace: Vec<_> = members
            .iter()
            .map(|&node| self.contains_node(node))
            .collect();
        let groups = groups_mut(self);
        for (&node, in_trace) in members.iter().zip(in_trace) {
            let unlinked = groups.unlink(node, group);
            assert!(unlinked || !in_trace, "memberships mirror members");
        }
        Ok(members)
    }

    fn add_to_group(&mut self, group: GroupId, node: NodeKey) -> Result<bool, AddToGroupError> {
        self.node(node)?;
        Ok(groups_mut(self).add_membership(group, node)?)
    }

    fn remove_from_group(&mut self, group: GroupId, node: NodeKey) -> Result<bool, UnknownGroup> {
        let in_trace = self.contains_node(node);
        groups_mut(self).remove_membership(group, node, in_trace)
    }
}

#[cfg(test)]
#[path = "group_tests.rs"]
mod tests;
