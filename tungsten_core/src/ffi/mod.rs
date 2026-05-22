//! FFI (Foreign Function Interface) for Tungsten Core
//!
//! This module provides a C-compatible API for the Tungsten Core calculus,
//! enabling the self-hosted elaborator (written in Tungsten) to construct
//! and type-check Core terms.
//!
//! ## Architecture
//!
//! The FFI uses an **arena-based** design with **index handles** to avoid
//! exposing raw pointers across the FFI boundary. All Terms, Types, and
//! Contexts are stored in a global Arena, and handles (u64 indices) are
//! returned to the caller.
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                         ARENA                               │
//! ├─────────────────────────────────────────────────────────────┤
//! │  terms: Vec<Term>     handles → index into this vec        │
//! │  types: Vec<Type>     handles → index into this vec        │
//! │  ctxs:  Vec<Context>  handles → index into this vec        │
//! │  last_error: String   error message from last failed op    │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Error Handling
//!
//! Functions that can fail (like `tg_typecheck`) return a boolean success flag.
//! On failure, the error message can be retrieved via `tg_get_last_error`.
//!
//! ## Thread Safety
//!
//! The current implementation uses a global mutable Arena and is NOT thread-safe.
//! This is acceptable for Phase 3C where the elaborator runs single-threaded.
//!
//! ## Safety
//!
//! This module contains `unsafe` code for the C FFI boundary. All unsafe blocks
//! are carefully reviewed to ensure memory safety.
//!
//! ## Submodules
//!
//! - [`terms`] - Term constructors (`tg_term_*`)
//! - [`types`] - Type constructors (`tg_type_*`)
//! - [`context`] - Context operations (`tg_ctx_*`)
//! - [`check`] - Type checking and error handling

// Allow unsafe code in this FFI module (workspace denies it by default)
#![allow(unsafe_code)]

mod check;
mod context;
mod driver;
mod terms;
mod types;

#[cfg(test)]
mod tests;

use std::cell::RefCell;

use crate::context::Context;
use crate::terms::Term;
use crate::types::Type;

// Re-export all public FFI functions
pub use check::*;
pub use context::*;
pub use driver::*;

// ============================================================================
// Handle Types
// ============================================================================

/// Handle to a Term in the arena (index into terms vec)
pub type TermHandle = u64;

/// Handle to a Type in the arena (index into types vec)
pub type TypeHandle = u64;

/// Handle to a Context in the arena (index into ctxs vec)
pub type CtxHandle = u64;

/// Invalid handle sentinel value
pub const INVALID_HANDLE: u64 = u64::MAX;

// ============================================================================
// Arena
// ============================================================================

/// The global arena storing all allocated Core objects.
///
/// Uses `RefCell` for interior mutability since we need to mutate through
/// a static reference. NOT thread-safe.
pub(crate) struct Arena {
    /// Allocated terms
    pub terms: Vec<Term>,
    /// Allocated types
    pub types: Vec<Type>,
    /// Allocated contexts
    pub ctxs: Vec<Context>,
    /// Last error message (for error retrieval)
    pub last_error: String,
}

impl Arena {
    pub fn new() -> Self {
        Arena {
            terms: Vec::new(),
            types: Vec::new(),
            ctxs: Vec::new(),
            last_error: String::new(),
        }
    }

    pub fn alloc_term(&mut self, term: Term) -> TermHandle {
        let handle = self.terms.len() as u64;
        self.terms.push(term);
        handle
    }

    pub fn alloc_type(&mut self, ty: Type) -> TypeHandle {
        let handle = self.types.len() as u64;
        self.types.push(ty);
        handle
    }

    pub fn alloc_ctx(&mut self, ctx: Context) -> CtxHandle {
        let handle = self.ctxs.len() as u64;
        self.ctxs.push(ctx);
        handle
    }

    pub fn get_term(&self, handle: TermHandle) -> Option<&Term> {
        self.terms.get(handle as usize)
    }

    pub fn get_type(&self, handle: TypeHandle) -> Option<&Type> {
        self.types.get(handle as usize)
    }

    pub fn get_ctx(&self, handle: CtxHandle) -> Option<&Context> {
        self.ctxs.get(handle as usize)
    }

    pub fn set_error(&mut self, msg: impl Into<String>) {
        self.last_error = msg.into();
    }

    pub fn clear_error(&mut self) {
        self.last_error.clear();
    }
}

// Thread-local arena for single-threaded use
thread_local! {
    pub(crate) static ARENA: RefCell<Arena> = RefCell::new(Arena::new());
}

/// Helper macro to access the arena mutably
macro_rules! with_arena {
    (|$arena:ident| $body:expr) => {
        $crate::ffi::ARENA.with(|cell| {
            let $arena = &mut *cell.borrow_mut();
            $body
        })
    };
}

/// Helper macro for arena read-only access
macro_rules! with_arena_ref {
    (|$arena:ident| $body:expr) => {
        $crate::ffi::ARENA.with(|cell| {
            let $arena = &*cell.borrow();
            $body
        })
    };
}

// Export macros for use in submodules
pub(crate) use with_arena;
pub(crate) use with_arena_ref;

// ============================================================================
// Initialization
// ============================================================================

/// Initialize/reset the FFI arena.
/// Call this at the start of elaboration to ensure a clean state.
#[no_mangle]
pub extern "C" fn tg_init() {
    with_arena!(|arena| {
        *arena = Arena::new();
    });
}

// ============================================================================
// String Conversion
// ============================================================================

use std::ffi::CString;
use std::os::raw::c_char;

/// Convert a Tungsten String (fat pointer: {ptr, len}) to a null-terminated C string.
///
/// This allocates a new null-terminated string. The returned pointer must be
/// freed by calling `tg_string_free` when no longer needed to avoid memory leaks.
/// However, for short-lived FFI calls during elaboration, leaking is acceptable.
///
/// # Safety
/// - `ptr` must be a valid pointer to `len` bytes of UTF-8 data
/// - The returned pointer is valid until freed or the arena is reset
#[no_mangle]
pub unsafe extern "C" fn tg_string_to_cstr(ptr: *const c_char, len: u64) -> *const c_char {
    if ptr.is_null() {
        return std::ptr::null();
    }

    // Create a slice from the Tungsten string data
    let slice = std::slice::from_raw_parts(ptr.cast::<u8>(), len as usize);

    // Convert to a string (assuming valid UTF-8)
    let s = match std::str::from_utf8(slice) {
        Ok(s) => s,
        Err(_) => return std::ptr::null(),
    };

    // Create a null-terminated CString and leak it
    // (In production we'd track these for cleanup, but for elaboration this is fine)
    match CString::new(s) {
        Ok(cstr) => cstr.into_raw().cast_const(),
        Err(_) => std::ptr::null(),
    }
}
