//! Runtime support library for compiled Tungsten programs.
//!
//! Provides:
//! - Stack overflow signal handler (SIGSEGV/SIGBUS detection with diagnostics)
//!
//! This crate is compiled into `tungsten_core`'s cdylib and linked into every
//! compiled Tungsten program. The signal handler is installed from the `main()`
//! prologue via `__tungsten_install_signal_handlers`.
//!
//! See ADR 18.4.26g §5 for design rationale.

// This crate is inherently unsafe: signal handlers, raw libc calls, mmap.
// The workspace uses `deny(unsafe_code)` but we override here.
#![allow(unsafe_code)]
// Clippy lint policy — see ADR 18.5.26h
#![allow(unknown_lints)] // Reason: devcontainer (1.95) and host (1.91) have different lint names
#![allow(clippy::doc_markdown)] // Reason: docs deferred
#![allow(clippy::borrow_as_ptr)] // Reason: FFI/signal handler code uses &x as *const _
#![allow(clippy::uninlined_format_args)] // Reason: cosmetic
#![allow(clippy::cast_precision_loss)] // Reason: intentional f64 for stats
#![allow(clippy::cast_possible_truncation)] // Reason: size casts checked at boundaries
#![allow(clippy::cast_possible_wrap)] // Reason: signal handler constants
#![allow(clippy::large_stack_arrays)] // Reason: fixed-size buffers for signal safety
#![allow(clippy::manual_c_str_literals)] // Reason: explicit c-string construction in signal handler
#![allow(function_casts_as_integer)] // Reason: signal handler function pointer arithmetic

mod alloc_profile;
mod signal_handler;

// Re-export the public entry points.
// These symbols are called from compiled Tungsten main() prologues.
pub use alloc_profile::{
    __tungsten_alloc_profile_malloc, __tungsten_alloc_profile_report,
    __tungsten_alloc_profile_set_filter, __tungsten_alloc_profile_set_fn,
};
pub use signal_handler::__tungsten_install_signal_handlers;
