//! Tests for FFI module

use std::os::raw::c_char;

use super::terms::core::*;
use super::terms::core_data::*;
use super::terms::ext::*;
use super::types::accessors::*;
use super::types::constructors::*;
use super::types::predicates::*;
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

#[test]
fn test_type_format_display() {
    use super::check::tg_type_format_display;
    use std::ffi::CStr;

    tg_init();

    // Test primitive types
    let nat = tg_type_nat();
    let ptr = tg_type_format_display(nat);
    assert!(!ptr.is_null());
    let s = unsafe { CStr::from_ptr(ptr) }.to_str().unwrap();
    assert_eq!(s, "Nat");
    unsafe { drop(std::ffi::CString::from_raw(ptr as *mut c_char)) };

    let bool_ty = tg_type_bool();
    let ptr = tg_type_format_display(bool_ty);
    assert!(!ptr.is_null());
    let s = unsafe { CStr::from_ptr(ptr) }.to_str().unwrap();
    assert_eq!(s, "Bool");
    unsafe { drop(std::ffi::CString::from_raw(ptr as *mut c_char)) };

    let string_ty = tg_type_string();
    let ptr = tg_type_format_display(string_ty);
    assert!(!ptr.is_null());
    let s = unsafe { CStr::from_ptr(ptr) }.to_str().unwrap();
    assert_eq!(s, "String");
    unsafe { drop(std::ffi::CString::from_raw(ptr as *mut c_char)) };

    // Test arrow type
    let arrow = tg_type_arrow(nat, nat);
    let ptr = tg_type_format_display(arrow);
    assert!(!ptr.is_null());
    let s = unsafe { CStr::from_ptr(ptr) }.to_str().unwrap();
    assert_eq!(s, "(Nat → Nat)");
    unsafe { drop(std::ffi::CString::from_raw(ptr as *mut c_char)) };

    // Test invalid handle
    let ptr = tg_type_format_display(INVALID_HANDLE);
    assert!(ptr.is_null());
}

#[test]
fn test_type_tag() {
    tg_init();
    assert_eq!(tg_type_tag(tg_type_nat()), 0);
    assert_eq!(tg_type_tag(tg_type_bool()), 1);
    assert_eq!(tg_type_tag(tg_type_string()), 2);
    assert_eq!(tg_type_tag(tg_type_unit()), 3);
    assert_eq!(tg_type_tag(tg_type_void()), 4);
    assert_eq!(tg_type_tag(tg_type_prop()), 5);

    let nat = tg_type_nat();
    assert_eq!(tg_type_tag(tg_type_arrow(nat, nat)), 6);
    assert_eq!(tg_type_tag(tg_type_product(nat, nat)), 7);
    assert_eq!(tg_type_tag(tg_type_sum(nat, nat)), 8);

    let name = std::ffi::CString::new("X").unwrap();
    assert_eq!(tg_type_tag(unsafe { tg_type_var(name.as_ptr()) }), 9);
    assert_eq!(
        tg_type_tag(unsafe { tg_type_forall(name.as_ptr(), nat) }),
        10
    );
    assert_eq!(tg_type_tag(unsafe { tg_type_mu(name.as_ptr(), nat) }), 11);
    let zero = tg_term_zero();
    assert_eq!(tg_type_tag(tg_type_eq(nat, zero, zero)), 12);
    assert_eq!(tg_type_tag(tg_type_ref(nat)), 13);
    assert_eq!(tg_type_tag(tg_type_ptr(nat)), 14);

    // Invalid handle
    assert_eq!(tg_type_tag(INVALID_HANDLE), 99);
}
