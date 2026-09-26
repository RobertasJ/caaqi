use std::collections::{HashMap, HashSet};

use slotmap::SecondaryMap;

use crate::{
    action_tree::{ActionNodeKey, ActionTree, UnknownNode, tree_ref},
    context::Context,
    id::Id,
    lifecycle::{LifecycleExt, NodeObserver},
};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RemoveFromGroupError {
    #[error(transparent)]
    UnknownGroup(#[from] UnknownGroup),
    #[error(transparent)]
    UnknownNode(#[from] UnknownNode),
}

/// The grouping resource: sets of action nodes, each identified by a
/// [`GroupId`].
///
/// Membership is stored both ways so a removed node can leave all its groups
/// without scanning every group. Nodes leave their groups when they are
/// removed from the tree.
#[derive(Debug, Default)]
pub struct Groups {
    members: HashMap<GroupId, HashSet<ActionNodeKey>>,
    memberships: SecondaryMap<ActionNodeKey, HashSet<GroupId>>,
}

impl Groups {
    fn exists(&self, group: GroupId) -> bool {
        self.members.contains_key(&group)
    }

    fn group(&self, group: GroupId) -> Result<&HashSet<ActionNodeKey>, UnknownGroup> {
        self.members.get(&group).ok_or(UnknownGroup(group))
    }

    fn contains(&self, group: GroupId, key: ActionNodeKey) -> Result<bool, UnknownGroup> {
        Ok(self.group(group)?.contains(&key))
    }

    fn members(
        &self,
        group: GroupId,
    ) -> Result<impl Iterator<Item = ActionNodeKey> + '_, UnknownGroup> {
        Ok(self.group(group)?.iter().copied())
    }

    fn groups_of(&self, key: ActionNodeKey) -> impl Iterator<Item = GroupId> + '_ {
        self.memberships.get(key).into_iter().flatten().copied()
    }

    /// Adds `key` to `group` in both directions. Returns `Ok(false)` if it
    /// was already a member.
    fn add_membership(&mut self, group: GroupId, key: ActionNodeKey) -> Result<bool, UnknownGroup> {
        let members = self.members.get_mut(&group).ok_or(UnknownGroup(group))?;
        let added = members.insert(key);
        if added {
            self.memberships
                .entry(key)
                .expect("node is in the tree")
                .or_default()
                .insert(group);
        }
        Ok(added)
    }

    /// Removes `key` from `group` in both directions. Returns `Ok(false)` if
    /// it wasn't a member.
    fn remove_membership(
        &mut self,
        group: GroupId,
        key: ActionNodeKey,
    ) -> Result<bool, UnknownGroup> {
        let members = self.members.get_mut(&group).ok_or(UnknownGroup(group))?;
        let removed = members.remove(&key);
        if removed {
            self.remove_group_of(key, group);
        }
        Ok(removed)
    }

    /// Removes `group` from the groups of `key`: the `memberships` half of
    /// [`remove_membership`](Self::remove_membership), for callers that have
    /// already updated `members`.
    fn remove_group_of(&mut self, key: ActionNodeKey, group: GroupId) {
        let groups = self
            .memberships
            .get_mut(key)
            .expect("memberships mirror members");
        groups.remove(&group);
        if groups.is_empty() {
            self.memberships.remove(key);
        }
    }

    /// Takes `key` out of every group it belongs to.
    fn remove_node(&mut self, key: ActionNodeKey) {
        // Copied because `remove_membership` edits the list being iterated.
        for group in self.memberships.get(key).cloned().unwrap_or_default() {
            let removed = self
                .remove_membership(group, key)
                .expect("memberships mirror members");
            assert!(removed, "memberships mirror members");
        }
    }
}

impl NodeObserver for Groups {
    fn nodes_removed(ctx: &mut Context, keys: &[ActionNodeKey]) {
        if let Some(groups) = ctx.get_mut::<Groups>() {
            for &key in keys {
                groups.remove_node(key);
            }
        }
    }
}

fn groups_mut(ctx: &mut Context) -> &mut Groups {
    if ctx.get::<Groups>().is_none() {
        ctx.observe_nodes::<Groups>();
    }
    ctx.get_or_insert_with(Groups::default)
}

pub trait GroupingExt {
    /// Whether `group` exists.
    fn group_exists(&self, group: GroupId) -> bool;

    /// Whether `key` is a member of `group`.
    fn group_contains(
        &self,
        group: GroupId,
        key: ActionNodeKey,
    ) -> Result<bool, GroupContainsError>;

    /// The members of `group`, in no particular order.
    fn group_members(
        &self,
        group: GroupId,
    ) -> Result<impl Iterator<Item = ActionNodeKey> + '_, UnknownGroup>;

    /// The groups `key` belongs to, in no particular order.
    fn groups_of(
        &self,
        key: ActionNodeKey,
    ) -> Result<impl Iterator<Item = GroupId> + '_, UnknownNode>;

    /// Creates an empty group.
    fn create_group(&mut self) -> GroupId;

    /// Removes `group`, returning its members.
    fn remove_group(&mut self, group: GroupId) -> Result<HashSet<ActionNodeKey>, UnknownGroup>;

    /// Adds `key` to `group`. Returns `Ok(false)` if it was already a member.
    fn add_to_group(&mut self, group: GroupId, key: ActionNodeKey)
    -> Result<bool, AddToGroupError>;

    /// Removes `key` from `group`. Returns `Ok(false)` if it wasn't a member.
    fn remove_from_group(
        &mut self,
        group: GroupId,
        key: ActionNodeKey,
    ) -> Result<bool, RemoveFromGroupError>;
}

/// The groups resource, for a read about `group`. Without it no group exists
/// yet.
fn groups(ctx: &Context, group: GroupId) -> Result<&Groups, UnknownGroup> {
    ctx.get::<Groups>().ok_or(UnknownGroup(group))
}

impl GroupingExt for Context {
    fn group_exists(&self, group: GroupId) -> bool {
        self.get::<Groups>()
            .is_some_and(|groups| groups.exists(group))
    }

    fn group_contains(
        &self,
        group: GroupId,
        key: ActionNodeKey,
    ) -> Result<bool, GroupContainsError> {
        tree_ref(self, key)?.node(key)?;
        Ok(groups(self, group)?.contains(group, key)?)
    }

    fn group_members(
        &self,
        group: GroupId,
    ) -> Result<impl Iterator<Item = ActionNodeKey>, UnknownGroup> {
        groups(self, group)?.members(group)
    }

    fn groups_of(&self, key: ActionNodeKey) -> Result<impl Iterator<Item = GroupId>, UnknownNode> {
        tree_ref(self, key)?.node(key)?;
        Ok(self
            .get::<Groups>()
            .into_iter()
            .flat_map(move |groups| groups.groups_of(key)))
    }

    fn create_group(&mut self) -> GroupId {
        let group = GroupId(Id::new());
        groups_mut(self).members.insert(group, HashSet::new());
        group
    }

    fn remove_group(&mut self, group: GroupId) -> Result<HashSet<ActionNodeKey>, UnknownGroup> {
        let groups = groups_mut(self);
        let members = groups.members.remove(&group).ok_or(UnknownGroup(group))?;
        for &key in &members {
            groups.remove_group_of(key, group);
        }
        Ok(members)
    }

    fn add_to_group(
        &mut self,
        group: GroupId,
        key: ActionNodeKey,
    ) -> Result<bool, AddToGroupError> {
        self.get_or_insert_with(ActionTree::default).node(key)?;
        Ok(groups_mut(self).add_membership(group, key)?)
    }

    fn remove_from_group(
        &mut self,
        group: GroupId,
        key: ActionNodeKey,
    ) -> Result<bool, RemoveFromGroupError> {
        self.get_or_insert_with(ActionTree::default).node(key)?;
        Ok(groups_mut(self).remove_membership(group, key)?)
    }
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;
    use crate::{
        action::ActionExt,
        action_tree::{
            ActionTreeExt, ClearChildrenError, ExecutingDescendant, NodeExecuting, RemoveNodeError,
        },
        current::CurrentActionExt,
    };

    fn members(ctx: &Context, group: GroupId) -> HashSet<ActionNodeKey> {
        ctx.group_members(group).unwrap().collect()
    }

    fn groups_of(ctx: &Context, key: ActionNodeKey) -> HashSet<GroupId> {
        ctx.groups_of(key).unwrap().collect()
    }

    /// Checks that `members` and `memberships` mirror each other exactly.
    fn expect_consistent(ctx: &Context) {
        let Some(groups) = ctx.get::<Groups>() else {
            return;
        };
        for (&group, members) in &groups.members {
            for &key in members {
                expect_true!(
                    groups.groups_of(key).any(|member_of| member_of == group),
                    "{key:?} records its membership of {group:?}",
                );
            }
        }
        for (key, member_of) in &groups.memberships {
            expect_that!(member_of, not(is_empty()), "empty memberships are removed");
            for &group in member_of {
                expect_that!(
                    groups.contains(group, key),
                    ok(eq(true)),
                    "{key:?} is in {group:?}"
                );
            }
        }
    }

    /// A root with two children, the first of which has a child.
    fn tree(ctx: &mut Context) -> [ActionNodeKey; 4] {
        let (root, (a, a_child, b)) = ctx.run_node(|ctx: &mut Context| {
            let (a, a_child) = ctx.run_node(|ctx: &mut Context| ctx.create_branch());
            let b = ctx.create_branch();
            (a, a_child, b)
        });
        [root, a, a_child, b]
    }

    #[gtest]
    fn create_group_makes_distinct_empty_groups() {
        let mut ctx = Context::new();
        let first = ctx.create_group();
        let second = ctx.create_group();

        expect_ne!(first, second);
        expect_true!(ctx.group_exists(first));
        expect_true!(ctx.group_exists(second));
        expect_that!(members(&ctx, first), is_empty());
    }

    #[gtest]
    fn removing_an_empty_group_returns_no_members() {
        let mut ctx = Context::new();
        let group = ctx.create_group();

        expect_that!(ctx.remove_group(group), ok(is_empty()));
        expect_false!(ctx.group_exists(group));
    }

    #[gtest]
    fn add_records_both_directions() {
        let mut ctx = Context::new();
        let group = ctx.create_group();
        let key = ctx.create_root();

        expect_that!(ctx.add_to_group(group, key), ok(eq(true)));
        expect_that!(members(&ctx, group), { eq(&key) });
        expect_that!(groups_of(&ctx, key), { eq(&group) });
        expect_consistent(&ctx);
    }

    #[gtest]
    fn adding_twice_is_rejected() {
        let mut ctx = Context::new();
        let group = ctx.create_group();
        let key = ctx.create_root();
        ctx.add_to_group(group, key).unwrap();

        expect_that!(ctx.add_to_group(group, key), ok(eq(false)));
        expect_that!(groups_of(&ctx, key), { eq(&group) });
        expect_consistent(&ctx);
    }

    #[gtest]
    fn add_rejects_unknown_groups() {
        let mut ctx = Context::new();
        let key = ctx.create_root();
        let foreign = Context::new().create_group();
        expect_that!(
            ctx.add_to_group(foreign, key),
            err(eq(AddToGroupError::UnknownGroup(UnknownGroup(foreign))))
        );

        let removed = ctx.create_group();
        ctx.remove_group(removed).unwrap();
        expect_that!(
            ctx.add_to_group(removed, key),
            err(eq(AddToGroupError::UnknownGroup(UnknownGroup(removed))))
        );

        expect_that!(groups_of(&ctx, key), is_empty());
    }

    #[gtest]
    fn add_rejects_nodes_outside_the_tree() {
        let foreign = Context::new().create_root();
        let mut ctx = Context::new();
        let group = ctx.create_group();
        // No tree exists yet in `ctx`.
        expect_that!(
            ctx.add_to_group(group, foreign),
            err(eq(AddToGroupError::UnknownNode(UnknownNode(foreign))))
        );

        let key = ctx.create_root();
        ctx.remove_node(key).unwrap();
        expect_that!(
            ctx.add_to_group(group, key),
            err(eq(AddToGroupError::UnknownNode(UnknownNode(key))))
        );

        expect_that!(members(&ctx, group), is_empty());
    }

    #[gtest]
    fn removing_a_non_member_changes_nothing() {
        let mut ctx = Context::new();
        let group = ctx.create_group();
        let key = ctx.create_root();
        expect_that!(ctx.remove_from_group(group, key), ok(eq(false)));

        ctx.add_to_group(group, key).unwrap();
        ctx.remove_from_group(group, key).unwrap();
        expect_that!(ctx.remove_from_group(group, key), ok(eq(false)));
    }

    #[gtest]
    fn remove_from_group_rejects_unknown_ids() {
        let mut ctx = Context::new();
        let group = ctx.create_group();
        let key = ctx.create_root();
        let removed_group = ctx.create_group();
        ctx.remove_group(removed_group).unwrap();
        expect_that!(
            ctx.remove_from_group(removed_group, key),
            err(eq(RemoveFromGroupError::UnknownGroup(UnknownGroup(
                removed_group
            ))))
        );

        ctx.add_to_group(group, key).unwrap();
        ctx.remove_node(key).unwrap();
        expect_that!(
            ctx.remove_from_group(group, key),
            err(eq(RemoveFromGroupError::UnknownNode(UnknownNode(key))))
        );
    }

    #[gtest]
    fn nodes_can_rejoin_a_group() {
        let mut ctx = Context::new();
        let group = ctx.create_group();
        let key = ctx.create_root();
        ctx.add_to_group(group, key).unwrap();
        ctx.remove_from_group(group, key).unwrap();

        expect_that!(ctx.add_to_group(group, key), ok(eq(true)));
        expect_that!(groups_of(&ctx, key), { eq(&group) });
        expect_consistent(&ctx);
    }

    #[gtest]
    fn removing_an_unknown_group_changes_nothing() {
        let mut ctx = Context::new();
        let group = ctx.create_group();
        let key = ctx.create_root();
        ctx.add_to_group(group, key).unwrap();
        let removed = ctx.create_group();
        ctx.remove_group(removed).unwrap();

        expect_that!(ctx.remove_group(removed), err(eq(&UnknownGroup(removed))));
        expect_that!(members(&ctx, group), { eq(&key) });
        expect_consistent(&ctx);
    }

    #[gtest]
    fn reused_slots_start_without_groups() {
        let mut ctx = Context::new();
        let group = ctx.create_group();
        let old = ctx.create_root();
        ctx.add_to_group(group, old).unwrap();
        ctx.remove_node(old).unwrap();

        let new = ctx.create_root();
        expect_that!(groups_of(&ctx, new), is_empty());
        expect_that!(members(&ctx, group), is_empty());
    }

    #[gtest]
    fn remove_from_group_clears_both_directions() {
        let mut ctx = Context::new();
        let kept = ctx.create_group();
        let left = ctx.create_group();
        let key = ctx.create_root();
        ctx.add_to_group(kept, key).unwrap();
        ctx.add_to_group(left, key).unwrap();

        expect_that!(ctx.remove_from_group(left, key), ok(eq(true)));
        expect_that!(members(&ctx, left), is_empty());
        expect_that!(groups_of(&ctx, key), { eq(&kept) });

        expect_that!(ctx.remove_from_group(kept, key), ok(eq(true)));
        expect_that!(groups_of(&ctx, key), is_empty());
        expect_consistent(&ctx);
    }

    #[gtest]
    #[should_panic(expected = "memberships mirror members")]
    fn remove_membership_panics_when_directions_diverge() {
        let mut ctx = Context::new();
        let group = ctx.create_group();
        let key = ctx.create_root();
        ctx.add_to_group(group, key).unwrap();
        ctx.get_mut::<Groups>().unwrap().memberships.remove(key);

        ctx.remove_from_group(group, key).unwrap();
    }

    #[gtest]
    fn remove_group_returns_members_and_clears_memberships() {
        let mut ctx = Context::new();
        let removed = ctx.create_group();
        let kept = ctx.create_group();
        let first = ctx.create_root();
        let second = ctx.create_root();
        ctx.add_to_group(removed, first).unwrap();
        ctx.add_to_group(removed, second).unwrap();
        ctx.add_to_group(kept, second).unwrap();

        expect_that!(
            ctx.remove_group(removed),
            ok(unordered_elements_are![eq(&first), eq(&second)])
        );
        expect_false!(ctx.group_exists(removed));
        expect_that!(groups_of(&ctx, first), is_empty());
        expect_that!(groups_of(&ctx, second), { eq(&kept) });
        expect_consistent(&ctx);
    }

    #[gtest]
    fn cleared_children_leave_their_groups() {
        let mut ctx = Context::new();
        let [root, a, a_child, b] = tree(&mut ctx);
        let group = ctx.create_group();
        for key in [root, a, a_child, b] {
            ctx.add_to_group(group, key).unwrap();
        }

        ctx.clear_children(a).unwrap();
        expect_that!(members(&ctx, group), {eq(&root), eq(&a), eq(&b)});
        expect_that!(ctx.groups_of(a_child).err(), some(eq(UnknownNode(a_child))));
        expect_that!(
            ctx.get::<Groups>()
                .unwrap()
                .groups_of(a_child)
                .collect::<Vec<_>>(),
            is_empty()
        );
        expect_consistent(&ctx);
    }

    #[gtest]
    fn removed_nodes_leave_every_group() {
        let mut ctx = Context::new();
        let [root, a, a_child, b] = tree(&mut ctx);
        let first = ctx.create_group();
        let second = ctx.create_group();
        for key in [root, a, a_child, b] {
            ctx.add_to_group(first, key).unwrap();
        }
        ctx.add_to_group(second, a_child).unwrap();
        ctx.add_to_group(second, b).unwrap();

        ctx.remove_node(a).unwrap();
        expect_that!(members(&ctx, first), {eq(&root), eq(&b)});
        expect_that!(members(&ctx, second), { eq(&b) });
        expect_consistent(&ctx);

        ctx.remove_node(root).unwrap();
        expect_that!(members(&ctx, first), is_empty());
        expect_that!(members(&ctx, second), is_empty());
        expect_true!(ctx.group_exists(first));
        expect_true!(ctx.group_exists(second));
        expect_consistent(&ctx);
    }

    #[gtest]
    fn groups_can_be_managed_while_executing() {
        let mut ctx = Context::new();
        let group = ctx.create_group();
        let (root, ()) = ctx.run_node(|ctx: &mut Context| {
            let root = ctx.current_action().unwrap();
            expect_that!(ctx.add_to_group(group, root), ok(eq(true)));
            let (child, ()) = ctx.run_node(|ctx: &mut Context| {
                let child = ctx.current_action().unwrap();
                expect_that!(ctx.add_to_group(group, child), ok(eq(true)));
            });
            ctx.remove_node(child).unwrap();
        });
        expect_that!(members(&ctx, group), { eq(&root) });
        expect_consistent(&ctx);
    }

    #[gtest]
    fn refused_removals_keep_memberships() {
        let mut ctx = Context::new();
        let group = ctx.create_group();
        ctx.run_node(|ctx: &mut Context| {
            let root = ctx.current_action().unwrap();
            ctx.add_to_group(group, root).unwrap();
            ctx.run_node(|ctx: &mut Context| {
                expect_that!(
                    ctx.remove_node(root),
                    err(eq(&RemoveNodeError::NodeExecuting(NodeExecuting(root))))
                );
                expect_that!(
                    ctx.clear_children(root),
                    err(eq(&ClearChildrenError::ExecutingDescendant(
                        ExecutingDescendant(root)
                    )))
                );
            });
            expect_that!(ctx.group_contains(group, root), ok(eq(true)));
        });
        expect_consistent(&ctx);
    }
}
