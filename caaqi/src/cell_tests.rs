use caaqi_core_scoped::context::{Context, scope};

use super::*;

#[test]
fn borrows_reach_the_value() {
    let mut ctx = Context::new();
    scope(&mut ctx, || {
        let cell = store_cell(1);

        *cell.borrow_mut() += 1;
        assert_eq!(*cell.borrow(), 2);
    });
}

#[test]
fn borrows_dont_hold_the_context() {
    let mut ctx = Context::new();
    scope(&mut ctx, || {
        let a = store_cell(2);
        let b = store_cell(1);

        let a_ref = a.borrow();
        let mut b_ref = b.borrow_mut();
        let c = store_cell(*a_ref);
        *b_ref += *a_ref + *c.borrow();
        drop((a_ref, b_ref));

        assert_eq!(*b.borrow(), 5);
    });
}

#[test]
fn conflicting_borrows_are_errors() {
    let mut ctx = Context::new();
    scope(&mut ctx, || {
        let cell = store_cell(0);

        let held = cell.borrow_mut();
        assert!(matches!(cell.try_borrow(), Err(BorrowCellError::Borrow(_))));
        assert!(matches!(
            cell.try_borrow_mut(),
            Err(BorrowCellMutError::BorrowMut(_))
        ));
        drop(held);

        let held = cell.borrow();
        assert!(cell.try_borrow().is_ok());
        assert!(matches!(
            cell.try_borrow_mut(),
            Err(BorrowCellMutError::BorrowMut(_))
        ));
        drop(held);
    });
}

#[test]
#[should_panic]
fn borrow_panics_while_mutably_borrowed() {
    let mut ctx = Context::new();
    scope(&mut ctx, || {
        let cell = store_cell(0);
        let _held = cell.borrow_mut();
        cell.borrow();
    });
}

#[test]
#[should_panic]
fn borrow_mut_panics_while_borrowed() {
    let mut ctx = Context::new();
    scope(&mut ctx, || {
        let cell = store_cell(0);
        let _held = cell.borrow();
        cell.borrow_mut();
    });
}

#[test]
fn removing_a_borrowed_cell_waits_for_the_borrow() {
    let mut ctx = Context::new();
    scope(&mut ctx, || {
        let value = Rc::new(5);
        let alive = Rc::downgrade(&value);
        let cell = store_cell(value);

        let held = cell.borrow();
        cell.remove().unwrap();
        assert!(matches!(
            cell.try_borrow(),
            Err(BorrowCellError::UnknownStored(_))
        ));
        assert_eq!(**held, 5);
        assert!(alive.upgrade().is_some());

        drop(held);
        assert!(alive.upgrade().is_none());
    });
}

#[test]
fn a_borrow_outlives_the_scope() {
    let mut ctx = Context::new();
    let held = scope(&mut ctx, || store_cell(String::from("kept")).borrow());
    drop(ctx);
    assert_eq!(*held, "kept");
}

#[test]
fn removed_cell_is_unknown() {
    let mut ctx = Context::new();
    scope(&mut ctx, || {
        let cell = store_cell(5);
        let copy = cell;

        cell.remove().unwrap();

        assert!(matches!(
            copy.try_borrow(),
            Err(BorrowCellError::UnknownStored(_))
        ));
        assert!(matches!(
            copy.try_borrow_mut(),
            Err(BorrowCellMutError::UnknownStored(_))
        ));
        assert!(copy.remove().is_err());
    });
}

#[test]
#[should_panic]
fn borrowing_a_removed_cell_panics() {
    let mut ctx = Context::new();
    scope(&mut ctx, || {
        let cell = store_cell(5);
        cell.remove().unwrap();
        cell.borrow();
    });
}
