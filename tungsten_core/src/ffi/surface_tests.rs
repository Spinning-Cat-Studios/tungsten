//! Exhaustive FFI type-surface tests (ADR 2.7.26a §4 close-out).
//!
//! The node-arena migration rewrote every constructor, accessor, and
//! predicate; this module pins the whole surface: each predicate on its own
//! shape AND a foreign shape, each accessor's exact child handle (sharing
//! makes handle equality exact) AND its wrong-shape `INVALID_HANDLE` path,
//! plus the node heap-bytes arithmetic for both arenas.

use std::ffi::CString;

use super::terms::core::*;
use super::terms::core_data::*;
use super::terms::nodes::{node_heap_bytes_term, TermNode};
use super::types::accessors::*;
use super::types::accessors_introspection::*;
use super::types::constructors::*;
use super::types::nodes::{node_heap_bytes, TypeNode};
use super::types::predicates::*;
use super::{tg_init, TypeHandle, INVALID_HANDLE};

/// Build one handle of every distinguishable type shape.
struct TypeShapes {
    nat: TypeHandle,
    arrow: TypeHandle,
    product: TypeHandle,
    sum: TypeHandle,
    tyvar: TypeHandle,
    forall: TypeHandle,
    mu: TypeHandle,
    eq: TypeHandle,
    ptr: TypeHandle,
    ref_ty: TypeHandle,
    error: TypeHandle,
}

fn build_shapes() -> TypeShapes {
    tg_init();
    let nat = tg_type_nat();
    let bool_ty = tg_type_bool();
    let name = CString::new("alpha").unwrap();
    let zero = tg_term_zero();
    unsafe {
        TypeShapes {
            nat,
            arrow: tg_type_arrow(nat, bool_ty),
            product: tg_type_product(nat, bool_ty),
            sum: tg_type_sum(nat, bool_ty),
            tyvar: tg_type_var(name.as_ptr()),
            forall: tg_type_forall(name.as_ptr(), nat),
            mu: tg_type_mu(name.as_ptr(), bool_ty),
            eq: tg_type_eq(nat, zero, zero),
            ptr: tg_type_ptr(nat),
            ref_ty: tg_type_ref(bool_ty),
            error: tg_type_error(),
        }
    }
}

#[test]
fn every_predicate_is_true_on_its_shape_and_false_elsewhere() {
    let s = build_shapes();
    let checks: [(&str, extern "C" fn(TypeHandle) -> bool, TypeHandle); 8] = [
        ("is_arrow", tg_type_is_arrow, s.arrow),
        ("is_product", tg_type_is_product, s.product),
        ("is_sum", tg_type_is_sum, s.sum),
        ("is_tyvar", tg_type_is_tyvar, s.tyvar),
        ("is_forall", tg_type_is_forall, s.forall),
        ("is_mu", tg_type_is_mu, s.mu),
        ("is_eq", tg_type_is_eq, s.eq),
        ("is_type_error", tg_is_type_error, s.error),
    ];
    for (name, pred, own_shape) in checks {
        assert!(pred(own_shape), "{name} must hold on its own shape");
        assert!(!pred(s.nat), "{name} must not hold on Nat");
        assert!(!pred(INVALID_HANDLE), "{name} must not hold on INVALID");
    }
    // tg_type_is_app: no App constructor on the FFI surface — false on all.
    assert!(!tg_type_is_app(s.arrow));
    assert!(!tg_type_is_app(s.tyvar));
    assert!(!tg_type_is_app(INVALID_HANDLE));
}

#[test]
fn every_accessor_returns_the_shared_child_handle() {
    let s = build_shapes();
    let nat = tg_type_get_arrow_domain(s.arrow);
    let bool_ty = tg_type_get_arrow_codomain(s.arrow);
    // Accessors return the STORED child handle — sharing makes this exact.
    assert_eq!(tg_type_get_product_left(s.product), nat);
    assert_eq!(tg_type_get_product_right(s.product), bool_ty);
    assert_eq!(tg_type_get_sum_left(s.sum), nat);
    assert_eq!(tg_type_get_sum_right(s.sum), bool_ty);
    assert_eq!(tg_type_get_forall_body(s.forall), nat);
    assert_eq!(tg_type_get_mu_body(s.mu), bool_ty);
    assert_eq!(tg_type_get_eq_type(s.eq), nat);
    // Eq term components round-trip as term handles.
    let lhs = tg_type_get_eq_lhs(s.eq);
    let rhs = tg_type_get_eq_rhs(s.eq);
    assert_ne!(lhs, INVALID_HANDLE);
    assert_eq!(lhs, rhs, "both Eq components were the same zero term");
}

#[test]
fn every_accessor_rejects_foreign_shapes() {
    let s = build_shapes();
    assert_eq!(tg_type_get_arrow_domain(s.nat), INVALID_HANDLE);
    assert_eq!(tg_type_get_arrow_codomain(s.sum), INVALID_HANDLE);
    assert_eq!(tg_type_get_product_left(s.arrow), INVALID_HANDLE);
    assert_eq!(tg_type_get_product_right(s.arrow), INVALID_HANDLE);
    assert_eq!(tg_type_get_sum_left(s.product), INVALID_HANDLE);
    assert_eq!(tg_type_get_sum_right(s.product), INVALID_HANDLE);
    assert_eq!(tg_type_get_forall_body(s.mu), INVALID_HANDLE);
    assert_eq!(tg_type_get_mu_body(s.forall), INVALID_HANDLE);
    assert_eq!(tg_type_get_eq_type(s.nat), INVALID_HANDLE);
    assert_eq!(tg_type_get_eq_lhs(s.nat), INVALID_HANDLE);
    assert_eq!(tg_type_get_eq_rhs(s.nat), INVALID_HANDLE);
    assert_eq!(tg_type_get_arrow_domain(INVALID_HANDLE), INVALID_HANDLE);
}

#[test]
fn name_accessors_return_the_stored_names() {
    let s = build_shapes();
    for (accessor, handle) in [
        (
            tg_type_get_tyvar_name as extern "C" fn(TypeHandle) -> *const std::os::raw::c_char,
            s.tyvar,
        ),
        (tg_type_get_forall_var, s.forall),
        (tg_type_get_mu_var, s.mu),
    ] {
        let ptr = accessor(handle);
        assert!(!ptr.is_null());
        let name = unsafe { std::ffi::CStr::from_ptr(ptr) }.to_str().unwrap();
        assert_eq!(name, "alpha");
        unsafe { drop(CString::from_raw(ptr.cast_mut())) };
        assert!(accessor(s.nat).is_null(), "wrong shape must yield null");
    }
    assert!(tg_type_get_app_name(s.tyvar).is_null());
}

#[test]
fn constructors_reject_invalid_children() {
    let s = build_shapes();
    let name = CString::new("v").unwrap();
    assert_eq!(tg_type_arrow(INVALID_HANDLE, s.nat), INVALID_HANDLE);
    assert_eq!(tg_type_arrow(s.nat, INVALID_HANDLE), INVALID_HANDLE);
    assert_eq!(tg_type_product(INVALID_HANDLE, s.nat), INVALID_HANDLE);
    assert_eq!(tg_type_sum(s.nat, INVALID_HANDLE), INVALID_HANDLE);
    assert_eq!(tg_type_ptr(INVALID_HANDLE), INVALID_HANDLE);
    assert_eq!(tg_type_ref(INVALID_HANDLE), INVALID_HANDLE);
    unsafe {
        assert_eq!(
            tg_type_forall(name.as_ptr(), INVALID_HANDLE),
            INVALID_HANDLE
        );
        assert_eq!(tg_type_mu(name.as_ptr(), INVALID_HANDLE), INVALID_HANDLE);
    }
    let zero = tg_term_zero();
    assert_eq!(tg_type_eq(INVALID_HANDLE, zero, zero), INVALID_HANDLE);
    assert_eq!(tg_type_eq(s.nat, INVALID_HANDLE, zero), INVALID_HANDLE);
    // Term constructors validate both term and type children.
    assert_eq!(tg_term_succ(INVALID_HANDLE), INVALID_HANDLE);
    assert_eq!(tg_term_app(zero, INVALID_HANDLE), INVALID_HANDLE);
    assert_eq!(tg_term_pair(INVALID_HANDLE, zero), INVALID_HANDLE);
    assert_eq!(tg_term_inl(INVALID_HANDLE, zero), INVALID_HANDLE);
    assert_eq!(tg_term_inl(s.sum, INVALID_HANDLE), INVALID_HANDLE);
    unsafe {
        let v = CString::new("x").unwrap();
        assert_eq!(
            tg_term_lambda(v.as_ptr(), INVALID_HANDLE, zero),
            INVALID_HANDLE
        );
        assert_eq!(
            tg_term_lambda(v.as_ptr(), s.nat, INVALID_HANDLE),
            INVALID_HANDLE
        );
    }
}

/// Exact-value arithmetic for the TYPE node heap accounting: every product
/// and sum in the App/Adt arms must matter (multi-element vecs, sized
/// strings, non-zero contributions on every path).
#[test]
fn type_node_heap_bytes_arithmetic_is_exact() {
    let handle = size_of::<TypeHandle>() as u64;
    let sized = super::test_support::sized_name;
    let args = vec![1 as TypeHandle, 2, 3];
    let args_cap = args.capacity() as u64;
    assert_eq!(
        node_heap_bytes(&TypeNode::App(sized("Forest"), args)),
        6 + args_cap * handle
    );
    let type_args = vec![4 as TypeHandle, 5];
    let ta_cap = type_args.capacity() as u64;
    let variants = vec![(sized("Leaf"), 6 as TypeHandle), (sized("Node"), 7)];
    let v_cap = variants.capacity() as u64;
    let v_slot = size_of::<(String, TypeHandle)>() as u64;
    assert_eq!(
        node_heap_bytes(&TypeNode::Adt(sized("Tree"), type_args, variants)),
        4 + ta_cap * handle + v_cap * v_slot + 4 + 4
    );
    assert_eq!(node_heap_bytes(&TypeNode::TyVar(sized("ab"))), 2);
    assert_eq!(node_heap_bytes(&TypeNode::Eq(0, 1, 2)), 0);
}

/// Exact-value arithmetic for the TERM node heap accounting.
#[test]
fn term_node_heap_bytes_arithmetic_is_exact() {
    let handle = size_of::<super::TermHandle>() as u64;
    let sized = super::test_support::sized_name;
    let args = vec![1 as super::TermHandle, 2, 3];
    let args_cap = args.capacity() as u64;
    assert_eq!(
        node_heap_bytes_term(&TermNode::ExternCall(sized("tg_fn"), args)),
        5 + args_cap * handle
    );
    let arms = vec![
        (0usize, sized("Leaf"), 4 as super::TermHandle),
        (1, sized("Node"), 5),
    ];
    let arms_cap = arms.capacity() as u64;
    let arm_slot = size_of::<(usize, String, super::TermHandle)>() as u64;
    assert_eq!(
        node_heap_bytes_term(&TermNode::AdtMatch(9, arms)),
        arms_cap * arm_slot + 4 + 4
    );
    assert_eq!(
        node_heap_bytes_term(&TermNode::Case(0, sized("lft"), 1, sized("rt"), 2)),
        3 + 2
    );
    assert_eq!(node_heap_bytes_term(&TermNode::Subst(0, 1, 2, 3)), 0);
}
