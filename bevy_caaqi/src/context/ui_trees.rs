mod layout;

use std::{fmt, marker::PhantomData, ptr::NonNull};

use bevy::prelude::Deref;
use slotmap::{SecondaryMap, SlotMap, new_key_type};
use smallvec::SmallVec;

use super::CTX;
use layout::{Positioning, Sizing};

/// With this value you can access from the slotmap,
/// knowing you wont be missing the node and that it wont be already borrowed
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DetachedNode(NodeId);

impl DetachedNode {
    fn inner(&self) -> NodeId {
        self.0
    }
}

new_key_type! {
    pub struct NodeId;
}

#[derive(Debug, Default)]
pub struct UiTree {
    pub(crate) roots: Vec<NodeId>,
    nodes: SlotMap<NodeId, Node>,
    sizing: SecondaryMap<NodeId, Sizing>,
    positioning: SecondaryMap<NodeId, Positioning>,
    attached_nodes: SmallVec<[NodeId; 1]>,
}

impl UiTree {
    pub fn new() -> Self {
        Self::default()
    }

    fn len(&self) -> usize {
        self.nodes.len()
    }

    fn add_node(&mut self, node: Node) -> DetachedNode {
        DetachedNode(self.nodes.insert(node))
    }

    fn get_node(&self, key: NodeId) -> Option<&Node> {
        self.nodes.get(key)
    }

    fn get_node_mut(&mut self, key: NodeId) -> Option<&mut Node> {
        self.nodes.get_mut(key)
    }

    fn get_sizing(&self, key: NodeId) -> Option<&Sizing> {
        self.sizing.get(key)
    }

    fn get_sizing_mut(&mut self, key: NodeId) -> Option<&mut Sizing> {
        self.sizing.get_mut(key)
    }

    fn get_positioning(&self, key: NodeId) -> Option<&Positioning> {
        self.positioning.get(key)
    }

    fn get_positioning_mut(&mut self, key: NodeId) -> Option<&mut Positioning> {
        self.positioning.get_mut(key)
    }

    fn traverse_bottom_up_sizing_mut<O>(
        &mut self,
        root: NodeId,
        for_each: impl Fn(&mut Sizing, &mut dyn Iterator<Item = O>) -> O,
    ) -> O {
        unsafe fn inner<O>(
            tree: *mut UiTree,
            key: NodeId,
            for_each: &impl Fn(&mut Sizing, &mut dyn Iterator<Item = O>) -> O,
        ) -> O {
            unsafe {
                let node = (*tree).nodes.get_unchecked(key);

                // We need the children independently of the borrow of `node`
                // while recursively accessing the tree.
                let children = node.children.as_slice() as *const [NodeId];

                let sizing = (*tree).sizing.entry(key).unwrap().or_default() as *mut Sizing;

                let mut children = (&*children)
                    .iter()
                    .copied()
                    .map(|child| inner(tree, child, for_each));

                for_each(&mut *sizing, &mut children)
            }
        }

        unsafe { inner(self as *mut Self, root, &for_each) }
    }
}

#[derive(Debug, Default)]
pub struct Node {
    parent: Option<NodeId>,
    children: smallvec::SmallVec<[NodeId; 1]>,
}

impl Node {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn has_children(&self) -> bool {
        !self.children.is_empty()
    }
}

pub struct Leaf;
pub struct Container;

pub trait CreateElement {
    type Kind;
    fn insert_element(self, tree: &mut UiTree) -> DetachedNode;
}

#[diagnostic::on_unimplemented(
    message = "`{Element}` cannot be used as a container",
    label = "this element kind does not allow children",
    note = "expected an element of kind `Container`"
)]
pub trait CanContainChildren<Element> {}

impl<Element> CanContainChildren<Element> for Container {}

#[derive(Debug, Default)]
pub struct Item;

impl CreateElement for Item {
    type Kind = Leaf;
    fn insert_element(self, tree: &mut UiTree) -> DetachedNode {
        tree.add_node(Node::new())
    }
}

#[derive(Debug, Default)]
pub struct Group;

impl CreateElement for Group {
    type Kind = Container;

    fn insert_element(self, tree: &mut UiTree) -> DetachedNode {
        tree.add_node(Node::new())
    }
}

pub struct Scope<Element>
where
    Element: CreateElement,
    Element::Kind: CanContainChildren<Element>,
{
    container: Element,
    children: SmallVec<[NodeId; 1]>,
}

impl<Element> Scope<Element>
where
    Element: CreateElement,
    Element::Kind: CanContainChildren<Element>,
{
    pub fn new_with<F>(mut element: Element, scope: F) -> Self
    where
        F: FnOnce(&mut Element),
    {
        let mut children = with_curr_tree(|tree| std::mem::take(&mut tree.attached_nodes));

        scope(&mut element);

        with_curr_tree(|tree| std::mem::swap(&mut tree.attached_nodes, &mut children));

        Self {
            children,
            container: element,
        }
    }
    pub fn new<F>(scope: F) -> Self
    where
        Element: Default,
        F: FnOnce(&mut Element),
    {
        Self::new_with(Element::default(), scope)
    }
}

impl<Element> CreateElement for Scope<Element>
where
    Element: CreateElement,
    Element::Kind: CanContainChildren<Element>,
{
    type Kind = Leaf;
    fn insert_element(self, tree: &mut UiTree) -> DetachedNode {
        let group = self.container;
        let node = group.insert_element(tree);

        let children = self.children;

        for child in &children {
            if node.inner() == *child {
                panic!("cannot attach a group to itself");
            }
            tree.get_node_mut(*child)
                .expect("group contains an invalid child key")
                .parent = Some(node.inner());
        }
        tree.get_node_mut(node.inner())
            .expect("newly inserted group is missing")
            .children = children;
        node
    }
}

fn with_curr_tree<R>(f: impl FnOnce(&mut UiTree) -> R) -> R {
    CTX.with(|ctx| f(&mut ctx.trees))
}

pub fn detached_node<Element: CreateElement>(element: Element) -> DetachedNode {
    with_curr_tree(|tree| element.insert_element(tree))
}

pub fn attach_node(node: DetachedNode) {
    CTX.with(|ctx| ctx.trees.attached_nodes.push(node.inner()));
}

pub fn node<Element: CreateElement>(element: Element) {
    attach_node(detached_node(element));
}

pub fn scope<Element>(scope: impl FnOnce(&mut Element)) -> Scope<Element>
where
    Element: CreateElement + Default,
    Element::Kind: CanContainChildren<Element>,
{
    Scope::new(scope)
}

pub fn detached_node_scope<Element>(scope: impl FnOnce(&mut Element)) -> DetachedNode
where
    Element: CreateElement + Default,
    Element::Kind: CanContainChildren<Element>,
{
    detached_node(Scope::new(scope))
}

pub fn node_scope<Element>(scope: impl FnOnce(&mut Element))
where
    Element: CreateElement + Default,
    Element::Kind: CanContainChildren<Element>,
{
    node(Scope::new(scope))
}

pub fn scope_with<Element>(element: Element, scope: impl FnOnce(&mut Element)) -> Scope<Element>
where
    Element: CreateElement,
    Element::Kind: CanContainChildren<Element>,
{
    Scope::new_with(element, scope)
}

pub fn detached_node_scope_with<Element>(
    element: Element,
    scope: impl FnOnce(&mut Element),
) -> DetachedNode
where
    Element: CreateElement,
    Element::Kind: CanContainChildren<Element>,
{
    detached_node(Scope::new_with(element, scope))
}

pub fn node_scope_with<Element>(element: Element, scope: impl FnOnce(&mut Element))
where
    Element: CreateElement,
    Element::Kind: CanContainChildren<Element>,
{
    node(Scope::new_with(element, scope))
}

impl fmt::Display for UiTree {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for root in &self.roots {
            write_node(self, *root, formatter, 0)?;
        }
        Ok(())
    }
}

fn write_node(
    tree: &UiTree,
    key: NodeId,
    formatter: &mut fmt::Formatter<'_>,
    depth: usize,
) -> fmt::Result {
    let Some(node) = tree.nodes.get(key) else {
        return Ok(());
    };
    for _ in 0..depth {
        formatter.write_str("\t")?;
    }
    writeln!(formatter, "*")?;
    for &child in &node.children {
        write_node(tree, child, formatter, depth + 1)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{CaaqiCtx, with_caaqi_ctx};

    #[test]
    fn root_with_no_children() {
        let mut ctx = CaaqiCtx::new();
        with_caaqi_ctx(&mut ctx, || {
            let root = detached_node(Group);

            with_curr_tree(|tree| {
                tree.roots.push(root.inner());

                assert_eq!(tree.len(), 1);
                assert_eq!(tree.to_string(), "*\n");
            });
        });
    }

    #[test]
    fn root_with_scope() {
        let mut ctx = CaaqiCtx::new();
        with_caaqi_ctx(&mut ctx, || {
            let root = detached_node_scope::<Group>(|_| {
                node(Group);
            });

            with_curr_tree(|tree| {
                tree.roots.push(root.inner());

                assert_eq!(tree.len(), 2);
                assert_eq!(tree.to_string(), "*\n\t*\n");
            });
        });
    }

    #[test]
    fn for_loop() {
        let mut ctx = CaaqiCtx::new();
        with_caaqi_ctx(&mut ctx, || {
            let root = detached_node_scope::<Group>(|_| {
                for _ in 0..5 {
                    node(Group);
                }
            });

            with_curr_tree(|tree| {
                tree.roots.push(root.inner());

                assert_eq!(tree.len(), 6);
                assert_eq!(tree.to_string(), "*\n\t*\n\t*\n\t*\n\t*\n\t*\n");
            });
        });
    }

    #[test]
    fn detached_nodes() {
        let mut ctx = CaaqiCtx::new();
        with_caaqi_ctx(&mut ctx, || {
            let root = detached_node_scope::<Group>(|_| {
                let create_detached = || {
                    detached_node_scope::<Group>(|_| {
                        node(Group);
                        for _ in 0..3 {
                            node_scope::<Group>(|_| {
                                node(Group);
                            });
                        }
                    })
                };

                for _ in 0..5 {
                    attach_node(create_detached());
                }
            });

            with_curr_tree(|tree| {
                tree.roots.push(root.inner());

                assert_eq!(tree.len(), 41);
                assert_eq!(
                    tree.to_string(),
                    "*\n\t*\n\t\t*\n\t\t*\n\t\t\t*\n\t\t*\n\t\t\t*\n\t\t*\n\t\t\t*\n\t*\n\t\t*\n\t\t*\n\t\t\t*\n\t\t*\n\t\t\t*\n\t\t*\n\t\t\t*\n\t*\n\t\t*\n\t\t*\n\t\t\t*\n\t\t*\n\t\t\t*\n\t\t*\n\t\t\t*\n\t*\n\t\t*\n\t\t*\n\t\t\t*\n\t\t*\n\t\t\t*\n\t\t*\n\t\t\t*\n\t*\n\t\t*\n\t\t*\n\t\t\t*\n\t\t*\n\t\t\t*\n\t\t*\n\t\t\t*\n"
                );
            });
        });
    }
}
