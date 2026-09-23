pub mod action;
pub mod action_tree;
pub mod context;
pub mod current;
pub mod grouping;
pub mod id;
pub mod lifecycle;

pub mod prelude {
    pub use crate::{
        action::{Action, ActionExt},
        action_tree::{ActionNodeKey, ActionTree, ActionTreeExt},
        context::Context,
        current::CurrentActionExt,
        grouping::{
            AddToGroupError, GroupId, GroupingExt, Groups, RemoveFromGroupError, UnknownGroup,
            UnknownNode,
        },
        lifecycle::{LifecycleExt, NodeObserver},
    };
}
