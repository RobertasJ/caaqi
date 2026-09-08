pub mod context_builder;
pub mod item;
pub mod node;

use bevy::ecs::{entity::Entity, hierarchy::ChildOf};
pub use item::Item;
use smallvec::SmallVec;

use crate::{context_tree_builder::DetachedNode, world_context::WorldContext};

/// Marker trait for element types that can be created as UI nodes.
pub trait Element: Send + Sync + 'static {
    /// Convert the element into a UI node bundle.
    fn into_ui_node(self, ctx: &mut WorldContext) -> DetachedNode;
}

pub trait ElementMutator: From<Entity> {}

/// Marker trait for element types that can contain children.
/// Used to enforce at compile-time that only container elements can have children in scopes.
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be used as a container",
    label = "this element kind does not allow children",
    note = "implement `CanHaveChildren` to allow this element to contain children"
)]
pub trait CanHaveChildren: Element {
    fn add_children(
        element: DetachedNode,
        ctx: &mut WorldContext,
        children: SmallVec<[DetachedNode; 1]>,
    ) -> DetachedNode {
        for child in children {
            ctx.entity_mut(*child).insert(ChildOf(*element));
        }

        element
    }
}
