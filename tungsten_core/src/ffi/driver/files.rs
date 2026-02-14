//! File I/O FFI functions

use std::ffi::{c_char, CStr, CString};
use std::fs;
use std::path::Path;
use std::ptr;

use super::{clear_driver_error, set_driver_error};

/// Read a file and return its contents as a string.
///
/// Returns null pointer on error (check `tg_driver_get_last_error` for details).
/// The returned string must be freed with `tg_free_string`.
///
/// # Safety
/// - `path` must be a valid null-terminated C string
#[no_mangle]
pub extern "C" fn tg_read_file(path: *const c_char) -> *mut c_char {
    clear_driver_error();

    if path.is_null() {
        set_driver_error("path is null");
        return ptr::null_mut();
    }

    let path_str = match unsafe { CStr::from_ptr(path) }.to_str() {
        Ok(s) => s,
        Err(e) => {
            set_driver_error(format!("invalid path encoding: {e}"));
            return ptr::null_mut();
        }
    };

    match fs::read_to_string(path_str) {
        Ok(contents) => CString::new(contents)
            .map(std::ffi::CString::into_raw)
            .unwrap_or_else(|e| {
                set_driver_error(format!("file contains null byte: {e}"));
                ptr::null_mut()
            }),
        Err(e) => {
            set_driver_error(format!("failed to read '{path_str}': {e}"));
            ptr::null_mut()
        }
    }
}

/// Write content to a file.
///
/// Returns 0 on success, -1 on error.
///
/// # Safety
/// - `path` must be a valid null-terminated C string
/// - `content` must be a valid pointer to `len` bytes
#[no_mangle]
pub extern "C" fn tg_write_file(path: *const c_char, content: *const c_char, len: u64) -> i32 {
    clear_driver_error();

    if path.is_null() {
        set_driver_error("path is null");
        return -1;
    }
    if content.is_null() && len > 0 {
        set_driver_error("content is null but len > 0");
        return -1;
    }

    let path_str = match unsafe { CStr::from_ptr(path) }.to_str() {
        Ok(s) => s,
        Err(e) => {
            set_driver_error(format!("invalid path encoding: {e}"));
            return -1;
        }
    };

    let content_slice = if len == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(content.cast::<u8>(), len as usize) }
    };

    match fs::write(path_str, content_slice) {
        Ok(()) => 0,
        Err(e) => {
            set_driver_error(format!("failed to write '{path_str}': {e}"));
            -1
        }
    }
}

/// Check if a file exists.
///
/// Returns 1 if exists, 0 if not.
///
/// # Safety
/// - `path` must be a valid null-terminated C string
#[no_mangle]
pub extern "C" fn tg_file_exists(path: *const c_char) -> i32 {
    if path.is_null() {
        return 0;
    }

    let path_str = match unsafe { CStr::from_ptr(path) }.to_str() {
        Ok(s) => s,
        Err(_) => return 0,
    };

    i32::from(Path::new(path_str).exists())
}

/// Check if a path is a directory.
///
/// Returns 1 if directory, 0 if not (or error).
///
/// # Safety
/// - `path` must be a valid null-terminated C string
#[no_mangle]
pub extern "C" fn tg_is_directory(path: *const c_char) -> i32 {
    if path.is_null() {
        return 0;
    }

    let path_str = match unsafe { CStr::from_ptr(path) }.to_str() {
        Ok(s) => s,
        Err(_) => return 0,
    };

    i32::from(Path::new(path_str).is_dir())
}

/// List directory contents.
///
/// Returns newline-separated list of entry names.
/// Returns null pointer on error.
/// The returned string must be freed with `tg_free_string`.
///
/// # Safety
/// - `path` must be a valid null-terminated C string
#[no_mangle]
pub extern "C" fn tg_list_directory(path: *const c_char) -> *mut c_char {
    clear_driver_error();

    if path.is_null() {
        set_driver_error("path is null");
        return ptr::null_mut();
    }

    let path_str = match unsafe { CStr::from_ptr(path) }.to_str() {
        Ok(s) => s,
        Err(e) => {
            set_driver_error(format!("invalid path encoding: {e}"));
            return ptr::null_mut();
        }
    };

    match fs::read_dir(path_str) {
        Ok(entries) => {
            let names: Vec<String> = entries
                .filter_map(std::result::Result::ok)
                .filter_map(|e| e.file_name().into_string().ok())
                .collect();
            CString::new(names.join("\n"))
                .map(std::ffi::CString::into_raw)
                .unwrap_or_else(|e| {
                    set_driver_error(format!("entry name contains null: {e}"));
                    ptr::null_mut()
                })
        }
        Err(e) => {
            set_driver_error(format!("failed to list '{path_str}': {e}"));
            ptr::null_mut()
        }
    }
}

/// Get parent directory of a path.
///
/// Returns null if path has no parent (is root) or on error.
/// The returned string must be freed with `tg_free_string`.
///
/// # Safety
/// - `path` must be a valid null-terminated C string
#[no_mangle]
pub extern "C" fn tg_parent_directory(path: *const c_char) -> *mut c_char {
    if path.is_null() {
        return ptr::null_mut();
    }

    let path_str = match unsafe { CStr::from_ptr(path) }.to_str() {
        Ok(s) => s,
        Err(_) => return ptr::null_mut(),
    };

    match Path::new(path_str).parent() {
        Some(parent) => {
            let parent_str = parent.to_string_lossy();
            // Return empty string for root paths that return "" as parent
            if parent_str.is_empty() {
                return ptr::null_mut();
            }
            CString::new(parent_str.into_owned())
                .map(std::ffi::CString::into_raw)
                .unwrap_or(ptr::null_mut())
        }
        None => ptr::null_mut(),
    }
}

/// Join two path components.
///
/// The returned string must be freed with `tg_free_string`.
///
/// # Safety
/// - `base` and `child` must be valid null-terminated C strings
#[no_mangle]
pub extern "C" fn tg_path_join(base: *const c_char, child: *const c_char) -> *mut c_char {
    if base.is_null() || child.is_null() {
        return ptr::null_mut();
    }

    let base_str = match unsafe { CStr::from_ptr(base) }.to_str() {
        Ok(s) => s,
        Err(_) => return ptr::null_mut(),
    };

    let child_str = match unsafe { CStr::from_ptr(child) }.to_str() {
        Ok(s) => s,
        Err(_) => return ptr::null_mut(),
    };

    let joined = Path::new(base_str).join(child_str);
    CString::new(joined.to_string_lossy().into_owned())
        .map(std::ffi::CString::into_raw)
        .unwrap_or(ptr::null_mut())
}
