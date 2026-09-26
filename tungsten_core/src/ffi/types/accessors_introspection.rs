//! Type variable accessors, substitution, and debug utilities.
//!
//! Type variable and application name accessors (tg_type_get_mu_var, etc.),
//! type substitution (tg_type_substitute), and debug/error utilities.
//! Component accessors (tg_type_get_*) are in `accessors.rs`.

use std::ffi::CStr;
use std::os::raw::c_char;

use super::node_substitute::substitute_type_handle;
use super::nodes::{materialize_type, TypeNode};
use crate::ffi::{with_arena, with_arena_ref, TypeHandle, INVALID_HANDLE};

// ============================================================================
// Type Variable and Application Accessors
// ============================================================================

/// Get the variable name of a μ-type. Returns null pointer if not a μ-type.
///
/// The returned string is valid until freed. Caller is responsible for memory.
#[no_mangle]
pub extern "C" fn tg_type_get_mu_var(ty: TypeHandle) -> *const c_char {
    use std::ffi::CString;

    with_arena_ref!(|arena| {
        match arena.get_type_node(ty) {
            Some(TypeNode::Mu(var, _)) => {
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

    with_arena_ref!(|arena| {
        match arena.get_type_node(ty) {
            Some(TypeNode::Forall(var, _)) => match CString::new(var.as_str()) {
                Ok(cstr) => cstr.into_raw().cast_const(),
                Err(_) => std::ptr::null(),
            },
            _ => std::ptr::null(),
        }
    })
}

/// Get the name of a type variable. Returns null pointer if not `TyVar`.
/// The returned string must be freed by the caller.
#[no_mangle]
pub extern "C" fn tg_type_get_tyvar_name(ty: TypeHandle) -> *const c_char {
    use std::ffi::CString;

    with_arena_ref!(|arena| {
        match arena.get_type_node(ty) {
            Some(TypeNode::TyVar(name)) => match CString::new(name.as_str()) {
                Ok(cstr) => cstr.into_raw().cast_const(),
                Err(_) => std::ptr::null(),
            },
            _ => std::ptr::null(),
        }
    })
}

/// Get the name of a type application. Returns null pointer if not App.
/// The returned string must be freed by the caller.
#[no_mangle]
pub extern "C" fn tg_type_get_app_name(ty: TypeHandle) -> *const c_char {
    use std::ffi::CString;

    with_arena_ref!(|arena| {
        match arena.get_type_node(ty) {
            Some(TypeNode::App(name, _)) => match CString::new(name.as_str()) {
                Ok(cstr) => cstr.into_raw().cast_const(),
                Err(_) => std::ptr::null(),
            },
            _ => std::ptr::null(),
        }
    })
}

// ============================================================================
// Type Substitution (Phase 3C-6)
// ============================================================================

/// Substitute a type variable in a type: τ[α := τ']
///
/// Sharing-aware on the node DAG (ADR 2.7.26a §4): only the spine of paths
/// containing α allocates; an unchanged type returns its original handle.
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
        substitute_type_handle(arena, ty, name_str, replacement).unwrap_or(INVALID_HANDLE)
    })
}

// ============================================================================
// Debug & Error Types
// ============================================================================

/// Debug function to print type info to stderr
#[no_mangle]
pub extern "C" fn tg_type_debug(ty: TypeHandle) {
    with_arena_ref!(|arena| {
        match materialize_type(arena, ty) {
            Some(t) => eprintln!("[FFI DEBUG] handle {ty} = {t:?}"),
            None => eprintln!("[FFI DEBUG] handle {ty} = INVALID"),
        }
    });
}

/// Construct a TypeError (poison type) for error cascade prevention.
///
/// TypeError unifies silently with any type — it suppresses secondary error
/// diagnostics without converting an error path into a success path.
#[no_mangle]
pub extern "C" fn tg_type_error() -> TypeHandle {
    with_arena!(|arena| arena.alloc_type_node(TypeNode::Error))
}

/// Check whether a type handle represents a TypeError (poison type).
/// Returns true if the type is `Type::Error`, false otherwise.
#[no_mangle]
pub extern "C" fn tg_is_type_error(ty: TypeHandle) -> bool {
    with_arena_ref!(|arena| { matches!(arena.get_type_node(ty), Some(TypeNode::Error)) })
}
