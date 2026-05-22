//! Cache I/O FFI functions for L2 elaboration caching (ADR 19.5.26e).
//!
//! Provides SHA-256 hashing, recursive directory creation, and binary
//! file I/O for the self-hosted compiler's elaboration cache.

use std::ffi::{c_char, CStr, CString};
use std::fs;
use std::path::Path;
use std::ptr;

use sha2::{Digest, Sha256};

use crate::ffi::driver::{clear_driver_error, set_driver_error};

// ============================================================================
// Hashing
// ============================================================================

/// Compute SHA-256 hash of a string, returning the hex-encoded digest.
///
/// Returns null on error. The returned string must be freed with
/// `tg_free_string`.
///
/// # Safety
/// - `data` must be a valid null-terminated C string
#[no_mangle]
pub extern "C" fn tg_sha256(data: *const c_char) -> *mut c_char {
    clear_driver_error();

    if data.is_null() {
        set_driver_error("data is null");
        return ptr::null_mut();
    }

    let data_str = match unsafe { CStr::from_ptr(data) }.to_str() {
        Ok(s) => s,
        Err(e) => {
            set_driver_error(format!("invalid data encoding: {e}"));
            return ptr::null_mut();
        }
    };

    let hash = Sha256::digest(data_str.as_bytes());
    let hex: String = hash.iter().fold(String::with_capacity(64), |mut s, b| {
        use std::fmt::Write;
        let _ = write!(s, "{b:02x}");
        s
    });

    CString::new(hex)
        .map(CString::into_raw)
        .unwrap_or_else(|e| {
            set_driver_error(format!("hash contains null byte: {e}"));
            ptr::null_mut()
        })
}

// ============================================================================
// Directory creation
// ============================================================================

/// Recursively create directories (like `mkdir -p`).
///
/// Returns 0 on success, -1 on error.
///
/// # Safety
/// - `path` must be a valid null-terminated C string
#[no_mangle]
pub extern "C" fn tg_mkdir_p(path: *const c_char) -> i32 {
    clear_driver_error();

    if path.is_null() {
        set_driver_error("path is null");
        return -1;
    }

    let path_str = match unsafe { CStr::from_ptr(path) }.to_str() {
        Ok(s) => s,
        Err(e) => {
            set_driver_error(format!("invalid path encoding: {e}"));
            return -1;
        }
    };

    match fs::create_dir_all(path_str) {
        Ok(()) => 0,
        Err(e) => {
            set_driver_error(format!("failed to create '{path_str}': {e}"));
            -1
        }
    }
}

// ============================================================================
// Binary file I/O
// ============================================================================

/// Read a file as raw bytes. Returns the data via out-pointers.
///
/// On success: `*out_data` points to the buffer, `*out_len` is its length.
/// On failure: `*out_data` is null, `*out_len` is 0, returns -1.
/// Returns 0 on success.
///
/// The returned buffer must be freed with `tg_free_bytes`.
///
/// # Safety
/// - `path` must be a valid null-terminated C string
/// - `out_data` and `out_len` must be valid writable pointers
#[no_mangle]
pub extern "C" fn tg_read_file_bytes(
    path: *const c_char,
    out_data: *mut *mut u8,
    out_len: *mut u64,
) -> i32 {
    clear_driver_error();

    // Initialize out-params to safe defaults
    if !out_data.is_null() {
        unsafe { *out_data = ptr::null_mut() };
    }
    if !out_len.is_null() {
        unsafe { *out_len = 0 };
    }

    if path.is_null() || out_data.is_null() || out_len.is_null() {
        set_driver_error("null argument");
        return -1;
    }

    let path_str = match unsafe { CStr::from_ptr(path) }.to_str() {
        Ok(s) => s,
        Err(e) => {
            set_driver_error(format!("invalid path encoding: {e}"));
            return -1;
        }
    };

    match fs::read(path_str) {
        Ok(bytes) => {
            let len = bytes.len() as u64;
            let boxed = bytes.into_boxed_slice();
            let ptr = Box::into_raw(boxed).cast::<u8>();
            unsafe {
                *out_data = ptr;
                *out_len = len;
            }
            0
        }
        Err(e) => {
            set_driver_error(format!("failed to read '{path_str}': {e}"));
            -1
        }
    }
}

/// Write raw bytes to a file, creating parent directories as needed.
///
/// Returns 0 on success, -1 on error.
///
/// # Safety
/// - `path` must be a valid null-terminated C string
/// - `data` must be a valid pointer to at least `len` bytes
#[no_mangle]
pub extern "C" fn tg_write_file_bytes(path: *const c_char, data: *const u8, len: u64) -> i32 {
    clear_driver_error();

    if path.is_null() {
        set_driver_error("path is null");
        return -1;
    }
    if data.is_null() && len > 0 {
        set_driver_error("data is null but len > 0");
        return -1;
    }

    let path_str = match unsafe { CStr::from_ptr(path) }.to_str() {
        Ok(s) => s,
        Err(e) => {
            set_driver_error(format!("invalid path encoding: {e}"));
            return -1;
        }
    };

    // Create parent directories if they don't exist
    if let Some(parent) = Path::new(path_str).parent() {
        if !parent.as_os_str().is_empty() {
            if let Err(e) = fs::create_dir_all(parent) {
                set_driver_error(format!(
                    "failed to create parent dir '{}': {e}",
                    parent.display()
                ));
                return -1;
            }
        }
    }

    let content = if len == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(data, len as usize) }
    };

    match fs::write(path_str, content) {
        Ok(()) => 0,
        Err(e) => {
            set_driver_error(format!("failed to write '{path_str}': {e}"));
            -1
        }
    }
}

/// Free a byte buffer returned by `tg_read_file_bytes`.
///
/// # Safety
/// - `data` must have been returned by `tg_read_file_bytes`
/// - `len` must match the length returned by `tg_read_file_bytes`
/// - Must be called exactly once per buffer
#[no_mangle]
pub extern "C" fn tg_free_bytes(data: *mut u8, len: u64) {
    if data.is_null() {
        return;
    }
    unsafe {
        let slice = std::slice::from_raw_parts_mut(data, len as usize);
        drop(Box::from_raw(std::ptr::from_mut::<[u8]>(slice)));
    }
}
