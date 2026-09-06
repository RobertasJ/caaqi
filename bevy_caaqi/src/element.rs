pub mod item;
pub mod scope;

pub use item::Item;
pub use scope::Scope;

use crate::context::{CaaqiCtx, DetachedNode};

/// Marker trait for element types that can be created as UI nodes.
pub trait IntoUiNode {
    /// Convert the element into a UI node bundle.
    fn into_ui_node(self, ctx: &mut CaaqiCtx) -> DetachedNode;
}

/// Marker trait for element types that can contain children.
/// Used to enforce at compile-time that only container elements can have children in scopes.
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be used as a container",
    label = "this element kind does not allow children",
    note = "implement `CanHaveChildren` to allow this element to contain children"
)]
pub trait CanHaveChildren: IntoUiNode {}
