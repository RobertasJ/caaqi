pub mod context_builder;
pub mod item;
pub mod node;

use bevy::ecs::{
    entity::{self, Entity},
    hierarchy::ChildOf,
};
pub use item::Item;
use smallvec::SmallVec;

use crate::{
    context_tree_builder::DetachedNode, element::context_builder::ElementScope,
    world_context::WorldContext,
};

/// Marker trait for element types that can be created as UI nodes.
pub trait Element: Send + Sync + 'static {
    /// Convert the element into a UI node bundle.
    fn into_ui_node(self, ctx: &mut WorldContext) -> DetachedNode<ElementScope>;
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
        element: DetachedNode<ElementScope>,
        ctx: &mut WorldContext,
        children: SmallVec<[DetachedNode<ElementScope>; 1]>,
    ) -> DetachedNode<ElementScope> {
        let mut entity_mut = ctx.entity_mut(*element);

        for child in children {
            entity_mut.add_child(*child);
        }

        element
    }
}
