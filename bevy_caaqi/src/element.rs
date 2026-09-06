pub mod item;
pub mod scope;

pub use item::Item;
pub use scope::Scope;

use bevy::prelude::Bundle;
use crate::context::CaaqiCtx;

/// Trait for types that can be converted into a Bevy bundle for a CaaqiNode entity.
/// Implementations should produce exactly one bundle per element instance.
pub trait IntoNodeBundle {
    fn into_node_bundle(self, ctx: &mut CaaqiCtx) -> impl Bundle;
}

/// Marker trait for element types that can contain children.
/// Used to enforce at compile-time that only container elements can have children in scopes.
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be used as a container",
    label = "this element kind does not allow children",
    note = "implement `CanHaveChildren` to allow this element to contain children"
)]
pub trait CanHaveChildren: IntoNodeBundle {}
