//! String concatenation FFI functions (libc-based allocation).
//!
//! Two variants:
//! - `tg_string_concat`: fresh allocation, preserves both inputs
//! - `tg_string_concat_owned`: realloc on left buffer (left is consumed)

use std::ffi::c_char;
use std::ptr;

use super::strings::TgString;

// ============================================================================
// String Concatenation (libc-based allocation)
// ============================================================================

/// Concatenate two Tungsten strings.
///
/// Always allocates a fresh buffer and copies both operands.
/// Preserves value semantics — neither input is consumed or modified.
///
/// Uses libc::malloc for allocation consistency with codegen-emitted
/// string buffers. Do NOT use Vec or Rust allocator APIs here.
///
/// # Safety
/// - Both `left.ptr` and `right.ptr` must be valid pointers to at
///   least `left.len` / `right.len` bytes, or null.
#[no_mangle]
pub extern "C" fn tg_string_concat(left: TgString, right: TgString) -> TgString {
    let new_len = left.len + right.len;

    if new_len == 0 {
        return TgString {
            ptr: ptr::null(),
            len: 0,
        };
    }

    let buf = unsafe { libc::malloc(new_len as usize).cast::<u8>() };
    if buf.is_null() {
        std::process::abort();
    }

    let mut offset = 0usize;
    if !left.ptr.is_null() && left.len > 0 {
        unsafe {
            ptr::copy_nonoverlapping(left.ptr.cast::<u8>(), buf, left.len as usize);
        }
        offset = left.len as usize;
    }
    if !right.ptr.is_null() && right.len > 0 {
        unsafe {
            ptr::copy_nonoverlapping(right.ptr.cast::<u8>(), buf.add(offset), right.len as usize);
        }
    }

    TgString {
        ptr: buf as *const c_char,
        len: new_len,
    }
}

/// Concatenate two Tungsten strings, consuming the left operand.
///
/// Uses realloc on the left buffer to avoid copying left's data when
/// the allocator can extend in-place. The left operand is invalidated
/// after this call — the caller must NOT use left.ptr afterward.
///
/// # Safety
/// - `left.ptr` must be a heap-allocated pointer from libc::malloc
///   (NOT a string literal, NOT a slice into another string)
/// - The caller must guarantee `left` is dead after this call
/// - `right.ptr` must be valid for `right.len` bytes, or null
#[no_mangle]
pub extern "C" fn tg_string_concat_owned(left: TgString, right: TgString) -> TgString {
    let new_len = left.len + right.len;

    if left.ptr.is_null() || left.len == 0 {
        return tg_string_concat(left, right);
    }

    let new_ptr =
        unsafe { libc::realloc(left.ptr as *mut libc::c_void, new_len as usize).cast::<u8>() };

    if new_ptr.is_null() {
        std::process::abort();
    }

    if !right.ptr.is_null() && right.len > 0 {
        unsafe {
            ptr::copy_nonoverlapping(
                right.ptr.cast::<u8>(),
                new_ptr.add(left.len as usize),
                right.len as usize,
            );
        }
    }

    TgString {
        ptr: new_ptr as *const c_char,
        len: new_len,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create a TgString from a &str using libc::malloc.
    unsafe fn make_tg_string(s: &str) -> TgString {
        if s.is_empty() {
            return TgString {
                ptr: ptr::null(),
                len: 0,
            };
        }
        let buf = libc::malloc(s.len()) as *mut u8;
        assert!(!buf.is_null());
        ptr::copy_nonoverlapping(s.as_ptr(), buf, s.len());
        TgString {
            ptr: buf as *const c_char,
            len: s.len() as u64,
        }
    }

    /// Helper: read a TgString back to a Rust String for comparison.
    unsafe fn tg_string_to_rust(s: TgString) -> String {
        if s.ptr.is_null() || s.len == 0 {
            return String::new();
        }
        let slice = std::slice::from_raw_parts(s.ptr.cast::<u8>(), s.len as usize);
        String::from_utf8_lossy(slice).to_string()
    }

    #[test]
    fn test_concat_normal() {
        unsafe {
            let left = make_tg_string("hello ");
            let right = make_tg_string("world");
            let result = tg_string_concat(left, right);
            assert_eq!(tg_string_to_rust(result), "hello world");
            assert_eq!(result.len, 11);
            libc::free(left.ptr as *mut libc::c_void);
            libc::free(right.ptr as *mut libc::c_void);
            libc::free(result.ptr as *mut libc::c_void);
        }
    }

    #[test]
    fn test_concat_both_empty() {
        let left = TgString {
            ptr: ptr::null(),
            len: 0,
        };
        let right = TgString {
            ptr: ptr::null(),
            len: 0,
        };
        let result = tg_string_concat(left, right);
        assert!(result.ptr.is_null());
        assert_eq!(result.len, 0);
    }

    #[test]
    fn test_concat_left_empty() {
        unsafe {
            let left = TgString {
                ptr: ptr::null(),
                len: 0,
            };
            let right = make_tg_string("world");
            let result = tg_string_concat(left, right);
            assert_eq!(tg_string_to_rust(result), "world");
            assert_eq!(result.len, 5);
            libc::free(right.ptr as *mut libc::c_void);
            libc::free(result.ptr as *mut libc::c_void);
        }
    }

    #[test]
    fn test_concat_right_empty() {
        unsafe {
            let left = make_tg_string("hello");
            let right = TgString {
                ptr: ptr::null(),
                len: 0,
            };
            let result = tg_string_concat(left, right);
            assert_eq!(tg_string_to_rust(result), "hello");
            assert_eq!(result.len, 5);
            libc::free(left.ptr as *mut libc::c_void);
            libc::free(result.ptr as *mut libc::c_void);
        }
    }

    #[test]
    fn test_concat_preserves_inputs() {
        unsafe {
            let left = make_tg_string("abc");
            let right = make_tg_string("def");
            let result = tg_string_concat(left, right);
            // Original inputs unchanged
            assert_eq!(tg_string_to_rust(left), "abc");
            assert_eq!(tg_string_to_rust(right), "def");
            assert_eq!(tg_string_to_rust(result), "abcdef");
            libc::free(left.ptr as *mut libc::c_void);
            libc::free(right.ptr as *mut libc::c_void);
            libc::free(result.ptr as *mut libc::c_void);
        }
    }

    #[test]
    fn test_concat_large() {
        unsafe {
            let big = "x".repeat(1_000_000);
            let left = make_tg_string(&big);
            let right = make_tg_string(&big);
            let result = tg_string_concat(left, right);
            assert_eq!(result.len, 2_000_000);
            libc::free(left.ptr as *mut libc::c_void);
            libc::free(right.ptr as *mut libc::c_void);
            libc::free(result.ptr as *mut libc::c_void);
        }
    }

    // --- tg_string_concat_owned tests ---

    #[test]
    fn test_concat_owned_normal() {
        unsafe {
            let left = make_tg_string("hello ");
            let right = make_tg_string("world");
            // left is consumed — do NOT use left.ptr after this
            let result = tg_string_concat_owned(left, right);
            assert_eq!(tg_string_to_rust(result), "hello world");
            assert_eq!(result.len, 11);
            libc::free(right.ptr as *mut libc::c_void);
            libc::free(result.ptr as *mut libc::c_void);
        }
    }

    #[test]
    fn test_concat_owned_left_empty_falls_back() {
        unsafe {
            let left = TgString {
                ptr: ptr::null(),
                len: 0,
            };
            let right = make_tg_string("world");
            let result = tg_string_concat_owned(left, right);
            assert_eq!(tg_string_to_rust(result), "world");
            libc::free(right.ptr as *mut libc::c_void);
            libc::free(result.ptr as *mut libc::c_void);
        }
    }

    #[test]
    fn test_concat_owned_right_empty() {
        unsafe {
            let left = make_tg_string("hello");
            let result = tg_string_concat_owned(
                left,
                TgString {
                    ptr: ptr::null(),
                    len: 0,
                },
            );
            assert_eq!(tg_string_to_rust(result), "hello");
            assert_eq!(result.len, 5);
            libc::free(result.ptr as *mut libc::c_void);
        }
    }

    #[test]
    fn test_concat_owned_chained() {
        unsafe {
            // Simulates (a ++ b) ++ c where inner result is consumed
            let a = make_tg_string("aaa");
            let b = make_tg_string("bbb");
            let ab = tg_string_concat_owned(a, b);
            // ab is now consumed by next concat
            let c = make_tg_string("ccc");
            let abc = tg_string_concat_owned(ab, c);
            assert_eq!(tg_string_to_rust(abc), "aaabbbccc");
            assert_eq!(abc.len, 9);
            libc::free(b.ptr as *mut libc::c_void);
            libc::free(c.ptr as *mut libc::c_void);
            libc::free(abc.ptr as *mut libc::c_void);
        }
    }
}
