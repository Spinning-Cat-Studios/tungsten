//! Type Checking and Error Handling for FFI
//!
//! This module provides C-compatible functions for type checking and error retrieval.

use std::ffi::CString;
use std::os::raw::c_char;

use crate::typecheck::{type_of, types_equal, TypeError};

use super::{with_arena, with_arena_ref, CtxHandle, TermHandle, TypeHandle};

// ============================================================================
// Type Checking
// ============================================================================

/// Type-check a term in a context.
///
/// Returns `true` on success (inferred type written to `out_type`),
/// or `false` on failure (error retrievable via `tg_get_last_error`).
///
/// # Safety
/// `out_type` must be a valid pointer to a `TypeHandle`.
#[no_mangle]
pub unsafe extern "C" fn tg_typecheck(
    ctx: CtxHandle,
    term: TermHandle,
    out_type: *mut TypeHandle,
) -> bool {
    if out_type.is_null() {
        return false;
    }

    with_arena!(|arena| {
        let ctx = if let Some(c) = arena.get_ctx(ctx) {
            c.clone()
        } else {
            arena.set_error("Invalid context handle");
            return false;
        };
        let term = if let Some(t) = arena.get_term(term) {
            t.clone()
        } else {
            arena.set_error("Invalid term handle");
            return false;
        };

        match type_of(&ctx, &term) {
            Ok(ty) => {
                arena.clear_error();
                let handle = arena.alloc_type(ty);
                *out_type = handle;
                true
            }
            Err(e) => {
                arena.set_error(format_type_error(&e));
                false
            }
        }
    })
}

/// Check if two types are equal (α-equivalent).
///
/// Returns `true` if the types are equal, `false` otherwise.
/// TypeError (poison) unifies with any type to prevent error cascades.
#[no_mangle]
pub extern "C" fn tg_types_equal(t1: TypeHandle, t2: TypeHandle) -> bool {
    use crate::types::Type;
    with_arena_ref!(|arena| {
        let ty1 = match arena.get_type(t1) {
            Some(t) => t,
            None => {
                return false;
            }
        };
        let ty2 = match arena.get_type(t2) {
            Some(t) => t,
            None => {
                return false;
            }
        };
        // TypeError unifies with anything — suppress secondary mismatches
        if matches!(ty1, Type::Error) || matches!(ty2, Type::Error) {
            return true;
        }
        types_equal(ty1, ty2)
    })
}

// ============================================================================
// Type Display
// ============================================================================

/// Format a type as a human-readable display string.
///
/// Returns a CString pointer (caller owns the memory).
/// Returns null pointer if the handle is invalid.
#[no_mangle]
pub extern "C" fn tg_type_format_display(ty: TypeHandle) -> *const c_char {
    with_arena_ref!(|arena| {
        match arena.get_type(ty) {
            Some(t) => {
                let display = format!("{t}");
                match CString::new(display) {
                    Ok(cstr) => cstr.into_raw().cast_const(),
                    Err(_) => std::ptr::null(),
                }
            }
            None => std::ptr::null(),
        }
    })
}

// ============================================================================
// Error Handling
// ============================================================================

/// Get the length of the last error message.
///
/// Returns 0 if no error.
#[no_mangle]
pub extern "C" fn tg_get_last_error_len() -> usize {
    with_arena_ref!(|arena| arena.last_error.len())
}

/// Get the last error message.
///
/// Copies the error message into the provided buffer.
/// Returns the number of bytes written (excluding null terminator),
/// or 0 if no error or buffer too small.
///
/// # Safety
/// `buf` must point to a buffer of at least `buf_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn tg_get_last_error(buf: *mut c_char, buf_len: usize) -> usize {
    if buf.is_null() || buf_len == 0 {
        return 0;
    }

    with_arena_ref!(|arena| {
        if arena.last_error.is_empty() {
            return 0;
        }

        // Create a CString from the error message
        let c_str = match CString::new(arena.last_error.as_str()) {
            Ok(s) => s,
            Err(_) => return 0,
        };
        let bytes = c_str.as_bytes_with_nul();

        if bytes.len() > buf_len {
            return 0; // Buffer too small
        }

        std::ptr::copy_nonoverlapping(bytes.as_ptr().cast::<c_char>(), buf, bytes.len());
        bytes.len() - 1 // Exclude null terminator from count
    })
}

// ============================================================================
// Helpers
// ============================================================================

/// Format a `TypeError` for display
fn format_type_error(e: &TypeError) -> String {
    format!("{e}")
}
