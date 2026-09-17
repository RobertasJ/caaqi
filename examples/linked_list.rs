use bevy::{
    MinimalPlugins,
    app::{App, Startup},
    ecs::system::Commands,
    log::LogPlugin,
};
use bevy_caaqi::{CaaqiPlugin, prelude::*, value::ArcBorrow};
use slotmap::{DefaultKey, SecondaryMap, SlotMap};

fn main() {
    App::new()
        .add_plugins((MinimalPlugins, CaaqiPlugin))
        .add_systems(Startup, queue_eval)
        .run();
}

fn queue_eval(commands: Commands) {
    defer_action_eval(commands, || {});
}

struct VarList<T: Send + Sync + 'static> {
    tracking_key: SyncKey,
    nodes: Value<SlotMap<DefaultKey, LinkedNode<T>>>,
    head: Option<DefaultKey>,
    tail: Option<DefaultKey>,
}

impl<T: Send + Sync + 'static> Clone for VarList<T> {
    fn clone(&self) -> Self {
        Self {
            tracking_key: self.tracking_key,
            nodes: self.nodes,
            head: self.head,
            tail: self.tail,
        }
    }
}

impl<T: Send + Sync + 'static> Copy for VarList<T> {}

struct LinkedNode<T: Send + Sync + 'static> {
    prev: Option<DefaultKey>,
    next: Option<DefaultKey>,
    value: T,
}

fn var_list<T: Send + Sync + 'static>() -> VarList<T> {
    let list = VarList {
        tracking_key: sync_key(),
        nodes: value(SlotMap::with_key()),
        head: None,
        tail: None,
    };
    list
}

struct VarListIter<T: Clone + Send + Sync + 'static> {
    nodes: ArcBorrow<SlotMap<DefaultKey, LinkedNode<T>>>,
    curr: Option<DefaultKey>,
}

impl<T: Clone + Send + Sync + 'static> Iterator for VarListIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(curr) = self.curr {
            let node = self.nodes.get(curr).unwrap();
            self.curr = node.next;
            Some(node.value.clone())
        } else {
            None
        }
    }
}

impl<T: Send + Sync + 'static> VarList<T> {
    fn iter(&self) -> VarListIter<T>
    where
        T: Clone,
    {
        let head = self.head;

        VarListIter {
            nodes: self.nodes.read(),
            curr: self.head,
        }
    }
}
