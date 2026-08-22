use std::{any::Any, cell::RefCell, fmt};

use bevy::prelude::Resource;
use slotmap::{SecondaryMap, SlotMap, new_key_type};
use smallvec::SmallVec;

use crate::layout::{PositioningNode, SizingNode};

new_key_type! {
    pub struct NodeKey;
}

#[derive(Debug, Resource, Default)]
pub struct UiTree {
    root: Option<NodeKey>,
    nodes: SlotMap<NodeKey, Node>,
    sizing: SecondaryMap<NodeKey, SizingNode>,
    position: SecondaryMap<NodeKey, PositioningNode>,
}

thread_local! {
    static CURR_TREE: RefCell<Option<Box<dyn Any>>> = RefCell::new(None);
    static ATTACHED_NODES: RefCell<SmallVec<[NodeKey;4]>> = RefCell::new(SmallVec::new());
}

#[derive(Debug, Default)]
pub struct Node {
    parent: Option<NodeKey>,
    children: smallvec::SmallVec<[NodeKey; 4]>,
}

impl Node {
    pub fn new() -> Self {
        Self::default()
    }
    fn with_children(mut self, children: SmallVec<[NodeKey; 4]>) -> Self {
        self.children = children;
        self
    }
    fn parent(&self) -> Option<NodeKey> {
        self.parent
    }
    fn child_keys(&self) -> &[NodeKey] {
        &self.children
    }
    fn child_count(&self) -> usize {
        self.children.len()
    }
    fn child_key(&self, index: usize) -> Option<NodeKey> {
        self.children.get(index).copied()
    }
    pub fn has_children(&self) -> bool {
        !self.children.is_empty()
    }
}

#[derive(Debug)]
#[must_use = "this node is detached; append it or intentionally discard it"]
pub struct DetachedNode {
    key: NodeKey,
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

impl CreateElement for Node {
    type Kind = Container;
    fn insert_element(self, tree: &mut UiTree) -> DetachedNode {
        let key = tree.add_node(self);
        DetachedNode { key }
    }
}

#[derive(Debug, Default)]
pub struct Item;

impl CreateElement for Item {
    type Kind = Leaf;
    fn insert_element(self, tree: &mut UiTree) -> DetachedNode {
        let key = tree.add_node(Node::new());
        DetachedNode { key }
    }
}

fn set_curr_tree(tree: UiTree) {
    CURR_TREE.with_borrow_mut(|current| {
        assert!(
            current.replace(Box::new(tree)).is_none(),
            "a tree is already being built"
        );
    });
}

fn take_curr_tree() -> UiTree {
    let tree = CURR_TREE
        .with_borrow_mut(|tree| tree.take())
        .expect("no tree is currently being built")
        .downcast()
        .expect("current tree has an unexpected type");
    *tree
}

fn with_curr_tree<R>(f: impl FnOnce(&mut UiTree) -> R) -> R {
    CURR_TREE.with_borrow_mut(|tree| {
        let tree = tree
            .as_mut()
            .expect("a UI tree must be being built")
            .downcast_mut::<UiTree>()
            .expect("current tree has an unexpected type");
        f(tree)
    })
}

pub struct Scope<F, Element>(F, Element)
where
    F: FnOnce(&mut Element),
    Element: CreateElement,
    Element::Kind: CanContainChildren<Element>;

impl<F, Element> Scope<F, Element>
where
    F: FnOnce(&mut Element),
    Element: CreateElement,
    Element::Kind: CanContainChildren<Element>,
{
    pub fn new_with(scope: F, element: Element) -> Self {
        Self(scope, element)
    }
    pub fn new(scope: F) -> Self
    where
        Element: Default,
    {
        Self(scope, Element::default())
    }
}

impl<F, Element> CreateElement for Scope<F, Element>
where
    F: FnOnce(&mut Element),
    Element: CreateElement,
    Element::Kind: CanContainChildren<Element>,
{
    type Kind = Leaf;
    fn insert_element(self, tree: &mut UiTree) -> DetachedNode {
        let mut group = self.1;
        let mut outer_nodes = ATTACHED_NODES.with_borrow_mut(std::mem::take);
        (self.0)(&mut group);
        ATTACHED_NODES.with_borrow_mut(|nodes| std::mem::swap(nodes, &mut outer_nodes));
        let children = outer_nodes;
        let node = group.insert_element(tree);
        for child in &children {
            if node.key == *child {
                panic!("cannot attach a group to itself");
            }
            tree.get_mut(*child)
                .expect("group contains an invalid child key")
                .parent = Some(node.key);
        }
        tree.get_mut(node.key)
            .expect("newly inserted group is missing")
            .children = children;
        node
    }
}

pub fn detached_node<Element: CreateElement>(element: Element) -> DetachedNode {
    with_curr_tree(|tree| element.insert_element(tree))
}
pub fn attach_node(node: DetachedNode) {
    ATTACHED_NODES.with_borrow_mut(|nodes| nodes.push(node.key));
}
pub fn node<Element: CreateElement>(element: Element) {
    attach_node(detached_node(element));
}

pub fn scope<Element, F>(scope: F) -> Scope<F, Element>
where
    Element: CreateElement + Default,
    Element::Kind: CanContainChildren<Element>,
    F: FnOnce(&mut Element),
{
    Scope::new(scope)
}

pub fn detached_node_scope<Element>(scope: impl FnOnce(&mut Element)) -> DetachedNode
where
    Element: CreateElement + Default,
    Element::Kind: CanContainChildren<Element>,
{
    detached_node(Scope(scope, Element::default()))
}

pub fn node_scope<Element>(scope: impl FnOnce(&mut Element))
where
    Element: CreateElement + Default,
    Element::Kind: CanContainChildren<Element>,
{
    node(Scope(scope, Element::default()))
}

pub fn scope_with<Element, F>(element: Element, scope: F) -> Scope<F, Element>
where
    Element: CreateElement,
    Element::Kind: CanContainChildren<Element>,
    F: FnOnce(&mut Element),
{
    Scope::new_with(scope, element)
}

pub fn detached_node_scope_with<Element>(
    element: Element,
    scope: impl FnOnce(&mut Element),
) -> DetachedNode
where
    Element: CreateElement,
    Element::Kind: CanContainChildren<Element>,
{
    detached_node(Scope(scope, element))
}

pub fn node_scope_with<Element>(element: Element, scope: impl FnOnce(&mut Element))
where
    Element: CreateElement,
    Element::Kind: CanContainChildren<Element>,
{
    node(Scope(scope, element))
}

pub fn build_ui_tree<Element, F>(element: Element, scope: F) -> UiTree
where
    Element: CreateElement,
    Element::Kind: CanContainChildren<Element>,
    F: FnOnce(&mut Element),
{
    set_curr_tree(UiTree::new());
    ATTACHED_NODES.with_borrow_mut(SmallVec::clear);
    let root = detached_node(scope_with(element, scope));
    let mut tree = take_curr_tree();
    tree.root = Some(root.key);
    tree
}

impl UiTree {
    pub fn new() -> Self {
        Self::default()
    }
    fn add_node(&mut self, node: Node) -> NodeKey {
        self.nodes.insert(node)
    }
    pub fn root(&self) -> Option<NodeKey> {
        self.root
    }
    pub fn root_node(&self) -> Option<&Node> {
        self.root.and_then(|key| self.get(key))
    }
    pub fn len(&self) -> usize {
        self.nodes.len()
    }
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
    fn nodes(&self) -> impl Iterator<Item = (NodeKey, &Node)> {
        self.nodes.iter()
    }
    fn get(&self, key: NodeKey) -> Option<&Node> {
        self.nodes.get(key)
    }
    fn get_mut(&mut self, key: NodeKey) -> Option<&mut Node> {
        self.nodes.get_mut(key)
    }
    fn get_disjoint_mut<const N: usize>(&mut self, keys: [NodeKey; N]) -> Option<[&mut Node; N]> {
        self.nodes.get_disjoint_mut(keys)
    }
}

impl fmt::Display for UiTree {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(root) = self.root {
            write_node(self, root, formatter, 0)?;
        }
        Ok(())
    }
}

fn write_node(
    tree: &UiTree,
    key: NodeKey,
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

    #[test]
    fn build_tree_creates_implicit_root() {
        let tree = build_ui_tree(Node::new(), |_| {
            node(Node::new());
        });
        assert_eq!(tree.len(), 2);
        assert_eq!(tree.to_string(), "*\n\t*\n");
    }

    #[test]
    fn group_is_detached_until_appended() {
        let tree = build_ui_tree(Node::new(), |_| {
            let child = detached_node_scope::<Node>(|_| {
                node(Node::new());
                node(Node::new());
            });
            attach_node(child);
        });
        assert_eq!(tree.to_string(), "*\n\t*\n\t\t*\n\t\t*\n");
    }

    #[test]
    fn append_group_preserves_nesting() {
        let tree = build_ui_tree(Node::new(), |_| {
            node(Node::new());
            node_scope::<Node>(|_| {
                node(Node::new());
                node_scope::<Node>(|_| {
                    node(Node::new());
                });
            });
            node(Node::new());
        });
        assert_eq!(
            tree.to_string(),
            "*\n\t*\n\t*\n\t\t*\n\t\t*\n\t\t\t*\n\t*\n"
        );
    }

    #[test]
    fn parent_relationships_are_created_by_group() {
        let tree = build_ui_tree(Node::new(), |_| {
            node_scope::<Node>(|_| {
                node(Node::new());
            });
        });
        let root = tree.root().unwrap();
        let branch = tree.get(root).unwrap().child_key(0).unwrap();
        let leaf = tree.get(branch).unwrap().child_key(0).unwrap();
        assert_eq!(tree.get(root).unwrap().parent(), None);
        assert_eq!(tree.get(branch).unwrap().parent(), Some(root));
        assert_eq!(tree.get(leaf).unwrap().parent(), Some(branch));
    }

    #[test]
    fn detached_nodes_are_allowed_to_remain_in_storage() {
        let tree = build_ui_tree(Node::new(), |_| {
            let _unused = detached_node(Node::new());
            node(Node::new());
        });
        assert_eq!(tree.len(), 3);
        assert_eq!(tree.to_string(), "*\n\t*\n");
    }

    #[test]
    fn detached_groups_are_allowed_to_remain_in_storage() {
        let tree = build_ui_tree(Node::new(), |_| {
            let _unused = detached_node_scope::<Node>(|_| {
                node(Node::new());
                node(Node::new());
            });
            node(Node::new());
        });
        assert_eq!(tree.len(), 5);
        assert_eq!(tree.to_string(), "*\n\t*\n");
    }
}
