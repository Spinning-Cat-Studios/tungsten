//! Type Constructors for FFI
//!
//! This module provides C-compatible functions for constructing Core types.

use std::ffi::CStr;
use std::os::raw::c_char;

use crate::types::Type;

use crate::ffi::{with_arena, TypeHandle, INVALID_HANDLE};
// ============================================================================
// Essential Type Constructors
// ============================================================================

/// Construct Nat type
#[no_mangle]
pub extern "C" fn tg_type_nat() -> TypeHandle {
    with_arena!(|arena| arena.alloc_type(Type::Nat))
}

/// Construct Bool type
#[no_mangle]
pub extern "C" fn tg_type_bool() -> TypeHandle {
    with_arena!(|arena| arena.alloc_type(Type::Bool))
}

/// Construct String type
#[no_mangle]
pub extern "C" fn tg_type_string() -> TypeHandle {
    with_arena!(|arena| arena.alloc_type(Type::String))
}

/// Construct Unit type
#[no_mangle]
pub extern "C" fn tg_type_unit() -> TypeHandle {
    with_arena!(|arena| arena.alloc_type(Type::Unit))
}

/// Construct arrow (function) type: τ1 → τ2
#[no_mangle]
pub extern "C" fn tg_type_arrow(t1: TypeHandle, t2: TypeHandle) -> TypeHandle {
    with_arena!(|arena| {
        let t1 = match arena.get_type(t1) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let t2 = match arena.get_type(t2) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_type(Type::Arrow(Box::new(t1), Box::new(t2)))
    })
}

// ============================================================================
// Extended Type Constructors
// ============================================================================

/// Construct Void type (empty type, logical false)
#[no_mangle]
pub extern "C" fn tg_type_void() -> TypeHandle {
    with_arena!(|arena| arena.alloc_type(Type::Void))
}

/// Construct Prop type (universe of propositions)
#[no_mangle]
pub extern "C" fn tg_type_prop() -> TypeHandle {
    with_arena!(|arena| arena.alloc_type(Type::Prop))
}

/// Construct product type: τ1 × τ2
#[no_mangle]
pub extern "C" fn tg_type_product(t1: TypeHandle, t2: TypeHandle) -> TypeHandle {
    with_arena!(|arena| {
        let t1 = match arena.get_type(t1) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let t2 = match arena.get_type(t2) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_type(Type::Product(Box::new(t1), Box::new(t2)))
    })
}

/// Construct sum type: τ1 + τ2
#[no_mangle]
pub extern "C" fn tg_type_sum(t1: TypeHandle, t2: TypeHandle) -> TypeHandle {
    with_arena!(|arena| {
        let t1 = match arena.get_type(t1) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let t2 = match arena.get_type(t2) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_type(Type::Sum(Box::new(t1), Box::new(t2)))
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
    with_arena!(|arena| arena.alloc_type(Type::TyVar(name_str.to_owned())))
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
        let body = match arena.get_type(body) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_type(Type::Forall(name_str.to_owned(), Box::new(body)))
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
        let body = match arena.get_type(body) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_type(Type::Mu(name_str.to_owned(), Box::new(body)))
    })
}

/// Construct a pointer type: *τ
#[no_mangle]
pub extern "C" fn tg_type_ptr(inner: TypeHandle) -> TypeHandle {
    with_arena!(|arena| {
        let inner_ty = match arena.get_type(inner) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_type(Type::Ptr(Box::new(inner_ty)))
    })
}

/// Construct a reference type: Ref<τ>
#[no_mangle]
pub extern "C" fn tg_type_ref(inner: TypeHandle) -> TypeHandle {
    with_arena!(|arena| {
        let inner_ty = match arena.get_type(inner) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_type(Type::Ref(Box::new(inner_ty)))
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
        let ty = match arena.get_type(ty) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let t1 = match arena.get_term(t1) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let t2 = match arena.get_term(t2) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_type(Type::eq(ty, t1, t2))
    })
}
