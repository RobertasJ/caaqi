use googletest::prelude::*;

use super::*;
use crate::trace::{DeleteError, RunnerInUse};

/// A runner for nodes whose runs don't matter.
fn noop(_: &mut Context, _: NodeKey) {}

fn node(ctx: &mut Context) -> NodeKey {
    ctx.create_node(noop).id()
}

fn delete(ctx: &mut Context, node: NodeKey) {
    ctx.node_mut(node).unwrap().delete().unwrap();
}

fn members(ctx: &Context, group: GroupId) -> HashSet<NodeKey> {
    ctx.group_members(group).unwrap().collect()
}

fn groups_of(ctx: &Context, node: NodeKey) -> HashSet<GroupId> {
    ctx.groups_of(node).unwrap().collect()
}

/// Checks that `members` and `memberships` mirror each other exactly for
/// nodes in the trace.
fn expect_consistent(ctx: &Context) {
    let Some(groups) = ctx.get::<Groups>() else {
        return;
    };
    for (group, members) in &groups.members {
        for &node in members.iter().filter(|&&node| ctx.contains_node(node)) {
            expect_true!(
                groups.groups_of(node).any(|member_of| member_of == group),
                "{node:?} records its membership of {group:?}",
            );
        }
    }
    for (node, member_of) in &groups.memberships {
        expect_that!(member_of, not(is_empty()), "empty memberships are removed");
        if !ctx.contains_node(node) {
            continue;
        }
        for &group in member_of {
            expect_that!(
                groups.group(group).map(|members| members.contains(&node)),
                ok(eq(true)),
                "{node:?} is in {group:?}"
            );
        }
    }
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
fn reads_reject_unknown_groups() {
    let mut ctx = Context::new();
    let node = node(&mut ctx);
    let removed = ctx.create_group();
    ctx.remove_group(removed).unwrap();

    expect_that!(
        ctx.group_contains(removed, node),
        err(eq(GroupContainsError::UnknownGroup(UnknownGroup(removed))))
    );
    expect_that!(
        ctx.group_members(removed).err(),
        some(eq(UnknownGroup(removed)))
    );
}

#[gtest]
fn add_records_both_directions() {
    let mut ctx = Context::new();
    let group = ctx.create_group();
    let node = node(&mut ctx);

    expect_that!(ctx.add_to_group(group, node), ok(eq(true)));
    expect_that!(members(&ctx, group), { eq(&node) });
    expect_that!(groups_of(&ctx, node), { eq(&group) });
    expect_that!(ctx.group_contains(group, node), ok(eq(true)));
    expect_consistent(&ctx);
}

#[gtest]
fn adding_twice_is_rejected() {
    let mut ctx = Context::new();
    let group = ctx.create_group();
    let node = node(&mut ctx);
    ctx.add_to_group(group, node).unwrap();

    expect_that!(ctx.add_to_group(group, node), ok(eq(false)));
    expect_that!(groups_of(&ctx, node), { eq(&group) });
    expect_consistent(&ctx);
}

#[gtest]
fn add_rejects_unknown_groups() {
    let mut ctx = Context::new();
    let node = node(&mut ctx);
    let removed = ctx.create_group();
    ctx.remove_group(removed).unwrap();
    expect_that!(
        ctx.add_to_group(removed, node),
        err(eq(AddToGroupError::UnknownGroup(UnknownGroup(removed))))
    );

    expect_that!(groups_of(&ctx, node), is_empty());
}

#[gtest]
fn add_rejects_deleted_nodes() {
    let mut ctx = Context::new();
    let group = ctx.create_group();
    let node = node(&mut ctx);
    delete(&mut ctx, node);
    expect_that!(
        ctx.add_to_group(group, node),
        err(eq(AddToGroupError::UnknownNode(UnknownNode(node))))
    );

    expect_that!(members(&ctx, group), is_empty());
}

#[gtest]
fn removing_a_non_member_changes_nothing() {
    let mut ctx = Context::new();
    let group = ctx.create_group();
    let node = node(&mut ctx);
    expect_that!(ctx.remove_from_group(group, node), ok(eq(false)));

    ctx.add_to_group(group, node).unwrap();
    ctx.remove_from_group(group, node).unwrap();
    expect_that!(ctx.remove_from_group(group, node), ok(eq(false)));
}

#[gtest]
fn remove_from_group_rejects_unknown_groups() {
    let mut ctx = Context::new();
    let group = ctx.create_group();
    let node = node(&mut ctx);
    let removed_group = ctx.create_group();
    ctx.remove_group(removed_group).unwrap();
    expect_that!(
        ctx.remove_from_group(removed_group, node),
        err(eq(UnknownGroup(removed_group)))
    );
    expect_that!(ctx.remove_from_group(group, node), ok(eq(false)));
}

#[gtest]
fn nodes_can_rejoin_a_group() {
    let mut ctx = Context::new();
    let group = ctx.create_group();
    let node = node(&mut ctx);
    ctx.add_to_group(group, node).unwrap();
    ctx.remove_from_group(group, node).unwrap();

    expect_that!(ctx.add_to_group(group, node), ok(eq(true)));
    expect_that!(groups_of(&ctx, node), { eq(&group) });
    expect_consistent(&ctx);
}

#[gtest]
fn removing_an_unknown_group_changes_nothing() {
    let mut ctx = Context::new();
    let group = ctx.create_group();
    let node = node(&mut ctx);
    ctx.add_to_group(group, node).unwrap();
    let removed = ctx.create_group();
    ctx.remove_group(removed).unwrap();

    expect_that!(ctx.remove_group(removed), err(eq(&UnknownGroup(removed))));
    expect_that!(members(&ctx, group), { eq(&node) });
    expect_consistent(&ctx);
}

#[gtest]
fn reused_slots_start_without_groups() {
    let mut ctx = Context::new();
    let group = ctx.create_group();
    let old = node(&mut ctx);
    ctx.add_to_group(group, old).unwrap();
    delete(&mut ctx, old);

    let new = node(&mut ctx);
    expect_ne!(old, new);
    expect_that!(groups_of(&ctx, new), is_empty());
    expect_that!(ctx.group_contains(group, new), ok(eq(false)));
    expect_that!(members(&ctx, group), { eq(&old) });

    // Joining with the reused slot replaces the old key's entry, and the
    // old key can still be removed.
    ctx.add_to_group(group, new).unwrap();
    expect_that!(members(&ctx, group), {eq(&old), eq(&new)});
    expect_that!(ctx.remove_from_group(group, old), ok(eq(true)));
    expect_that!(members(&ctx, group), { eq(&new) });
    expect_that!(groups_of(&ctx, new), { eq(&group) });
    expect_consistent(&ctx);
}

#[gtest]
fn remove_from_group_clears_both_directions() {
    let mut ctx = Context::new();
    let kept = ctx.create_group();
    let left = ctx.create_group();
    let node = node(&mut ctx);
    ctx.add_to_group(kept, node).unwrap();
    ctx.add_to_group(left, node).unwrap();

    expect_that!(ctx.remove_from_group(left, node), ok(eq(true)));
    expect_that!(members(&ctx, left), is_empty());
    expect_that!(groups_of(&ctx, node), { eq(&kept) });

    expect_that!(ctx.remove_from_group(kept, node), ok(eq(true)));
    expect_that!(groups_of(&ctx, node), is_empty());
    expect_consistent(&ctx);
}

#[gtest]
#[should_panic(expected = "memberships mirror members")]
fn remove_membership_panics_when_directions_diverge() {
    let mut ctx = Context::new();
    let group = ctx.create_group();
    let node = node(&mut ctx);
    ctx.add_to_group(group, node).unwrap();
    ctx.get_mut::<Groups>().unwrap().memberships.remove(node);

    ctx.remove_from_group(group, node).unwrap();
}

#[gtest]
fn remove_group_returns_members_and_clears_memberships() {
    let mut ctx = Context::new();
    let removed = ctx.create_group();
    let kept = ctx.create_group();
    let first = node(&mut ctx);
    let second = node(&mut ctx);
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
fn remove_group_returns_stale_keys() {
    let mut ctx = Context::new();
    let group = ctx.create_group();
    let deleted = node(&mut ctx);
    let kept = node(&mut ctx);
    ctx.add_to_group(group, deleted).unwrap();
    ctx.add_to_group(group, kept).unwrap();
    delete(&mut ctx, deleted);

    expect_that!(
        ctx.remove_group(group),
        ok(unordered_elements_are![eq(&deleted), eq(&kept)])
    );
    let groups = ctx.get::<Groups>().unwrap();
    expect_that!(groups.memberships.get(deleted), none());
    expect_that!(groups.memberships.get(kept), none());
    expect_consistent(&ctx);
}

#[gtest]
fn deleted_members_stay_until_removed() {
    let mut ctx = Context::new();
    let group = ctx.create_group();
    let deleted = node(&mut ctx);
    let kept = node(&mut ctx);
    ctx.add_to_group(group, deleted).unwrap();
    ctx.add_to_group(group, kept).unwrap();
    delete(&mut ctx, deleted);

    expect_that!(members(&ctx, group), {eq(&deleted), eq(&kept)});
    expect_that!(
        ctx.group_contains(group, deleted),
        err(eq(GroupContainsError::UnknownNode(UnknownNode(deleted))))
    );
    expect_that!(ctx.groups_of(deleted).err(), some(eq(UnknownNode(deleted))));

    expect_that!(ctx.remove_from_group(group, deleted), ok(eq(true)));
    expect_that!(members(&ctx, group), { eq(&kept) });
    expect_that!(
        ctx.get::<Groups>().unwrap().memberships.get(deleted),
        none()
    );
    expect_that!(ctx.remove_from_group(group, deleted), ok(eq(false)));
    expect_consistent(&ctx);
}

#[gtest]
fn groups_can_be_managed_while_running() {
    let mut ctx = Context::new();
    let group = ctx.create_group();
    let root = ctx
        .create_node(move |ctx: &mut Context, root: NodeKey| {
            expect_that!(ctx.add_to_group(group, root), ok(eq(true)));
            let child = ctx
                .create_node(move |ctx: &mut Context, child: NodeKey| {
                    expect_that!(ctx.add_to_group(group, child), ok(eq(true)));
                })
                .id();
            ctx.node_mut(child).unwrap().run().unwrap();
            expect_that!(members(ctx, group), {eq(&root), eq(&child)});
            expect_that!(ctx.remove_from_group(group, child), ok(eq(true)));
            delete(ctx, child);
        })
        .id();
    ctx.node_mut(root).unwrap().run().unwrap();

    expect_that!(members(&ctx, group), { eq(&root) });
    expect_consistent(&ctx);
}

#[gtest]
fn refused_deletes_keep_memberships() {
    let mut ctx = Context::new();
    let group = ctx.create_group();
    let root = ctx
        .create_node(move |ctx: &mut Context, root: NodeKey| {
            ctx.add_to_group(group, root).unwrap();
            expect_that!(
                ctx.node_mut(root).unwrap().delete(),
                err(eq(DeleteError::RunnerInUse(RunnerInUse(root))))
            );
            expect_that!(ctx.group_contains(group, root), ok(eq(true)));
        })
        .id();
    ctx.node_mut(root).unwrap().run().unwrap();

    expect_that!(members(&ctx, group), { eq(&root) });
    expect_consistent(&ctx);
}
