//! Tests for FFI module

use std::os::raw::c_char;

use super::*;

#[test]
fn test_arena_init() {
    tg_init();
    // After init, arena should be empty
    with_arena_ref!(|arena| {
        assert!(arena.terms.is_empty());
        assert!(arena.types.is_empty());
        assert!(arena.ctxs.is_empty());
        assert!(arena.last_error.is_empty());
    });
}

#[test]
fn test_term_constructors() {
    tg_init();

    // Test basic term constructors
    let zero = tg_term_zero();
    assert_ne!(zero, INVALID_HANDLE);

    let succ_zero = tg_term_succ(zero);
    assert_ne!(succ_zero, INVALID_HANDLE);

    let t = tg_term_true();
    assert_ne!(t, INVALID_HANDLE);

    let f = tg_term_false();
    assert_ne!(f, INVALID_HANDLE);

    let unit = tg_term_unit();
    assert_ne!(unit, INVALID_HANDLE);
}

#[test]
fn test_type_constructors() {
    tg_init();

    let nat = tg_type_nat();
    assert_ne!(nat, INVALID_HANDLE);

    let bool_ty = tg_type_bool();
    assert_ne!(bool_ty, INVALID_HANDLE);

    let arrow = tg_type_arrow(nat, bool_ty);
    assert_ne!(arrow, INVALID_HANDLE);
}

#[test]
fn test_context_operations() {
    tg_init();

    let ctx = tg_ctx_empty();
    assert_ne!(ctx, INVALID_HANDLE);

    let nat_ty = tg_type_nat();

    unsafe {
        let name = b"x\0".as_ptr() as *const c_char;
        let ctx2 = tg_ctx_extend(ctx, name, nat_ty);
        assert_ne!(ctx2, INVALID_HANDLE);

        // Lookup should find x
        let found_ty = tg_ctx_lookup(ctx2, name);
        assert_ne!(found_ty, INVALID_HANDLE);

        // Lookup should not find y
        let not_found = tg_ctx_lookup(ctx2, b"y\0".as_ptr() as *const c_char);
        assert_eq!(not_found, INVALID_HANDLE);
    }
}

#[test]
fn test_typecheck_success() {
    tg_init();

    // Type-check: zero : Nat
    let ctx = tg_ctx_empty();
    let zero = tg_term_zero();

    let mut out_ty: TypeHandle = INVALID_HANDLE;
    unsafe {
        let success = tg_typecheck(ctx, zero, &mut out_ty);
        assert!(success);
        assert_ne!(out_ty, INVALID_HANDLE);
    }

    // Check the inferred type is Nat
    let nat = tg_type_nat();
    assert!(tg_types_equal(out_ty, nat));
}

#[test]
fn test_typecheck_lambda() {
    tg_init();

    // Type-check: λx:Nat. x : Nat → Nat
    let nat_ty = tg_type_nat();

    unsafe {
        let var_name = b"x\0".as_ptr() as *const c_char;
        let x = tg_term_var_named(var_name);
        let id = tg_term_lambda(var_name, nat_ty, x);

        let ctx = tg_ctx_empty();
        let mut out_ty: TypeHandle = INVALID_HANDLE;
        let success = tg_typecheck(ctx, id, &mut out_ty);
        assert!(success, "Lambda should type-check");
        assert_ne!(out_ty, INVALID_HANDLE);

        // Check it's Nat → Nat
        let expected = tg_type_arrow(nat_ty, nat_ty);
        assert!(tg_types_equal(out_ty, expected));
    }
}

#[test]
fn test_typecheck_failure() {
    tg_init();

    // Create an ill-typed term: true + zero (application of true to zero)
    let t = tg_term_true();
    let zero = tg_term_zero();
    let bad = tg_term_app(t, zero);

    let ctx = tg_ctx_empty();
    let mut out_ty: TypeHandle = INVALID_HANDLE;
    unsafe {
        let success = tg_typecheck(ctx, bad, &mut out_ty);
        assert!(!success, "Bad term should not type-check");

        // Check error is set
        let err_len = tg_get_last_error_len();
        assert!(err_len > 0, "Error message should be set");
    }
}
