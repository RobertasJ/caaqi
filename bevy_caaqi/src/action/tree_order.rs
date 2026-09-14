use bevy::prelude::*;

use crate::action::node::TreeNode;

pub struct TreeOrder {
    nodes: Vec<Entity>,
}

impl TreeOrder {
    pub fn new(world: &World, root: Entity) -> Self {
        let mut nodes = Vec::new();
        let mut stack = vec![root];

        while let Some(node) = stack.pop() {
            nodes.push(node);

            if let Some(children) = world.get::<Children>(node) {
                // Push backwards so the first child is visited first.
                for child in children.iter().rev() {
                    if world.get::<TreeNode>(child).is_some() {
                        stack.push(child);
                    }
                }
            }
        }

        Self { nodes }
    }

    pub fn forward(&self) -> impl Iterator<Item = Entity> + '_ {
        self.nodes.iter().copied()
    }

    pub fn backward(&self) -> impl Iterator<Item = Entity> + '_ {
        self.nodes.iter().rev().copied()
    }

    pub fn after(&self, node: Entity) -> impl Iterator<Item = Entity> + '_ {
        let index = self
            .nodes
            .iter()
            .position(|entity| *entity == node)
            .expect("node is not part of this tree");

        self.nodes[index + 1..].iter().copied()
    }
}
