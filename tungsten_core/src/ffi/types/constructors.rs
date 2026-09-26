//! Type Constructors for FFI
//!
//! This module provides C-compatible functions for constructing Core types.
//!
//! Constructors are O(1) node pushes: children are referenced by handle,
//! never cloned (ADR 2.7.26a §4). An invalid child handle yields
//! `INVALID_HANDLE` — same contract as the old deep-cloning constructors.

use std::ffi::CStr;
use std::os::raw::c_char;

use super::nodes::TypeNode;
use crate::ffi::{valid_terms, valid_types, with_arena, TypeHandle, INVALID_HANDLE};

// ============================================================================
// Essential Type Constructors
// ============================================================================

/// Construct Nat type
#[no_mangle]
pub extern "C" fn tg_type_nat() -> TypeHandle {
    with_arena!(|arena| arena.alloc_type_node(TypeNode::Nat))
}

/// Construct Int type (ADR 14.9.26c)
#[no_mangle]
pub extern "C" fn tg_type_int() -> TypeHandle {
    with_arena!(|arena| arena.alloc_type_node(TypeNode::Int))
}

/// Construct Bool type
#[no_mangle]
pub extern "C" fn tg_type_bool() -> TypeHandle {
    with_arena!(|arena| arena.alloc_type_node(TypeNode::Bool))
}

/// Construct String type
#[no_mangle]
pub extern "C" fn tg_type_string() -> TypeHandle {
    with_arena!(|arena| arena.alloc_type_node(TypeNode::String))
}

/// Construct Unit type
#[no_mangle]
pub extern "C" fn tg_type_unit() -> TypeHandle {
    with_arena!(|arena| arena.alloc_type_node(TypeNode::Unit))
}

/// Construct arrow (function) type: τ1 → τ2
#[no_mangle]
pub extern "C" fn tg_type_arrow(t1: TypeHandle, t2: TypeHandle) -> TypeHandle {
    with_arena!(|arena| {
        if !valid_types(arena, &[t1, t2]) {
            return INVALID_HANDLE;
        }
        arena.alloc_type_node(TypeNode::Arrow(t1, t2))
    })
}

// ============================================================================
// Extended Type Constructors
// ============================================================================

/// Construct Void type (empty type, logical false)
#[no_mangle]
pub extern "C" fn tg_type_void() -> TypeHandle {
    with_arena!(|arena| arena.alloc_type_node(TypeNode::Void))
}

/// Construct Prop type (universe of propositions)
#[no_mangle]
pub extern "C" fn tg_type_prop() -> TypeHandle {
    with_arena!(|arena| arena.alloc_type_node(TypeNode::Prop))
}

/// Construct product type: τ1 × τ2
#[no_mangle]
pub extern "C" fn tg_type_product(t1: TypeHandle, t2: TypeHandle) -> TypeHandle {
    with_arena!(|arena| {
        if !valid_types(arena, &[t1, t2]) {
            return INVALID_HANDLE;
        }
        arena.alloc_type_node(TypeNode::Product(t1, t2))
    })
}

/// Construct sum type: τ1 + τ2
#[no_mangle]
pub extern "C" fn tg_type_sum(t1: TypeHandle, t2: TypeHandle) -> TypeHandle {
    with_arena!(|arena| {
        if !valid_types(arena, &[t1, t2]) {
            return INVALID_HANDLE;
        }
        arena.alloc_type_node(TypeNode::Sum(t1, t2))
    })
}

/// Construct a type variable: α
///
/// # Safety
/// `name` must be a valid null-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn tg_type_var(name: *const c_char) -> TypeHandle {
    if name.is_null() {
        return INVALID_HANDLE;
    }
    let name_str = match CStr::from_ptr(name).to_str() {
        Ok(s) => s,
        Err(_) => return INVALID_HANDLE,
    };
    with_arena!(|arena| arena.alloc_type_node(TypeNode::TyVar(name_str.to_owned())))
}

/// Construct a forall type: ∀α. τ
///
/// # Safety
/// `var_name` must be a valid null-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn tg_type_forall(var_name: *const c_char, body: TypeHandle) -> TypeHandle {
    if var_name.is_null() {
        return INVALID_HANDLE;
    }
    let name_str = match CStr::from_ptr(var_name).to_str() {
        Ok(s) => s,
        Err(_) => return INVALID_HANDLE,
    };
    with_arena!(|arena| {
        if !valid_types(arena, &[body]) {
            return INVALID_HANDLE;
        }
        arena.alloc_type_node(TypeNode::Forall(name_str.to_owned(), body))
    })
}

/// Construct a recursive type: μα. τ
///
/// # Safety
/// `var_name` must be a valid null-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn tg_type_mu(var_name: *const c_char, body: TypeHandle) -> TypeHandle {
    if var_name.is_null() {
        return INVALID_HANDLE;
    }
    let name_str = match CStr::from_ptr(var_name).to_str() {
        Ok(s) => s,
        Err(_) => return INVALID_HANDLE,
    };
    with_arena!(|arena| {
        if !valid_types(arena, &[body]) {
            return INVALID_HANDLE;
        }
        arena.alloc_type_node(TypeNode::Mu(name_str.to_owned(), body))
    })
}

/// Construct a pointer type: *τ
#[no_mangle]
pub extern "C" fn tg_type_ptr(inner: TypeHandle) -> TypeHandle {
    with_arena!(|arena| {
        if !valid_types(arena, &[inner]) {
            return INVALID_HANDLE;
        }
        arena.alloc_type_node(TypeNode::Ptr(inner))
    })
}

/// Construct a reference type: Ref<τ>
#[no_mangle]
pub extern "C" fn tg_type_ref(inner: TypeHandle) -> TypeHandle {
    with_arena!(|arena| {
        if !valid_types(arena, &[inner]) {
            return INVALID_HANDLE;
        }
        arena.alloc_type_node(TypeNode::Ref(inner))
    })
}

/// Construct an equality type: Eq τ t₁ t₂
///
/// This represents propositional equality between two terms of the same type.
#[no_mangle]
pub extern "C" fn tg_type_eq(
    ty: TypeHandle,
    t1: super::super::TermHandle,
    t2: super::super::TermHandle,
) -> TypeHandle {
    with_arena!(|arena| {
        if !valid_terms(arena, &[t1, t2]) || !valid_types(arena, &[ty]) {
            return INVALID_HANDLE;
        }
        arena.alloc_type_node(TypeNode::Eq(ty, t1, t2))
    })
}
