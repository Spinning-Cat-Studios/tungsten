//! Core Term Constructors for FFI
//!
//! Structural/binding operations: var, lambda, app, let, if, annot, fix, global.
//! Data constructors (zero, succ, pair, inl, inr, etc.) are in core_data.rs.

use std::ffi::CStr;
use std::os::raw::c_char;

use crate::terms::Term;

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
        arena.alloc_term(Term::Var(name))
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
    with_arena!(|arena| arena.alloc_term(Term::Var(name_str.to_owned())))
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
        let ty = match arena.get_type(ty) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let body = match arena.get_term(body) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::Lambda(name_str.to_owned(), ty, Box::new(body)))
    })
}

/// Construct an application: t1 t2
#[no_mangle]
pub extern "C" fn tg_term_app(func: TermHandle, arg: TermHandle) -> TermHandle {
    with_arena!(|arena| {
        let func = match arena.get_term(func) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let arg = match arena.get_term(arg) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::App(Box::new(func), Box::new(arg)))
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
        let ty = match arena.get_type(ty) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let def = match arena.get_term(def) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let body = match arena.get_term(body) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::Let(
            name_str.to_owned(),
            ty,
            Box::new(def),
            Box::new(body),
        ))
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
        let cond = match arena.get_term(cond) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let t_then = match arena.get_term(t_then) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let t_else = match arena.get_term(t_else) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::If(Box::new(cond), Box::new(t_then), Box::new(t_else)))
    })
}

/// Construct type annotation: (t : τ)
#[no_mangle]
pub extern "C" fn tg_term_annot(t: TermHandle, ty: TypeHandle) -> TermHandle {
    with_arena!(|arena| {
        let t = match arena.get_term(t) {
            Some(term) => term.clone(),
            None => return INVALID_HANDLE,
        };
        let ty = match arena.get_type(ty) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::Annot(Box::new(t), ty))
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
        let ty = match arena.get_type(ty) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let body = match arena.get_term(body) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        arena.alloc_term(Term::Fix(name_str.to_owned(), ty, Box::new(body)))
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
    with_arena!(|arena| arena.alloc_term(Term::Global(name_str.to_owned())))
}
