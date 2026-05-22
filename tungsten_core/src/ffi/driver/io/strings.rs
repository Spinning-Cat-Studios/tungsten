//! String manipulation FFI functions for Tungsten strings.
//!
//! Includes: len, char_at, drop, slice, to_cstring, from_cstring, append_char, cstr_eq

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
    let len = bytes.len();

    if len == 0 {
        return TgString {
            ptr: ptr::null(),
            len: 0,
        };
    }

    // Allocate via libc::malloc for allocator consistency (ADR 18.5.26f)
    let buf = unsafe { libc::malloc(len).cast::<u8>() };
    if buf.is_null() {
        std::process::abort();
    }
    unsafe {
        ptr::copy_nonoverlapping(bytes.as_ptr(), buf, len);
    }

    TgString {
        ptr: buf as *const c_char,
        len: len as u64,
    }
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
///
/// Uses libc::malloc for allocation consistency with codegen-emitted
/// string buffers (ADR 18.5.26f allocator discipline).
#[no_mangle]
pub extern "C" fn tg_string_append_char(s: TgString, c: u64) -> TgString {
    let new_len = s.len + 1;

    let buf = unsafe { libc::malloc(new_len as usize).cast::<u8>() };
    if buf.is_null() {
        std::process::abort();
    }

    if !s.ptr.is_null() && s.len > 0 {
        unsafe {
            ptr::copy_nonoverlapping(s.ptr.cast::<u8>(), buf, s.len as usize);
        }
    }
    unsafe {
        *buf.add(s.len as usize) = c as u8;
    }

    TgString {
        ptr: buf as *const c_char,
        len: new_len,
    }
}

/// Parse a null-terminated C string as a decimal natural number.
///
/// Returns the parsed `u64` value, or 0 if the string is null, empty,
/// or contains non-digit characters.
///
/// # Safety
/// - `s` must be a valid null-terminated C string, or null
#[no_mangle]
pub extern "C" fn tg_string_to_nat(s: *const c_char) -> u64 {
    if s.is_null() {
        return 0;
    }

    let cstr = unsafe { CStr::from_ptr(s) };
    let text = match cstr.to_str() {
        Ok(t) => t,
        Err(_) => return 0,
    };
    text.parse::<u64>().unwrap_or(0)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_cstring(s: &str) -> *mut c_char {
        CString::new(s).unwrap().into_raw()
    }

    #[test]
    fn test_string_to_nat_valid() {
        let s = make_cstring("12345");
        assert_eq!(tg_string_to_nat(s), 12345);
        unsafe { drop(CString::from_raw(s)) };
    }

    #[test]
    fn test_string_to_nat_zero() {
        let s = make_cstring("0");
        assert_eq!(tg_string_to_nat(s), 0);
        unsafe { drop(CString::from_raw(s)) };
    }

    #[test]
    fn test_string_to_nat_large() {
        let s = make_cstring("18446744073709551615");
        assert_eq!(tg_string_to_nat(s), u64::MAX);
        unsafe { drop(CString::from_raw(s)) };
    }

    #[test]
    fn test_string_to_nat_null() {
        assert_eq!(tg_string_to_nat(ptr::null()), 0);
    }

    #[test]
    fn test_string_to_nat_empty() {
        let s = make_cstring("");
        assert_eq!(tg_string_to_nat(s), 0);
        unsafe { drop(CString::from_raw(s)) };
    }

    #[test]
    fn test_string_to_nat_non_numeric() {
        let s = make_cstring("abc");
        assert_eq!(tg_string_to_nat(s), 0);
        unsafe { drop(CString::from_raw(s)) };
    }

    #[test]
    fn test_string_to_nat_overflow() {
        let s = make_cstring("99999999999999999999");
        assert_eq!(tg_string_to_nat(s), 0);
        unsafe { drop(CString::from_raw(s)) };
    }

    #[test]
    fn test_string_to_nat_negative() {
        let s = make_cstring("-1");
        assert_eq!(tg_string_to_nat(s), 0);
        unsafe { drop(CString::from_raw(s)) };
    }

    #[test]
    fn test_string_to_nat_leading_whitespace() {
        let s = make_cstring(" 42");
        assert_eq!(tg_string_to_nat(s), 0);
        unsafe { drop(CString::from_raw(s)) };
    }
}
