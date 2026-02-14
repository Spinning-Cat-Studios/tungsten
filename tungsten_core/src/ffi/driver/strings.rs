//! String helper and conversion FFI functions

use std::ffi::{c_char, CStr, CString};
use std::ptr;

// ============================================================================
// String Helpers (for Tungsten string manipulation)
// ============================================================================

/// Get the length of a string in bytes.
///
/// # Safety
/// - `s` must be a valid pointer to `len` bytes, but we're passed the length
///   so this just returns what we're given (for interface consistency)
#[no_mangle]
pub extern "C" fn tg_string_len(s: *const c_char, len: u64) -> u64 {
    // Tungsten strings are {ptr, len} - this just returns the length
    // This function exists for interface consistency
    let _ = s; // unused, length is passed directly
    len
}

/// Get a character (byte) at index.
///
/// Returns the byte value at index `i`, or 0 if out of bounds.
///
/// # Safety
/// - `s` must be a valid pointer to at least `len` bytes
#[no_mangle]
pub extern "C" fn tg_string_char_at(s: *const c_char, len: u64, i: u64) -> u64 {
    if s.is_null() || i >= len {
        return 0;
    }

    unsafe { *s.add(i as usize) as u64 }
}

/// Drop first n characters (bytes) from string.
///
/// Returns a new string (ptr, len) pair. The returned pointer points into
/// the original string data (no allocation), so do NOT free it separately.
///
/// Returns (ptr + n, len - n) if n < len, or (ptr + len, 0) if n >= len.
///
/// # Safety
/// - `s` must be a valid pointer to at least `len` bytes
#[no_mangle]
pub extern "C" fn tg_string_drop(s: *const c_char, len: u64, n: u64) -> StringSlice {
    if s.is_null() {
        return StringSlice {
            ptr: ptr::null(),
            len: 0,
        };
    }

    if n >= len {
        StringSlice {
            ptr: unsafe { s.add(len as usize) },
            len: 0,
        }
    } else {
        StringSlice {
            ptr: unsafe { s.add(n as usize) },
            len: len - n,
        }
    }
}

/// String slice return type for `tg_string_drop` and `tg_string_slice`
#[repr(C)]
pub struct StringSlice {
    pub ptr: *const c_char,
    pub len: u64,
}

/// Take a slice of a string [start, end).
///
/// Returns a new string (ptr, len) pair. The returned pointer points into
/// the original string data (no allocation), so do NOT free it separately.
///
/// # Safety
/// - `s` must be a valid pointer to at least `len` bytes
/// - `start` and `end_pos` are clamped to valid bounds
#[no_mangle]
pub extern "C" fn tg_string_slice(
    s: *const c_char,
    len: u64,
    start: u64,
    end_pos: u64,
) -> StringSlice {
    if s.is_null() {
        return StringSlice {
            ptr: ptr::null(),
            len: 0,
        };
    }

    // Clamp to valid bounds
    let start = start.min(len);
    let end_pos = end_pos.min(len).max(start);

    StringSlice {
        ptr: unsafe { s.add(start as usize) },
        len: end_pos - start,
    }
}

// ============================================================================
// String Conversion (Tungsten String <-> CString)
// ============================================================================

/// Tungsten string representation: { ptr: *const `c_char`, len: u64 }
#[repr(C)]
#[derive(Clone, Copy)]
pub struct TgString {
    pub ptr: *const c_char,
    pub len: u64,
}

/// Convert a Tungsten String to a null-terminated C string.
///
/// This allocates a new null-terminated copy of the string data.
/// The returned `CString` must be freed with `tg_free_string`.
///
/// # Safety
/// - `s.ptr` must be a valid pointer to at least `s.len` bytes
#[no_mangle]
pub extern "C" fn tg_string_to_cstring(s: TgString) -> *mut c_char {
    if s.ptr.is_null() {
        // Return empty string
        return CString::new("")
            .map(std::ffi::CString::into_raw)
            .unwrap_or(ptr::null_mut());
    }

    let slice = unsafe { std::slice::from_raw_parts(s.ptr.cast::<u8>(), s.len as usize) };

    // Copy into a Vec and create CString (adds null terminator)
    let vec = slice.to_vec();
    CString::new(vec)
        .map(std::ffi::CString::into_raw)
        .unwrap_or_else(|_| {
            // String contained null bytes - this shouldn't happen for valid strings
            ptr::null_mut()
        })
}

/// Convert a null-terminated C string to a Tungsten String.
///
/// This allocates a new copy of the string data (without null terminator).
/// The Tungsten runtime is responsible for managing this memory.
///
/// # Safety
/// - `s` must be a valid null-terminated C string
#[no_mangle]
pub extern "C" fn tg_cstring_to_string(s: *const c_char) -> TgString {
    if s.is_null() {
        return TgString {
            ptr: ptr::null(),
            len: 0,
        };
    }

    let cstr = unsafe { CStr::from_ptr(s) };
    let bytes = cstr.to_bytes(); // without null terminator

    // Allocate new buffer and copy
    let mut vec = bytes.to_vec();
    let len = vec.len() as u64;
    let ptr = vec.as_mut_ptr() as *const c_char;
    std::mem::forget(vec); // Tungsten runtime will manage this memory

    TgString { ptr, len }
}

/// Get the length of a Tungsten String (for FFI consistency).
#[no_mangle]
pub extern "C" fn tg_string_len_internal(s: TgString) -> u64 {
    s.len
}

/// Get character at index from a Tungsten String.
#[no_mangle]
pub extern "C" fn tg_string_char_at_internal(s: TgString, i: u64) -> u64 {
    if s.ptr.is_null() || i >= s.len {
        return 0;
    }
    unsafe { *s.ptr.add(i as usize) as u64 }
}

/// Append a character (byte value) to a Tungsten String.
///
/// This allocates a new string with the character appended.
/// The Tungsten runtime is responsible for managing this memory.
#[no_mangle]
pub extern "C" fn tg_string_append_char(s: TgString, c: u64) -> TgString {
    let new_len = s.len + 1;
    let mut vec = Vec::with_capacity(new_len as usize);

    if !s.ptr.is_null() && s.len > 0 {
        let slice = unsafe { std::slice::from_raw_parts(s.ptr.cast::<u8>(), s.len as usize) };
        vec.extend_from_slice(slice);
    }
    vec.push(c as u8);

    let ptr = vec.as_mut_ptr() as *const c_char;
    std::mem::forget(vec);

    TgString { ptr, len: new_len }
}

/// Compare two C strings for equality.
/// Returns true if both are equal (same content), false otherwise.
/// Handles null pointers safely - two nulls are equal, null != non-null.
#[no_mangle]
pub extern "C" fn tg_cstr_eq(a: *const c_char, b: *const c_char) -> bool {
    if a.is_null() && b.is_null() {
        return true;
    }
    if a.is_null() || b.is_null() {
        return false;
    }

    // SAFETY: Both pointers are non-null
    let a_cstr = unsafe { CStr::from_ptr(a) };
    let b_cstr = unsafe { CStr::from_ptr(b) };
    a_cstr == b_cstr
}
