use std::{
    cell::Cell,
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};

use googletest::prelude::*;

use super::*;

/// What nodes and rewinds did, in the order they did it.
#[derive(Default)]
struct Log(Vec<String>);

fn log(ctx: &mut Context, entry: impl ToString) {
    ctx.get_or_insert_with(Log::default)
        .0
        .push(entry.to_string());
}

fn logged(ctx: &Context) -> Vec<String> {
    ctx.get::<Log>().map_or_else(Vec::new, |log| log.0.clone())
}

/// Every node key a test created.
#[derive(Default)]
struct Created(Vec<NodeKey>);

fn created(ctx: &mut Context, node: NodeKey) {
    ctx.get_or_insert_with(Created::default).0.push(node);
}

/// How many of the nodes a test created are still in the trace.
fn live(ctx: &Context) -> usize {
    ctx.get::<Created>().map_or(0, |created| {
        created
            .0
            .iter()
            .filter(|&&node| ctx.contains_node(node))
            .count()
    })
}

#[gtest]
fn child_is_placed_under_the_current_node_and_run() {
    let mut ctx = Context::new();
    let child = Rc::new(Cell::new(None));
    let root = ctx.root({
        let child = child.clone();
        move |ctx| {
            log(ctx, "root");
            child.set(Some(ctx.child(|ctx| log(ctx, "child"))));
        }
    });
    let child = child.get().unwrap();

    expect_eq!(logged(&ctx), ["root", "child"]);
    expect_that!(ctx.node(child).unwrap().parent(), some(eq(root)));
    expect_eq!(ctx.node(root).unwrap().children(), &[child]);
}

#[gtest]
fn child_outside_of_a_running_node_is_a_root() {
    let mut ctx = Context::new();
    let node = ctx.child(|ctx| log(ctx, "ran"));

    expect_eq!(logged(&ctx), ["ran"]);
    expect_that!(ctx.node(node).unwrap().parent(), none());
}

#[gtest]
fn rerunning_the_parent_replaces_its_children() {
    let mut ctx = Context::new();
    let root = ctx.root(|ctx| {
        for _ in 0..3 {
            let child = ctx.child(|_| {});
            created(ctx, child);
        }
    });
    created(&mut ctx, root);
    expect_eq!(live(&ctx), 4);

    for _ in 0..10 {
        let old = ctx.node(root).unwrap().children().to_vec();
        ctx.rerun(root).unwrap();

        expect_eq!(live(&ctx), 4);
        let new = ctx.node(root).unwrap().children().to_vec();
        expect_that!(new.len(), eq(3));
        for child in old {
            expect_false!(ctx.contains_node(child));
            expect_false!(new.contains(&child));
        }
    }
}

#[gtest]
fn nested_children_are_cleaned_up_by_rewinding_the_top() {
    let mut ctx = Context::new();
    let root = ctx.root(|ctx| {
        let a = ctx.child(|ctx| {
            let b = ctx.child(|ctx| {
                let c = ctx.child(|ctx| ctx.on_rewind(|ctx| log(ctx, "c rewound")));
                created(ctx, c);
            });
            created(ctx, b);
        });
        created(ctx, a);
    });

    expect_eq!(live(&ctx), 3);
    ctx.rewind(root).unwrap();

    expect_eq!(live(&ctx), 0);
    expect_that!(ctx.node(root).unwrap().children(), is_empty());
    expect_eq!(logged(&ctx), ["c rewound"]);
    expect_that!(ctx.rewind_keys(root).unwrap(), is_empty());
}

#[gtest]
fn current_is_the_running_node() {
    /// What `current` was at each point.
    #[derive(Default)]
    struct Seen {
        root: Option<NodeKey>,
        in_child: Option<NodeKey>,
        child: Option<NodeKey>,
        after_child: Option<NodeKey>,
        after_panic: Option<NodeKey>,
    }

    let mut ctx = Context::new();
    let root = ctx.root(|ctx| {
        let root = ctx.current();
        let child = ctx.child(|ctx| {
            let in_child = ctx.current();
            ctx.get_or_insert_with(Seen::default).in_child = Some(in_child);
        });
        let after_child = ctx.current();
        let panicked = catch_unwind(AssertUnwindSafe(|| {
            ctx.child(|_| panic!("the child panics"));
        }));
        assert!(panicked.is_err());
        let after_panic = ctx.current();

        let seen = ctx.get_or_insert_with(Seen::default);
        seen.root = Some(root);
        seen.child = Some(child);
        seen.after_child = Some(after_child);
        seen.after_panic = Some(after_panic);
    });

    let seen = ctx.get::<Seen>().unwrap();
    expect_that!(seen.root, some(eq(root)));
    expect_eq!(seen.in_child, seen.child);
    expect_that!(seen.after_child, some(eq(root)));
    expect_that!(seen.after_panic, some(eq(root)));
    expect_that!(ctx.try_current(), none());
}

#[gtest]
fn a_panicking_child_is_cleaned_up_with_its_parent() {
    let mut ctx = Context::new();
    let root = ctx.root(|ctx| {
        let _ = catch_unwind(AssertUnwindSafe(|| {
            ctx.child(|_| panic!("the child panics"));
        }));
    });
    expect_that!(ctx.node(root).unwrap().children().len(), eq(1));

    ctx.rewind(root).unwrap();
    expect_that!(ctx.node(root).unwrap().children(), is_empty());
}

#[gtest]
fn on_rewind_runs_when_the_node_is_rewound() {
    let mut ctx = Context::new();
    let root = ctx.root(|ctx| {
        log(ctx, "ran");
        ctx.on_rewind(|ctx| log(ctx, "rewound"));
    });
    expect_eq!(logged(&ctx), ["ran"]);

    ctx.rewind(root).unwrap();
    expect_eq!(logged(&ctx), ["ran", "rewound"]);

    ctx.rerun(root).unwrap();
    ctx.rerun(root).unwrap();
    expect_eq!(logged(&ctx), ["ran", "rewound", "ran", "rewound", "ran"]);
}

#[gtest]
fn rerun_on_a_deleted_node_is_unknown() {
    let mut ctx = Context::new();
    let root = ctx.root(|_| {});
    ctx.rewind(root).unwrap();
    ctx.node_mut(root).unwrap().delete().unwrap();

    expect_that!(
        ctx.rerun(root),
        err(eq(RerunError::UnknownNode(UnknownNode(root))))
    );
}

#[gtest]
fn rerun_on_a_running_node_is_refused_without_rewinding_it() {
    let mut ctx = Context::new();
    let root = ctx.root(|ctx| {
        ctx.on_rewind(|ctx| log(ctx, "rewound"));
        let node = ctx.current();
        let result = ctx.rerun(node);
        ctx.insert(result);
    });

    expect_that!(
        ctx.get::<Result<(), RerunError>>(),
        some(eq(&Err(RerunError::RunnerInUse(RunnerInUse(root)))))
    );
    expect_that!(logged(&ctx), is_empty());
}

#[gtest]
fn a_rerun_node_joins_its_group_once_per_run() {
    use crate::raw::group::{GroupId, GroupingExt};

    let mut ctx = Context::new();
    let group = ctx.create_group();
    let root = ctx.root(move |ctx| {
        let node = ctx.current();
        ctx.add_to_group_until_rewind(group, node).unwrap();
    });
    let members = |ctx: &Context| ctx.group_members(group).unwrap().collect::<Vec<_>>();
    expect_eq!(members(&ctx), [root]);

    for _ in 0..3 {
        ctx.rerun(root).unwrap();
        expect_eq!(members(&ctx), [root]);
        expect_eq!(
            ctx.groups_of(root).unwrap().collect::<Vec<GroupId>>(),
            [group]
        );
        expect_that!(ctx.rewind_keys(root).unwrap().len(), eq(1));
    }

    ctx.rewind(root).unwrap();
    expect_that!(members(&ctx), is_empty());
}

#[gtest]
fn outside_of_a_running_node_there_is_no_current_node() {
    let mut ctx = Context::new();

    expect_that!(ctx.try_current(), none());
    expect_true!(catch_unwind(AssertUnwindSafe(|| ctx.current())).is_err());
    expect_true!(catch_unwind(AssertUnwindSafe(|| ctx.on_rewind(|_| {}))).is_err());

    ctx.root(|_| {});
    expect_that!(ctx.try_current(), none());
}
