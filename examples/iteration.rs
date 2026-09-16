use bevy::{
    MinimalPlugins,
    app::{App, Startup},
    ecs::system::Commands,
    log::LogPlugin,
};
use bevy_caaqi::prelude::*;

#[derive(Clone, Debug)]
struct User {
    name: &'static str,
    age: u32,
    active: bool,
    role: Role,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Role {
    Admin,
    Member,
    Guest,
}

#[derive(Clone, Copy)]
enum RoleFilter {
    Everyone,
    Staff,
    Exact(Role),
}

fn main() {
    App::new()
        .add_plugins((
            MinimalPlugins,
            LogPlugin {
                filter: "info,iteration=debug".into(),
                ..Default::default()
            },
            CaaqiPlugin,
        ))
        .add_systems(Startup, queue_eval)
        .run();
}

fn queue_eval(commands: Commands) {
    defer_action_eval(commands, move || {
        let users = state(vec![
            User {
                name: "Alice",
                age: 31,
                active: true,
                role: Role::Admin,
            },
            User {
                name: "Bob",
                age: 17,
                active: true,
                role: Role::Member,
            },
            User {
                name: "Cara",
                age: 25,
                active: false,
                role: Role::Member,
            },
            User {
                name: "David",
                age: 42,
                active: true,
                role: Role::Member,
            },
            User {
                name: "Eva",
                age: 23,
                active: true,
                role: Role::Guest,
            },
        ]);

        let mut active_only = state(false);
        let mut age_filter_enabled = state(false);
        let minimum_age = state(18);
        let mut role_filter = state(RoleFilter::Everyone);
        let mut search = state(String::new());

        let mut filtered = var(Vec::<User>::new());

        action(move || {
            let source = users.read().clone();
            *filtered.write() = source;
        });

        // Disable a filter and previously removed users can return.
        action(move || {
            if *active_only.read() {
                filtered.write().retain(|user| user.active);
            }
        });

        // minimum_age is only tracked while this filter is enabled.
        action(move || {
            if *age_filter_enabled.read() {
                let minimum = *minimum_age.read();
                filtered.write().retain(|user| user.age >= minimum);
            }
        });

        action(move || {
            match *role_filter.read() {
                RoleFilter::Everyone => {}
                RoleFilter::Staff => {
                    filtered.write().retain(|user| user.role == Role::Admin);
                }
                RoleFilter::Exact(role) => {
                    filtered.write().retain(|user| user.role == role);
                }
            }
        });

        action(move || {
            let query = search.read().trim().to_lowercase();

            if !query.is_empty() {
                filtered
                    .write()
                    .retain(|user| user.name.to_lowercase().contains(&query));
            }
        });

        action(move || {
            let names: Vec<_> = filtered
                .read()
                .iter()
                .map(|user| user.name)
                .collect();

            bevy::log::debug!("Matching users: {names:?}");
        });

        // Try different inputs: these narrow the initial five users to David.
        action(move || {
            active_only.set(true);
            age_filter_enabled.set(true);
            role_filter.set(RoleFilter::Exact(Role::Member));
            search.set("da".to_owned());
        });
    });
}
