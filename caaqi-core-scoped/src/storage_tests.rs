use super::*;
use crate::context::{Context, scope};

#[test]
fn stored_values_are_reached_through_closures() {
    let mut ctx = Context::new();
    scope(&mut ctx, || {
        let name = store(String::from("Ada"));
        name.get_mut(|name| name.push_str(" Lovelace")).unwrap();
        assert_eq!(name.get(|name| name.len()), Ok(12));
        assert_eq!(name.remove().unwrap(), "Ada Lovelace");
        assert!(name.get(|_| ()).is_err());
    });
}

#[test]
fn stored_converts_to_and_from_caaqi_core_stored() {
    let mut ctx = Context::new();
    scope(&mut ctx, || {
        let stored = store(1);
        let inner = caaqi_core::storage::Stored::from(stored);
        assert_eq!(Stored::from(inner), stored);
        assert_eq!(Stored::from(inner).get(|n| *n), Ok(1));
    });
}
