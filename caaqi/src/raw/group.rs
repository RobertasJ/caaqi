//! Groups: sets of nodes, each identified by a [`GroupId`], built on the
//! trace and rewinds.
//!
//! Groups are a building block for tracking which nodes have something in
//! common, for example which nodes depend on a piece of state. Everything
//! public goes through [`GroupingExt`].
//!
//! # Cleanup is manual
//!
//! The trace doesn't notify anyone when a node is deleted, and groups don't
//! track deletion: nothing removes a deleted node from its groups. A
//! membership lasts until it's removed with
//! [`remove_from_group`](GroupingExt::remove_from_group) or its group is
//! removed.
//!
//! Most memberships should last only one run (a node that reads something
//! joins its group for the duration of that run). For those, use
//! [`add_to_group_until_rewind`](GroupingExt::add_to_group_until_rewind),
//! which removes the membership when the node is rewound. Nodes are rewound
//! before they're deleted, so this also keeps deleted nodes out of the
//! group. Memberships made with [`add_to_group`](GroupingExt::add_to_group)
//! have to be removed by hand.
//!
//! Otherwise, a deleted node stays in its groups as a stale key: reads return
//! what's stored, so [`group_members`](GroupingExt::group_members) yields it,
//! and [`remove_from_group`](GroupingExt::remove_from_group) accepts it.

use std::collections::{HashMap, HashSet};

use slotmap::SecondaryMap;

use crate::{
    context::Context,
    id::Id,
    raw::{
        rewind::{RewindExt, RewindKey},
        trace::{NodeKey, TraceExt, UnknownNode},
    },
};

/// A group, created by [`GroupingExt::create_group`]. Ids are unique across
/// the whole process, so an id is never reused, even in another context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GroupId(Id);

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("group {0:?} doesn't exist")]
pub struct UnknownGroup(pub GroupId);

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AddToGroupError {
    #[error(transparent)]
    UnknownGroup(#[from] UnknownGroup),
    #[error(transparent)]
    UnknownNode(#[from] UnknownNode),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum GroupContainsError {
    #[error(transparent)]
    UnknownGroup(#[from] UnknownGroup),
    #[error(transparent)]
    UnknownNode(#[from] UnknownNode),
}

/// The groups resource: every group and its members.
///
/// Membership is stored both ways, group to nodes and node to groups, so a
/// node's groups can be read without scanning every group. For nodes in the
/// trace, the two directions mirror each other exactly. A deleted node's
/// stale key stays in `members` until it's removed, and its `memberships`
/// entry stays too, until then or until its slot is reused, which replaces
/// the entry.
///
/// Everything public goes through [`GroupingExt`].
#[derive(Debug, Default)]
pub struct Groups {
    members: HashMap<GroupId, HashSet<NodeKey>>,
    memberships: SecondaryMap<NodeKey, HashSet<GroupId>>,
}

impl Groups {
    fn exists(&self, group: GroupId) -> bool {
        self.members.contains_key(&group)
    }

    fn group(&self, group: GroupId) -> Result<&HashSet<NodeKey>, UnknownGroup> {
        self.members.get(&group).ok_or(UnknownGroup(group))
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
        let members = self.members.get_mut(&group).ok_or(UnknownGroup(group))?;
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
        let members = self.members.get_mut(&group).ok_or(UnknownGroup(group))?;
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

/// Creating groups and managing their members. Cleanup is manual: see the
/// [module docs](self).
pub trait GroupingExt {
    /// Whether `group` exists.
    fn group_exists(&self, group: GroupId) -> bool;

    /// Whether `node` is a member of `group`.
    ///
    /// Fails if `group` doesn't exist, or if `node` isn't in the trace, stale
    /// keys included.
    fn group_contains(&self, group: GroupId, node: NodeKey) -> Result<bool, GroupContainsError>;

    /// The members of `group`, in no particular order. Deleted nodes that
    /// didn't leave the group are included, as stale keys.
    fn group_members(
        &self,
        group: GroupId,
    ) -> Result<impl Iterator<Item = NodeKey> + '_, UnknownGroup>;

    /// The groups `node` belongs to, in no particular order. Fails if `node`
    /// isn't in the trace.
    fn groups_of(&self, node: NodeKey) -> Result<impl Iterator<Item = GroupId> + '_, UnknownNode>;

    /// Creates an empty group.
    fn create_group(&mut self) -> GroupId;

    /// Removes `group`, returning its members, stale keys included.
    fn remove_group(&mut self, group: GroupId) -> Result<HashSet<NodeKey>, UnknownGroup>;

    /// Adds `node` to `group`. Returns `Ok(false)` if it was already a
    /// member.
    ///
    /// The membership lasts until it's removed with
    /// [`remove_from_group`](Self::remove_from_group) or the group is
    /// removed, even if `node` is deleted. For one that lasts a run, use
    /// [`add_to_group_until_rewind`](Self::add_to_group_until_rewind).
    ///
    /// Fails if `group` doesn't exist or `node` isn't in the trace.
    fn add_to_group(&mut self, group: GroupId, node: NodeKey) -> Result<bool, AddToGroupError>;

    /// Adds `node` to `group`, and registers a rewind on `node` that removes
    /// it again. The membership lasts until the node is rewound: a rerun has
    /// to join again. Since nodes are rewound before they're deleted, this
    /// also keeps deleted nodes out of the group.
    ///
    /// The rewind is registered even if `node` was already a member, so it
    /// ends that membership too. If the group or the membership is already
    /// gone when the rewind runs, it does nothing.
    fn add_to_group_until_rewind(
        &mut self,
        group: GroupId,
        node: NodeKey,
    ) -> Result<RewindKey, AddToGroupError>;

    /// Removes `node` from `group`. Returns `Ok(false)` if it wasn't a
    /// member.
    ///
    /// `node` doesn't have to be in the trace: this is how a deleted node's
    /// stale key is removed. Fails only if `group` doesn't exist.
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
        let group = GroupId(Id::new());
        groups_mut(self).members.insert(group, HashSet::new());
        group
    }

    fn remove_group(&mut self, group: GroupId) -> Result<HashSet<NodeKey>, UnknownGroup> {
        let members = groups_mut(self)
            .members
            .remove(&group)
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

    fn add_to_group_until_rewind(
        &mut self,
        group: GroupId,
        node: NodeKey,
    ) -> Result<RewindKey, AddToGroupError> {
        self.add_to_group(group, node)?;
        let rewind = self
            .register_rewind(node, move |ctx, _| {
                // `Ok(false)`: the membership was already removed.
                // `Err(UnknownGroup)`: the group was removed, taking the
                // membership with it.
                let _: Result<bool, UnknownGroup> = ctx.remove_from_group(group, node);
            })
            .expect("`add_to_group` checked that the node is in the trace");
        Ok(rewind)
    }

    fn remove_from_group(&mut self, group: GroupId, node: NodeKey) -> Result<bool, UnknownGroup> {
        let in_trace = self.contains_node(node);
        groups_mut(self).remove_membership(group, node, in_trace)
    }
}

#[cfg(test)]
#[path = "group_tests.rs"]
mod tests;
