pub mod item;
pub mod scope;

pub use item::Item;
pub use scope::Scope;

pub struct Leaf;
pub struct Container;

use bevy::prelude::Bundle;
use crate::context::{CaaqiCtx, DetachedNode};

/// Trait for types that can be converted into a Bevy bundle for a CaaqiNode entity.
/// Implementations should produce exactly one bundle per element instance.
pub trait IntoNodeBundle {
    fn into_node_bundle(self, ctx: &mut CaaqiCtx) -> impl Bundle;
}

/// Legacy trait maintained for backward compatibility during migration.
/// Use `IntoNodeBundle` for new code.
pub trait CreateElement {
    type Kind;
    fn insert_element(self, ctx: &mut CaaqiCtx) -> DetachedNode;
}

#[diagnostic::on_unimplemented(
    message = "`{Element}` cannot be used as a container",
    label = "this element kind does not allow children",
    note = "expected an element of kind `Container`"
)]
pub trait CanContainChildren<Element> {}

impl<Element> CanContainChildren<Element> for Container {}
