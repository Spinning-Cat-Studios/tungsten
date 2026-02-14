//! Tests for driver FFI functions

use std::ffi::{c_char, CStr, CString};

use super::files::{tg_file_exists, tg_is_directory, tg_path_join};
use super::process::{tg_exec_process, tg_getenv, tg_stderr_is_tty, tg_stdout_is_tty};
use super::strings::{
    tg_cstring_to_string, tg_string_append_char, tg_string_drop, tg_string_slice,
    tg_string_to_cstring, TgString,
};
use super::tg_free_string;

#[test]
fn test_file_exists() {
    let path = CString::new("Cargo.toml").unwrap();
    assert_eq!(tg_file_exists(path.as_ptr()), 1);

    let path = CString::new("nonexistent_file_12345.txt").unwrap();
    assert_eq!(tg_file_exists(path.as_ptr()), 0);
}

#[test]
fn test_is_directory() {
    let path = CString::new("src").unwrap();
    assert_eq!(tg_is_directory(path.as_ptr()), 1);

    let path = CString::new("Cargo.toml").unwrap();
    assert_eq!(tg_is_directory(path.as_ptr()), 0);
}

#[test]
fn test_path_join() {
    let base = CString::new("src").unwrap();
    let child = CString::new("lib.rs").unwrap();

    let result = tg_path_join(base.as_ptr(), child.as_ptr());
    assert!(!result.is_null());

    let result_str = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
    assert!(result_str == "src/lib.rs" || result_str == "src\\lib.rs");

    tg_free_string(result);
}

#[test]
fn test_tty_detection() {
    // These should return 0 or 1 without crashing
    let stdout_tty = tg_stdout_is_tty();
    let stderr_tty = tg_stderr_is_tty();
    assert!(stdout_tty == 0 || stdout_tty == 1);
    assert!(stderr_tty == 0 || stderr_tty == 1);
}

#[test]
fn test_string_slice() {
    let s = CString::new("hello world").unwrap();
    let ptr = s.as_ptr();
    let len = 11u64;

    // Slice "world"
    let slice = tg_string_slice(ptr, len, 6, 11);
    assert_eq!(slice.len, 5);

    let slice_str =
        unsafe { std::str::from_utf8(std::slice::from_raw_parts(slice.ptr as *const u8, 5)) }
            .unwrap();
    assert_eq!(slice_str, "world");
}

#[test]
fn test_string_drop() {
    let s = CString::new("hello").unwrap();
    let ptr = s.as_ptr();
    let len = 5u64;

    // Drop first 2 chars
    let slice = tg_string_drop(ptr, len, 2);
    assert_eq!(slice.len, 3);

    let slice_str =
        unsafe { std::str::from_utf8(std::slice::from_raw_parts(slice.ptr as *const u8, 3)) }
            .unwrap();
    assert_eq!(slice_str, "llo");
}

#[test]
fn test_string_conversions() {
    // Create a TgString from raw data
    let data = "hello world";
    let tg_str = TgString {
        ptr: data.as_ptr() as *const c_char,
        len: data.len() as u64,
    };

    // Convert to CString
    let cstring = tg_string_to_cstring(tg_str);
    assert!(!cstring.is_null());

    let cstr = unsafe { CStr::from_ptr(cstring) };
    assert_eq!(cstr.to_str().unwrap(), "hello world");

    // Convert back to TgString
    let tg_str2 = tg_cstring_to_string(cstring);
    assert_eq!(tg_str2.len, 11);

    // Verify contents
    let result_slice =
        unsafe { std::slice::from_raw_parts(tg_str2.ptr as *const u8, tg_str2.len as usize) };
    assert_eq!(result_slice, b"hello world");

    // Clean up
    tg_free_string(cstring);
    // Note: tg_str2.ptr was allocated by tg_cstring_to_string and would
    // normally be managed by Tungsten runtime. For test cleanup:
    unsafe {
        Vec::from_raw_parts(
            tg_str2.ptr as *mut u8,
            tg_str2.len as usize,
            tg_str2.len as usize,
        );
    }
}

#[test]
fn test_string_append_char() {
    let data = "hel";
    let tg_str = TgString {
        ptr: data.as_ptr() as *const c_char,
        len: data.len() as u64,
    };

    // Append 'l'
    let tg_str2 = tg_string_append_char(tg_str, b'l' as u64);
    assert_eq!(tg_str2.len, 4);

    // Append 'o'
    let tg_str3 = tg_string_append_char(tg_str2, b'o' as u64);
    assert_eq!(tg_str3.len, 5);

    // Verify contents
    let result_slice =
        unsafe { std::slice::from_raw_parts(tg_str3.ptr as *const u8, tg_str3.len as usize) };
    assert_eq!(result_slice, b"hello");

    // Clean up
    unsafe {
        Vec::from_raw_parts(
            tg_str2.ptr as *mut u8,
            tg_str2.len as usize,
            tg_str2.len as usize,
        );
        Vec::from_raw_parts(
            tg_str3.ptr as *mut u8,
            tg_str3.len as usize,
            tg_str3.len as usize,
        );
    }
}

// ============================================================================
// Subprocess execution tests
// ============================================================================

#[test]
fn test_exec_process_success() {
    let program = CString::new("echo").unwrap();
    let args = CString::new("hello").unwrap();
    let code = tg_exec_process(program.as_ptr(), args.as_ptr());
    assert_eq!(code, 0);
}

#[test]
fn test_exec_process_failure() {
    let program = CString::new("false").unwrap();
    let args = CString::new("").unwrap();
    let code = tg_exec_process(program.as_ptr(), args.as_ptr());
    assert_ne!(code, 0);
}

#[test]
fn test_exec_process_nonexistent() {
    let program = CString::new("/nonexistent/binary/xxxxx").unwrap();
    let args = CString::new("").unwrap();
    let code = tg_exec_process(program.as_ptr(), args.as_ptr());
    assert_eq!(code, -1);
}

#[test]
fn test_exec_process_multi_args() {
    // echo with multiple args separated by newlines
    let program = CString::new("echo").unwrap();
    let args = CString::new("arg1\narg2\narg3").unwrap();
    let code = tg_exec_process(program.as_ptr(), args.as_ptr());
    assert_eq!(code, 0);
}

#[test]
fn test_exec_process_null_args() {
    let program = CString::new("true").unwrap();
    let code = tg_exec_process(program.as_ptr(), std::ptr::null());
    assert_eq!(code, 0);
}

// ============================================================================
// Environment variable tests
// ============================================================================

#[test]
fn test_getenv_exists() {
    // PATH should always be set
    let name = CString::new("PATH").unwrap();
    let result = tg_getenv(name.as_ptr());
    assert!(!result.is_null());
    // Clean up
    super::tg_free_string(result);
}

#[test]
fn test_getenv_not_set() {
    let name = CString::new("TUNGSTEN_TEST_NONEXISTENT_VAR_12345").unwrap();
    let result = tg_getenv(name.as_ptr());
    assert!(result.is_null());
}
