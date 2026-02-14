//! Driver FFI - File I/O, CLI arguments, console output, and process control
//!
//! This module provides FFI functions for the self-hosted driver (Phase 3D).
//! These functions enable the Tungsten compiler to:
//! - Read and write files
//! - Access CLI arguments
//! - Print to stdout/stderr
//! - Detect TTY for colored output
//! - Exit with a status code
//!
//! ## Memory Ownership
//!
//! Functions returning strings (like `tg_read_file`) use `CString::into_raw()`,
//! which transfers ownership to the caller. The Tungsten side should:
//! 1. Copy the string into a managed Tungsten `String`
//! 2. Call `tg_free_string` to release the Rust allocation
//!
//! ## Error Handling
//!
//! Functions that can fail return null on error. Call `tg_driver_get_last_error`
//! to retrieve the error message.
//!
//! ## CLI Arguments
//!
//! CLI args must be initialized by calling `tg_init_args` from `main()` before
//! any Tungsten code runs. Uses `OnceLock` for safe initialization.

// Allow unsafe code in this FFI module
#![allow(unsafe_code)]

mod cli;
mod console;
mod eval;
mod files;
mod process;
mod strings;

#[cfg(test)]
mod tests;

use std::ffi::{c_char, CString};
use std::ptr;

// Re-export all public FFI functions
pub use cli::*;
pub use console::*;
pub use eval::*;
pub use files::*;
pub use process::*;
pub use strings::*;

// ============================================================================
// Thread-local error storage for driver operations
// ============================================================================

thread_local! {
    static LAST_DRIVER_ERROR: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

pub(crate) fn set_driver_error(msg: impl Into<String>) {
    LAST_DRIVER_ERROR.with(|e| {
        *e.borrow_mut() = msg.into();
    });
}

pub(crate) fn clear_driver_error() {
    LAST_DRIVER_ERROR.with(|e| {
        e.borrow_mut().clear();
    });
}

/// Get the last driver error message.
/// Returns null if no error occurred.
/// The returned string must be freed with `tg_free_string`.
#[no_mangle]
pub extern "C" fn tg_driver_get_last_error() -> *mut c_char {
    LAST_DRIVER_ERROR.with(|e| {
        let err = e.borrow();
        if err.is_empty() {
            ptr::null_mut()
        } else {
            CString::new(err.as_str())
                .map(std::ffi::CString::into_raw)
                .unwrap_or(ptr::null_mut())
        }
    })
}

// ============================================================================
// Memory Management
// ============================================================================

/// Free a string previously returned by an FFI function.
///
/// Must be called exactly once per string returned by:
/// - `tg_read_file`
/// - `tg_list_directory`
/// - `tg_parent_directory`
/// - `tg_path_join`
/// - `tg_argv`
/// - `tg_driver_get_last_error`
///
/// # Safety
/// - `s` must be a pointer returned by one of the above functions
/// - `s` must not have been freed already
/// - `s` may be null (no-op)
#[no_mangle]
pub extern "C" fn tg_free_string(s: *mut c_char) {
    if !s.is_null() {
        unsafe {
            drop(CString::from_raw(s));
        }
    }
}
