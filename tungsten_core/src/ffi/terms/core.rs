//! Core Term Constructors for FFI
//!
//! Structural/binding operations: var, lambda, app, let, if, annot, fix, global.
//! Data constructors (zero, succ, pair, inl, inr, etc.) are in core_data.rs.
//!
//! All constructors are O(1) node pushes (ADR 2.7.26a §4): children stay
//! handles, embedded types stay type-node handles.

use std::ffi::CStr;
use std::os::raw::c_char;

use super::nodes::TermNode;
use super::{valid_terms, valid_types};
use crate::ffi::{with_arena, TermHandle, TypeHandle, INVALID_HANDLE};

// ============================================================================
// Essential Term Constructors
// ============================================================================

/// Construct a variable term by de Bruijn index.
///
/// The index is converted to a placeholder name like `$0`, `$1`, etc.
/// The actual binding is resolved by the context at type-check time.
#[no_mangle]
pub extern "C" fn tg_term_var(index: u64) -> TermHandle {
    with_arena!(|arena| {
        // Use de Bruijn-style naming: $0, $1, etc.
        let name = format!("${index}");
        arena.alloc_term_node(TermNode::Var(name))
    })
}

/// Construct a variable term by name.
///
/// This is useful for referencing named bindings directly.
///
/// # Safety
/// `name` must be a valid null-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn tg_term_var_named(name: *const c_char) -> TermHandle {
    if name.is_null() {
        return INVALID_HANDLE;
    }
    let name_str = match CStr::from_ptr(name).to_str() {
        Ok(s) => s,
        Err(_) => return INVALID_HANDLE,
    };
    with_arena!(|arena| arena.alloc_term_node(TermNode::Var(name_str.to_owned())))
}

/// Construct a lambda abstraction: λx:τ. body
///
/// # Safety
/// `var_name` must be a valid null-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn tg_term_lambda(
    var_name: *const c_char,
    ty: TypeHandle,
    body: TermHandle,
) -> TermHandle {
    if var_name.is_null() {
        return INVALID_HANDLE;
    }
    let name_str = match CStr::from_ptr(var_name).to_str() {
        Ok(s) => s,
        Err(_) => return INVALID_HANDLE,
    };

    with_arena!(|arena| {
        if !valid_types(arena, &[ty]) || !valid_terms(arena, &[body]) {
            return INVALID_HANDLE;
        }
        arena.alloc_term_node(TermNode::Lambda(name_str.to_owned(), ty, body))
    })
}

/// Construct an application: t1 t2
#[no_mangle]
pub extern "C" fn tg_term_app(func: TermHandle, arg: TermHandle) -> TermHandle {
    with_arena!(|arena| {
        if !valid_terms(arena, &[func, arg]) {
            return INVALID_HANDLE;
        }
        arena.alloc_term_node(TermNode::App(func, arg))
    })
}

/// Construct a let binding: let x : τ = def in body
///
/// # Safety
/// `var_name` must be a valid null-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn tg_term_let(
    var_name: *const c_char,
    ty: TypeHandle,
    def: TermHandle,
    body: TermHandle,
) -> TermHandle {
    if var_name.is_null() {
        return INVALID_HANDLE;
    }
    let name_str = match CStr::from_ptr(var_name).to_str() {
        Ok(s) => s,
        Err(_) => return INVALID_HANDLE,
    };

    with_arena!(|arena| {
        if !valid_types(arena, &[ty]) || !valid_terms(arena, &[def, body]) {
            return INVALID_HANDLE;
        }
        arena.alloc_term_node(TermNode::Let(name_str.to_owned(), ty, def, body))
    })
}

// ============================================================================
// Extended Term Constructors
// ============================================================================

/// Construct an if-then-else: if cond then `t_then` else `t_else`
#[no_mangle]
pub extern "C" fn tg_term_if(
    cond: TermHandle,
    t_then: TermHandle,
    t_else: TermHandle,
) -> TermHandle {
    with_arena!(|arena| {
        if !valid_terms(arena, &[cond, t_then, t_else]) {
            return INVALID_HANDLE;
        }
        arena.alloc_term_node(TermNode::If(cond, t_then, t_else))
    })
}

/// Construct type annotation: (t : τ)
#[no_mangle]
pub extern "C" fn tg_term_annot(t: TermHandle, ty: TypeHandle) -> TermHandle {
    with_arena!(|arena| {
        if !valid_terms(arena, &[t]) || !valid_types(arena, &[ty]) {
            return INVALID_HANDLE;
        }
        arena.alloc_term_node(TermNode::Annot(t, ty))
    })
}

/// Construct fix-point: fix f:τ. body
///
/// # Safety
/// `var_name` must be a valid null-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn tg_term_fix(
    var_name: *const c_char,
    ty: TypeHandle,
    body: TermHandle,
) -> TermHandle {
    if var_name.is_null() {
        return INVALID_HANDLE;
    }
    let name_str = match CStr::from_ptr(var_name).to_str() {
        Ok(s) => s,
        Err(_) => return INVALID_HANDLE,
    };

    with_arena!(|arena| {
        if !valid_types(arena, &[ty]) || !valid_terms(arena, &[body]) {
            return INVALID_HANDLE;
        }
        arena.alloc_term_node(TermNode::Fix(name_str.to_owned(), ty, body))
    })
}

/// Construct global reference
///
/// # Safety
/// `name` must be a valid null-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn tg_term_global(name: *const c_char) -> TermHandle {
    if name.is_null() {
        return INVALID_HANDLE;
    }
    let name_str = match CStr::from_ptr(name).to_str() {
        Ok(s) => s,
        Err(_) => return INVALID_HANDLE,
    };
    with_arena!(|arena| arena.alloc_term_node(TermNode::Global(name_str.to_owned())))
}
