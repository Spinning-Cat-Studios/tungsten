//! Type Constructors for FFI
//!
//! This module provides C-compatible functions for constructing Core types.

use std::ffi::CStr;
use std::os::raw::c_char;

use crate::types::Type;

use super::{with_arena, TypeHandle, INVALID_HANDLE};

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
    t1: super::TermHandle,
    t2: super::TermHandle,
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

// ============================================================================
// Type Introspection (Phase 3C-5)
// ============================================================================

/// Check if a type is a μ-type (recursive type)
#[no_mangle]
pub extern "C" fn tg_type_is_mu(ty: TypeHandle) -> bool {
    with_arena!(|arena| {
        match arena.get_type(ty) {
            Some(Type::Mu(_, _)) => true,
            _ => false,
        }
    })
}

/// Get the body of a μ-type. Returns `INVALID_HANDLE` if not a μ-type.
#[no_mangle]
pub extern "C" fn tg_type_get_mu_body(ty: TypeHandle) -> TypeHandle {
    with_arena!(|arena| {
        match arena.get_type(ty) {
            Some(Type::Mu(_, body)) => arena.alloc_type((**body).clone()),
            _ => INVALID_HANDLE,
        }
    })
}

/// Check if a type is a sum type
#[no_mangle]
pub extern "C" fn tg_type_is_sum(ty: TypeHandle) -> bool {
    with_arena!(|arena| {
        match arena.get_type(ty) {
            Some(Type::Sum(_, _)) => true,
            _ => false,
        }
    })
}

/// Get the left component of a sum type. Returns `INVALID_HANDLE` if not a sum.
#[no_mangle]
pub extern "C" fn tg_type_get_sum_left(ty: TypeHandle) -> TypeHandle {
    with_arena!(|arena| {
        match arena.get_type(ty) {
            Some(Type::Sum(left, _)) => arena.alloc_type((**left).clone()),
            _ => INVALID_HANDLE,
        }
    })
}

/// Get the right component of a sum type. Returns `INVALID_HANDLE` if not a sum.
#[no_mangle]
pub extern "C" fn tg_type_get_sum_right(ty: TypeHandle) -> TypeHandle {
    with_arena!(|arena| {
        match arena.get_type(ty) {
            Some(Type::Sum(_, right)) => arena.alloc_type((**right).clone()),
            _ => INVALID_HANDLE,
        }
    })
}

/// Check if a type is a product type
#[no_mangle]
pub extern "C" fn tg_type_is_product(ty: TypeHandle) -> bool {
    with_arena!(|arena| {
        match arena.get_type(ty) {
            Some(Type::Product(_, _)) => true,
            _ => false,
        }
    })
}

/// Get the left component of a product type. Returns `INVALID_HANDLE` if not a product.
#[no_mangle]
pub extern "C" fn tg_type_get_product_left(ty: TypeHandle) -> TypeHandle {
    with_arena!(|arena| {
        match arena.get_type(ty) {
            Some(Type::Product(left, _)) => arena.alloc_type((**left).clone()),
            _ => INVALID_HANDLE,
        }
    })
}

/// Get the right component of a product type. Returns `INVALID_HANDLE` if not a product.
#[no_mangle]
pub extern "C" fn tg_type_get_product_right(ty: TypeHandle) -> TypeHandle {
    with_arena!(|arena| {
        match arena.get_type(ty) {
            Some(Type::Product(_, right)) => arena.alloc_type((**right).clone()),
            _ => INVALID_HANDLE,
        }
    })
}

/// Check if a type is an arrow (function) type
#[no_mangle]
pub extern "C" fn tg_type_is_arrow(ty: TypeHandle) -> bool {
    with_arena!(|arena| {
        match arena.get_type(ty) {
            Some(Type::Arrow(_, _)) => true,
            _ => false,
        }
    })
}

/// Get the domain of an arrow type. Returns `INVALID_HANDLE` if not an arrow.
#[no_mangle]
pub extern "C" fn tg_type_get_arrow_domain(ty: TypeHandle) -> TypeHandle {
    with_arena!(|arena| {
        match arena.get_type(ty) {
            Some(Type::Arrow(domain, _)) => arena.alloc_type((**domain).clone()),
            _ => INVALID_HANDLE,
        }
    })
}

/// Get the codomain of an arrow type. Returns `INVALID_HANDLE` if not an arrow.
#[no_mangle]
pub extern "C" fn tg_type_get_arrow_codomain(ty: TypeHandle) -> TypeHandle {
    with_arena!(|arena| {
        match arena.get_type(ty) {
            Some(Type::Arrow(_, codomain)) => arena.alloc_type((**codomain).clone()),
            _ => INVALID_HANDLE,
        }
    })
}

/// Check if a type is an equality type (Eq τ t₁ t₂)
#[no_mangle]
pub extern "C" fn tg_type_is_eq(ty: TypeHandle) -> bool {
    with_arena!(|arena| {
        match arena.get_type(ty) {
            Some(Type::Eq(_, _, _)) => true,
            _ => false,
        }
    })
}

/// Get the type component of an equality type. Returns `INVALID_HANDLE` if not Eq.
#[no_mangle]
pub extern "C" fn tg_type_get_eq_type(ty: TypeHandle) -> TypeHandle {
    with_arena!(|arena| {
        match arena.get_type(ty) {
            Some(Type::Eq(inner_ty, _, _)) => arena.alloc_type((**inner_ty).clone()),
            _ => INVALID_HANDLE,
        }
    })
}

/// Get the LHS term of an equality type. Returns `INVALID_HANDLE` if not Eq.
#[no_mangle]
pub extern "C" fn tg_type_get_eq_lhs(ty: TypeHandle) -> super::TermHandle {
    with_arena!(|arena| {
        match arena.get_type(ty) {
            Some(Type::Eq(_, lhs, _)) => arena.alloc_term((**lhs).clone()),
            _ => INVALID_HANDLE,
        }
    })
}

/// Get the RHS term of an equality type. Returns `INVALID_HANDLE` if not Eq.
#[no_mangle]
pub extern "C" fn tg_type_get_eq_rhs(ty: TypeHandle) -> super::TermHandle {
    with_arena!(|arena| {
        match arena.get_type(ty) {
            Some(Type::Eq(_, _, rhs)) => arena.alloc_term((**rhs).clone()),
            _ => INVALID_HANDLE,
        }
    })
}

/// Check if a type is a forall type (∀α. τ)
#[no_mangle]
pub extern "C" fn tg_type_is_forall(ty: TypeHandle) -> bool {
    with_arena!(|arena| {
        match arena.get_type(ty) {
            Some(Type::Forall(_, _)) => true,
            _ => false,
        }
    })
}

/// Get the body of a forall type. Returns `INVALID_HANDLE` if not Forall.
/// Note: The body still has the free type variable - full substitution
/// requires additional support.
#[no_mangle]
pub extern "C" fn tg_type_get_forall_body(ty: TypeHandle) -> TypeHandle {
    with_arena!(|arena| {
        match arena.get_type(ty) {
            Some(Type::Forall(_, body)) => arena.alloc_type((**body).clone()),
            _ => INVALID_HANDLE,
        }
    })
}

// ============================================================================
// Type Substitution (Phase 3C-6)
// ============================================================================

/// Substitute a type variable in a type: τ[α := τ']
///
/// # Safety
/// `var_name` must be a valid null-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn tg_type_substitute(
    ty: TypeHandle,
    var_name: *const c_char,
    replacement: TypeHandle,
) -> TypeHandle {
    if var_name.is_null() {
        return INVALID_HANDLE;
    }
    let name_str = match CStr::from_ptr(var_name).to_str() {
        Ok(s) => s,
        Err(_) => return INVALID_HANDLE,
    };

    with_arena!(|arena| {
        let ty = match arena.get_type(ty) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let replacement = match arena.get_type(replacement) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_type(ty.substitute(name_str, &replacement))
    })
}

/// Get the variable name of a μ-type. Returns null pointer if not a μ-type.
///
/// The returned string is valid until freed. Caller is responsible for memory.
#[no_mangle]
pub extern "C" fn tg_type_get_mu_var(ty: TypeHandle) -> *const c_char {
    use std::ffi::CString;

    with_arena!(|arena| {
        match arena.get_type(ty) {
            Some(Type::Mu(var, _)) => {
                // Create a CString and leak it (caller responsible for memory)
                match CString::new(var.as_str()) {
                    Ok(cstr) => cstr.into_raw().cast_const(),
                    Err(_) => std::ptr::null(),
                }
            }
            _ => std::ptr::null(),
        }
    })
}

/// Get the variable name of a forall type. Returns null pointer if not Forall.
///
/// The returned string is valid until freed. Caller is responsible for memory.
#[no_mangle]
pub extern "C" fn tg_type_get_forall_var(ty: TypeHandle) -> *const c_char {
    use std::ffi::CString;

    with_arena!(|arena| {
        match arena.get_type(ty) {
            Some(Type::Forall(var, _)) => match CString::new(var.as_str()) {
                Ok(cstr) => cstr.into_raw().cast_const(),
                Err(_) => std::ptr::null(),
            },
            _ => std::ptr::null(),
        }
    })
}

/// Check if a type is a type variable (named type like record names).
#[no_mangle]
pub extern "C" fn tg_type_is_tyvar(ty: TypeHandle) -> bool {
    with_arena!(|arena| { matches!(arena.get_type(ty), Some(Type::TyVar(_))) })
}

/// Get the name of a type variable. Returns null pointer if not `TyVar`.
/// The returned string must be freed by the caller.
#[no_mangle]
pub extern "C" fn tg_type_get_tyvar_name(ty: TypeHandle) -> *const c_char {
    use std::ffi::CString;

    with_arena!(|arena| {
        match arena.get_type(ty) {
            Some(Type::TyVar(name)) => match CString::new(name.as_str()) {
                Ok(cstr) => cstr.into_raw().cast_const(),
                Err(_) => std::ptr::null(),
            },
            _ => std::ptr::null(),
        }
    })
}

/// Check if a type is a type application (parametric type like List<T>).
#[no_mangle]
pub extern "C" fn tg_type_is_app(ty: TypeHandle) -> bool {
    with_arena!(|arena| { matches!(arena.get_type(ty), Some(Type::App(_, _))) })
}

/// Get the name of a type application. Returns null pointer if not App.
/// The returned string must be freed by the caller.
#[no_mangle]
pub extern "C" fn tg_type_get_app_name(ty: TypeHandle) -> *const c_char {
    use std::ffi::CString;

    with_arena!(|arena| {
        match arena.get_type(ty) {
            Some(Type::App(name, _)) => match CString::new(name.as_str()) {
                Ok(cstr) => cstr.into_raw().cast_const(),
                Err(_) => std::ptr::null(),
            },
            _ => std::ptr::null(),
        }
    })
}

/// Debug function to print type info to stderr
#[no_mangle]
pub extern "C" fn tg_type_debug(ty: TypeHandle) {
    with_arena!(|arena| {
        match arena.get_type(ty) {
            Some(t) => eprintln!("[FFI DEBUG] handle {ty} = {t:?}"),
            None => eprintln!("[FFI DEBUG] handle {ty} = INVALID"),
        }
    });
}
