//! Extended term constructors for FFI (case, fold/unfold, polymorphism, refs, etc.)

use std::ffi::CStr;
use std::os::raw::c_char;

use crate::terms::Term;

use super::core::tg_term_lambda;
use crate::ffi::{with_arena, TermHandle, TypeHandle, INVALID_HANDLE};

// ============================================================================
// Sum Type and Pattern Matching (Phase 3C-5)
// ============================================================================

/// Construct case analysis on a sum type: case t of inl x => t1 | inr y => t2
///
/// # Safety
/// `left_var` and `right_var` must be valid null-terminated UTF-8 strings.
#[no_mangle]
pub unsafe extern "C" fn tg_term_case(
    scrutinee: TermHandle,
    left_var: *const c_char,
    left_body: TermHandle,
    right_var: *const c_char,
    right_body: TermHandle,
) -> TermHandle {
    if left_var.is_null() || right_var.is_null() {
        return INVALID_HANDLE;
    }
    let left_var_str = match CStr::from_ptr(left_var).to_str() {
        Ok(s) => s,
        Err(_) => return INVALID_HANDLE,
    };
    let right_var_str = match CStr::from_ptr(right_var).to_str() {
        Ok(s) => s,
        Err(_) => return INVALID_HANDLE,
    };

    with_arena!(|arena| {
        let scrutinee = match arena.get_term(scrutinee) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let left_body = match arena.get_term(left_body) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let right_body = match arena.get_term(right_body) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::Case(
            Box::new(scrutinee),
            left_var_str.to_owned(),
            Box::new(left_body),
            right_var_str.to_owned(),
            Box::new(right_body),
        ))
    })
}

// ============================================================================
// Recursive Types (Phase 3C-5)
// ============================================================================

/// Construct fold: fold [μα.τ] t
///
/// Packs a value into a recursive type.
/// - t : τ[α := μα.τ]
/// - Result: μα.τ
#[no_mangle]
pub extern "C" fn tg_term_fold(mu_ty: TypeHandle, t: TermHandle) -> TermHandle {
    with_arena!(|arena| {
        let ty = match arena.get_type(mu_ty) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let t = match arena.get_term(t) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::Fold(ty, Box::new(t)))
    })
}

/// Construct unfold: unfold [μα.τ] t
///
/// Unpacks a recursive type.
/// - t : μα.τ
/// - Result: τ[α := μα.τ]
#[no_mangle]
pub extern "C" fn tg_term_unfold(mu_ty: TypeHandle, t: TermHandle) -> TermHandle {
    with_arena!(|arena| {
        let ty = match arena.get_type(mu_ty) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let t = match arena.get_term(t) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::Unfold(ty, Box::new(t)))
    })
}

// ============================================================================
// Polymorphism (Phase 3C-5)
// ============================================================================

/// Construct type abstraction: Λα. t
///
/// # Safety
/// `ty_var` must be a valid null-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn tg_term_type_abs(ty_var: *const c_char, body: TermHandle) -> TermHandle {
    if ty_var.is_null() {
        return INVALID_HANDLE;
    }
    let ty_var_str = match CStr::from_ptr(ty_var).to_str() {
        Ok(s) => s,
        Err(_) => return INVALID_HANDLE,
    };

    with_arena!(|arena| {
        let body = match arena.get_term(body) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::TyAbs(ty_var_str.to_owned(), Box::new(body)))
    })
}

/// Construct type application: t [τ]
#[no_mangle]
pub extern "C" fn tg_term_type_app(t: TermHandle, ty: TypeHandle) -> TermHandle {
    with_arena!(|arena| {
        let t = match arena.get_term(t) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let ty = match arena.get_type(ty) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::TyApp(Box::new(t), ty))
    })
}

// ============================================================================
// References (Phase 3C-5)
// ============================================================================

/// Construct a new reference: ref v
#[no_mangle]
pub extern "C" fn tg_term_ref_new(v: TermHandle) -> TermHandle {
    with_arena!(|arena| {
        let v = match arena.get_term(v) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::RefNew(Box::new(v)))
    })
}

/// Construct reference dereference: get r
#[no_mangle]
pub extern "C" fn tg_term_ref_get(r: TermHandle) -> TermHandle {
    with_arena!(|arena| {
        let r = match arena.get_term(r) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::RefGet(Box::new(r)))
    })
}

/// Construct reference assignment: set r v
#[no_mangle]
pub extern "C" fn tg_term_ref_set(r: TermHandle, v: TermHandle) -> TermHandle {
    with_arena!(|arena| {
        let r = match arena.get_term(r) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let v = match arena.get_term(v) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::RefSet(Box::new(r), Box::new(v)))
    })
}

// ============================================================================
// Term Introspection (Phase 3C-5)
// ============================================================================

/// Check if two terms are equal (structurally/α-equivalent)
#[no_mangle]
pub extern "C" fn tg_terms_equal(t1: TermHandle, t2: TermHandle) -> bool {
    with_arena!(|arena| {
        let t1 = match arena.get_term(t1) {
            Some(t) => t,
            None => return false,
        };
        let t2 = match arena.get_term(t2) {
            Some(t) => t,
            None => return false,
        };
        t1 == t2
    })
}

/// Construct a lambda abstraction (alias for `tg_term_lambda`).
///
/// This is a shorter name used by the self-hosted compiler.
///
/// # Safety
/// `var_name` must be a valid null-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn tg_term_abs(
    var_name: *const c_char,
    ty: TypeHandle,
    body: TermHandle,
) -> TermHandle {
    tg_term_lambda(var_name, ty, body)
}

/// Construct a sorry term (placeholder for incomplete proofs).
///
/// Sorry terms have a given type but no computational content.
#[no_mangle]
pub extern "C" fn tg_term_sorry(_ty: TypeHandle) -> TermHandle {
    // Note: The type argument is ignored - Term::Sorry doesn't carry a type
    with_arena!(|arena| arena.alloc_term(Term::Sorry))
}

/// Construct an early return term (ADR 13.5.26d).
///
/// Wraps the inner term in `Term::Return`. Type is ⊥ (Void).
#[no_mangle]
pub extern "C" fn tg_term_return(inner: TermHandle) -> TermHandle {
    with_arena!(|arena| {
        let inner_term = match arena.get_term(inner) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::early_return(inner_term))
    })
}
