use super::*;

#[test]
fn changes_value_through_shared_context() {
    let mut ctx = Context::new();
    let cell = ctx.store_cell(1);

    let shared: &Context = &ctx;
    *shared.borrow_cell_mut(cell) += 1;
    assert_eq!(*shared.borrow_cell(cell), 2);
}

#[test]
fn conflicting_borrows_are_errors() {
    let mut ctx = Context::new();
    let cell = ctx.store_cell(0);

    let held = ctx.try_borrow_cell_mut(cell).unwrap();
    assert!(matches!(
        ctx.try_borrow_cell(cell),
        Err(BorrowCellError::Borrow(_))
    ));
    assert!(matches!(
        ctx.try_borrow_cell_mut(cell),
        Err(BorrowCellMutError::BorrowMut(_))
    ));
    drop(held);

    let held = ctx.try_borrow_cell(cell).unwrap();
    assert!(ctx.try_borrow_cell(cell).is_ok());
    assert!(matches!(
        ctx.try_borrow_cell_mut(cell),
        Err(BorrowCellMutError::BorrowMut(_))
    ));
    drop(held);
}

#[test]
#[should_panic]
fn borrow_cell_panics_while_mutably_borrowed() {
    let mut ctx = Context::new();
    let cell = ctx.store_cell(0);

    let _held = ctx.borrow_cell_mut(cell);
    ctx.borrow_cell(cell);
}

#[test]
#[should_panic]
fn borrow_cell_mut_panics_while_borrowed() {
    let mut ctx = Context::new();
    let cell = ctx.store_cell(0);

    let _held = ctx.borrow_cell(cell);
    ctx.borrow_cell_mut(cell);
}

#[test]
fn removed_cell_is_unknown() {
    let mut ctx = Context::new();
    let cell = ctx.store_cell(5);
    let copy = cell;

    assert_eq!(ctx.remove_cell(cell).unwrap(), 5);

    assert!(matches!(
        ctx.try_borrow_cell(copy),
        Err(BorrowCellError::UnknownStored(_))
    ));
    assert!(matches!(
        ctx.try_borrow_cell_mut(copy),
        Err(BorrowCellMutError::UnknownStored(_))
    ));
    assert!(ctx.remove_cell(copy).is_err());
}

#[test]
#[should_panic]
fn borrow_removed_cell_panics() {
    let mut ctx = Context::new();
    let cell = ctx.store_cell(5);
    ctx.remove_cell(cell).unwrap();

    ctx.borrow_cell(cell);
}
