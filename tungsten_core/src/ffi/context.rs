//! Context Operations for FFI
//!
//! This module provides C-compatible functions for managing typing contexts.

use std::ffi::CStr;
use std::os::raw::c_char;

use crate::context::Context;

use super::{with_arena, CtxHandle, TypeHandle, INVALID_HANDLE};

// ============================================================================
// Context Operations
// ============================================================================

/// Create an empty context
#[no_mangle]
pub extern "C" fn tg_ctx_empty() -> CtxHandle {
    with_arena!(|arena| arena.alloc_ctx(Context::new()))
}

/// Extend a context with a term binding: Γ, x : τ
///
/// Returns a new context handle (contexts are immutable).
///
/// # Safety
/// `var_name` must be a valid null-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn tg_ctx_extend(
    ctx: CtxHandle,
    var_name: *const c_char,
    ty: TypeHandle,
) -> CtxHandle {
    if var_name.is_null() {
        return INVALID_HANDLE;
    }
    let name_str = match CStr::from_ptr(var_name).to_str() {
        Ok(s) => s,
        Err(_) => return INVALID_HANDLE,
    };

    with_arena!(|arena| {
        let ctx = match arena.get_ctx(ctx) {
            Some(c) => c.clone(),
            None => return INVALID_HANDLE,
        };
        let ty = match arena.get_type(ty) {
            Some(t) => t.clone(),
            None => return INVALID_HANDLE,
        };
        let new_ctx = ctx.with_term(name_str, ty);
        arena.alloc_ctx(new_ctx)
    })
}

/// Look up a variable's type in a context.
///
/// Returns the type handle if found, or `INVALID_HANDLE` if not found.
///
/// # Safety
/// `var_name` must be a valid null-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn tg_ctx_lookup(ctx: CtxHandle, var_name: *const c_char) -> TypeHandle {
    if var_name.is_null() {
        return INVALID_HANDLE;
    }
    let name_str = match CStr::from_ptr(var_name).to_str() {
        Ok(s) => s,
        Err(_) => return INVALID_HANDLE,
    };

    with_arena!(|arena| {
        let ctx = match arena.get_ctx(ctx) {
            Some(c) => c,
            None => return INVALID_HANDLE,
        };
        match ctx.lookup_term(name_str) {
            Some(ty) => arena.alloc_type(ty.clone()),
            None => INVALID_HANDLE,
        }
    })
}
