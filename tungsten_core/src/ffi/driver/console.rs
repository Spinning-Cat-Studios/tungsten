//! Console output FFI functions

use std::ffi::c_char;
use std::io::{self, Write};

/// Print to stdout (no newline).
///
/// # Safety
/// - `s` must be a valid pointer to `len` bytes
#[no_mangle]
pub extern "C" fn tg_print(s: *const c_char, len: u64) {
    if s.is_null() {
        return;
    }

    let slice = unsafe { std::slice::from_raw_parts(s.cast::<u8>(), len as usize) };
    let _ = io::stdout().write_all(slice);
    let _ = io::stdout().flush();
}

/// Print to stdout with newline.
///
/// # Safety
/// - `s` must be a valid pointer to `len` bytes
#[no_mangle]
pub extern "C" fn tg_println(s: *const c_char, len: u64) {
    if s.is_null() {
        let _ = io::stdout().write_all(b"\n");
        let _ = io::stdout().flush();
        return;
    }

    let slice = unsafe { std::slice::from_raw_parts(s.cast::<u8>(), len as usize) };
    let _ = io::stdout().write_all(slice);
    let _ = io::stdout().write_all(b"\n");
    let _ = io::stdout().flush();
}

/// Print to stderr with newline.
///
/// # Safety
/// - `s` must be a valid pointer to `len` bytes
#[no_mangle]
pub extern "C" fn tg_eprintln(s: *const c_char, len: u64) {
    if s.is_null() {
        let _ = io::stderr().write_all(b"\n");
        let _ = io::stderr().flush();
        return;
    }

    let slice = unsafe { std::slice::from_raw_parts(s.cast::<u8>(), len as usize) };
    let _ = io::stderr().write_all(slice);
    let _ = io::stderr().write_all(b"\n");
    let _ = io::stderr().flush();
}

/// Debug print a u8 value to stderr.
/// Used for tracing tag values in sum types.
#[no_mangle]
pub extern "C" fn tg_debug_tag(label: *const c_char, label_len: u64, tag: u8) {
    let label_slice = if label.is_null() {
        b"tag" as &[u8]
    } else {
        unsafe { std::slice::from_raw_parts(label.cast::<u8>(), label_len as usize) }
    };

    eprintln!(
        "[DEBUG] {}: {}",
        std::str::from_utf8(label_slice).unwrap_or("???"),
        tag
    );
}
