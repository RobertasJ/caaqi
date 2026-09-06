pub mod item;
pub mod scope;

pub use item::Item;
pub use scope::Scope;

pub struct Leaf;
pub struct Container;

use crate::context::{CaaqiCtx, DetachedNode};

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
