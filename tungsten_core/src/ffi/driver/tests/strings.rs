//! String-representation FFI and SHA-256.

use std::ffi::{c_char, CStr, CString};

use crate::ffi::driver::io::{
    tg_cstring_to_string, tg_sha256, tg_string_append_char, tg_string_drop, tg_string_slice,
    tg_string_to_cstring, TgString,
};
use crate::ffi::driver::tg_free_string;

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

#[test]
fn test_sha256_known_value() {
    // SHA-256 of empty string is well-known
    let data = CString::new("").unwrap();
    let result = tg_sha256(data.as_ptr());
    assert!(!result.is_null());
    let hex = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
    assert_eq!(
        hex,
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    tg_free_string(result);
}

#[test]
fn test_sha256_hello() {
    let data = CString::new("hello").unwrap();
    let result = tg_sha256(data.as_ptr());
    assert!(!result.is_null());
    let hex = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
    assert_eq!(
        hex,
        "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
    );
    assert_eq!(hex.len(), 64);
    tg_free_string(result);
}

#[test]
fn test_sha256_null() {
    let result = tg_sha256(std::ptr::null());
    assert!(result.is_null());
}
