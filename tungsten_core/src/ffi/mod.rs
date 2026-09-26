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
//! │  terms: Vec<TermNode> handles → index into this vec        │
//! │  types: Vec<TypeNode> handles → index into this vec        │
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

pub(crate) mod arena_stats;
mod check;
mod context;
mod driver;
mod evaluator_bridges;
pub(crate) mod positivity;
mod termination;
// `pub(crate)` so the `Int` term tests beside the enum (`terms/int_tests.rs`)
// can drive the five constructors the way the self-host does (ADR 14.9.26c).
pub(crate) mod terms;
// `pub(crate)` rather than private: the evaluator's arena arms call these
// `tg_type_*` symbols directly rather than re-deriving what they do, so the
// evaluated and compiled paths cannot drift (ADR 7.8.26c D5).
pub(crate) mod types;

#[cfg(test)]
mod surface_tests;
#[cfg(test)]
mod term_surface_tests;
#[cfg(test)]
pub(crate) mod test_support;
#[cfg(test)]
mod tests;

use std::cell::RefCell;

use crate::context::Context;

// Re-export all public FFI functions
pub use check::*;
pub use context::*;
pub use driver::*;
// Re-exported by name because `driver` itself is private, so the glob above
// carries the driver's *items* but not its modules (ADR 28.7.26a §2.2).
pub use driver::console_capture;
pub use positivity::*;
pub use termination::*;
// Crate-internal: the safe wrappers the evaluator's arena arms call in place
// of the three `unsafe`/raw-pointer symbols (ADR 7.8.26c D5), and the two the
// builder arms call for the same reason (ADR 14.9.26a).
pub(crate) use evaluator_bridges::{
    cstr_address_of, mu_type_from_cstr, string_at_cstr_address, string_builder_push_text,
    string_builder_take_text,
};

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
///
/// **Retention model (ADR 2.7.26a §3.4/§4):** the vectors are grow-only —
/// nothing is ever freed — so retention must stay O(what was built), never
/// O(construction history). Types are stored as [`types::nodes::TypeNode`]s
/// whose children are *handles*: composing shares children instead of
/// deep-cloning them (the owned-`Type` arena retained O(N·depth) per tree —
/// ~35% of a 31 GiB self-compiled RSS blow-up). Owned `Type` trees exist only
/// transiently at materialize boundaries; likewise terms as
/// [`terms::nodes::TermNode`]s (stage M). Do not add deep-cloning composite
/// paths — compose handles.
pub(crate) struct Arena {
    /// Allocated term nodes (handle-children representation, ADR 2.7.26a §4)
    pub terms: Vec<terms::nodes::TermNode>,
    /// Allocated type nodes (handle-children representation, ADR 2.7.26a §4)
    pub types: Vec<types::nodes::TypeNode>,
    /// Allocated contexts
    pub ctxs: Vec<Context>,
    /// Last error message (for error retrieval)
    pub last_error: String,
    /// Cumulative deep-byte retention counters (ADR 2.7.26a §3.4).
    /// Only accumulated while the allocation profiler is active.
    pub retention: arena_stats::ArenaRetentionStats,
}

impl Arena {
    pub fn new() -> Self {
        Arena {
            terms: Vec::new(),
            types: Vec::new(),
            ctxs: Vec::new(),
            last_error: String::new(),
            retention: arena_stats::ArenaRetentionStats::default(),
        }
    }

    pub fn alloc_term_node(&mut self, node: terms::nodes::TermNode) -> TermHandle {
        if tungsten_runtime::alloc_profile_is_active() {
            self.retention.terms += terms::nodes::node_heap_bytes_term(&node);
        }
        let handle = self.terms.len() as u64;
        self.terms.push(node);
        handle
    }

    pub fn alloc_type_node(&mut self, node: types::nodes::TypeNode) -> TypeHandle {
        if tungsten_runtime::alloc_profile_is_active() {
            self.retention.types += types::nodes::node_heap_bytes(&node);
        }
        let handle = self.types.len() as u64;
        self.types.push(node);
        handle
    }

    pub fn alloc_ctx(&mut self, ctx: Context) -> CtxHandle {
        if tungsten_runtime::alloc_profile_is_active() {
            self.retention.ctxs += arena_stats::deep_ctx_bytes(&ctx);
        }
        let handle = self.ctxs.len() as u64;
        self.ctxs.push(ctx);
        handle
    }

    pub fn get_term_node(&self, handle: TermHandle) -> Option<&terms::nodes::TermNode> {
        self.terms.get(handle as usize)
    }

    pub fn get_type_node(&self, handle: TypeHandle) -> Option<&types::nodes::TypeNode> {
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

/// True iff every handle refers to a live term node (constructor guard).
pub(crate) fn valid_terms(arena: &Arena, handles: &[TermHandle]) -> bool {
    handles.iter().all(|h| arena.get_term_node(*h).is_some())
}

/// True iff every handle refers to a live type node (constructor guard).
pub(crate) fn valid_types(arena: &Arena, handles: &[TypeHandle]) -> bool {
    handles.iter().all(|h| arena.get_type_node(*h).is_some())
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
