//! Groups of nodes, like [`caaqi::group`].

use std::collections::HashSet;

pub use caaqi::group::{AddToGroupError, GroupContainsError, GroupId, UnknownGroup};
use caaqi::{group::GroupingExt, trace::NodeKey};

use crate::{context::with_ctx, trace::TraceNode};

/// A group of nodes: a newtype over caaqi's [`GroupId`] with the group's
/// operations as methods. Convert with `From` either way.
///
/// Once the group is [deleted](Self::delete), its methods fail with
/// [`UnknownGroup`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Group(GroupId);

impl From<GroupId> for Group {
    fn from(id: GroupId) -> Self {
        Group(id)
    }
}

impl From<Group> for GroupId {
    fn from(group: Group) -> Self {
        group.0
    }
}

impl Group {
    /// [`GroupingExt::group_exists`].
    pub fn exists(self) -> bool {
        with_ctx(|ctx| ctx.group_exists(self.0))
    }

    /// [`GroupingExt::group_contains`].
    pub fn contains(self, node: TraceNode) -> Result<bool, GroupContainsError> {
        with_ctx(|ctx| ctx.group_contains(self.0, node.into()))
    }

    /// [`GroupingExt::group_members`]. The members are collected first, so
    /// the context can change while iterating.
    pub fn members(self) -> Result<impl Iterator<Item = TraceNode>, UnknownGroup> {
        with_ctx(|ctx| {
            Ok(ctx
                .group_members(self.0)?
                .map(TraceNode::from)
                .collect::<Vec<_>>()
                .into_iter())
        })
    }

    /// [`GroupingExt::add_to_group`].
    pub fn insert(self, node: TraceNode) -> Result<bool, AddToGroupError> {
        with_ctx(|ctx| ctx.add_to_group(self.0, node.into()))
    }

    /// [`GroupingExt::remove_from_group`].
    pub fn remove(self, node: TraceNode) -> Result<bool, UnknownGroup> {
        with_ctx(|ctx| ctx.remove_from_group(self.0, node.into()))
    }

    /// [`GroupingExt::remove_group`]: removes the group, and returns its
    /// members.
    pub fn delete(self) -> Result<HashSet<TraceNode>, UnknownGroup> {
        with_ctx(|ctx| {
            Ok(ctx
                .remove_group(self.0)?
                .into_iter()
                .map(TraceNode::from)
                .collect())
        })
    }
}

/// [`GroupingExt::create_group`].
pub fn create_group() -> Group {
    with_ctx(|ctx| Group(ctx.create_group()))
}

/// The node's side of its groups, which are reached only through their
/// [`Group`].
impl TraceNode {
    /// [`GroupingExt::group_count`]: how many groups the node is in.
    ///
    /// # Panics
    ///
    /// If the node was deleted.
    pub fn group_count(self) -> usize {
        let key = NodeKey::from(self);
        with_ctx(|ctx| ctx.group_count(key)).unwrap_or_else(|_| panic!("node {key:?} was deleted"))
    }

    /// [`GroupingExt::remove_from_all_groups`]: removes the node from every
    /// group it's in, and returns how many that was.
    ///
    /// # Panics
    ///
    /// If the node was deleted.
    pub fn remove_from_all_groups(self) -> usize {
        let key = NodeKey::from(self);
        with_ctx(|ctx| ctx.remove_from_all_groups(key))
            .unwrap_or_else(|_| panic!("node {key:?} was deleted"))
    }
}

#[cfg(test)]
#[path = "group_tests.rs"]
mod tests;
