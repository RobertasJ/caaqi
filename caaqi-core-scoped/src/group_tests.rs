use super::*;
use crate::{
    context::{Context, scope},
    trace::{UnknownNode, create_node},
};

#[test]
fn groups_work_through_the_scope() {
    let mut ctx = Context::new();
    scope(&mut ctx, || {
        let node = create_node(|_: TraceNode| {});
        let group = create_group();
        assert!(group.exists());

        assert_eq!(group.insert(node), Ok(true));
        assert_eq!(group.contains(node), Ok(true));
        assert_eq!(group.members().unwrap().collect::<Vec<_>>(), [node]);
        assert_eq!(node.group_count(), 1);

        assert_eq!(group.remove(node), Ok(true));
        group.insert(node).unwrap();
        assert_eq!(node.remove_from_all_groups(), 1);
        assert_eq!(group.delete().map(|members| members.len()), Ok(0));
        assert!(!group.exists());
        assert_eq!(group.insert(node), Err(UnknownGroup(group.into()).into()));
    });
}

#[test]
fn a_deleted_member_is_refused() {
    let mut ctx = Context::new();
    scope(&mut ctx, || {
        let group = create_group();
        let node = create_node(|_: TraceNode| {});
        node.delete().unwrap();
        assert_eq!(group.insert(node), Err(UnknownNode(node.into()).into()));
    });
}

#[test]
#[should_panic(expected = "was deleted")]
fn group_count_panics_for_a_deleted_node() {
    let mut ctx = Context::new();
    scope(&mut ctx, || {
        let node = create_node(|_: TraceNode| {});
        node.delete().unwrap();
        node.group_count();
    });
}

#[test]
fn groups_convert_to_and_from_group_ids() {
    let mut ctx = Context::new();
    scope(&mut ctx, || {
        let group = create_group();
        let id = GroupId::from(group);
        assert_eq!(Group::from(id), group);
        assert!(Group::from(id).exists());
    });
}
