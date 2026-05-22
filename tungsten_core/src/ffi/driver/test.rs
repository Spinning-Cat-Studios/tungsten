//! Test runner FFI functions — assertion checking and failure tracking.
//!
//! These functions support `tungsten test`:
//! - `tg_test_begin`: reset failure state for a new test
//! - `tg_assert_eq_nat`: compare two Nat values
//! - `tg_assert_eq_string`: compare two String values (ptr+len)
//! - `tg_assert_eq_bool`: compare two Bool values (0/1)
//! - `tg_test_check_failure`: query whether the current test failed

use std::cell::{Cell, RefCell};
use std::ffi::c_char;

thread_local! {
    static TEST_FAILED: Cell<bool> = const { Cell::new(false) };
    static TEST_NAME: RefCell<String> = const { RefCell::new(String::new()) };
}

/// Reset failure flag for a new test. Stores the test name for diagnostics.
///
/// # Safety
/// - `name` must be a valid pointer to `len` bytes of UTF-8
#[no_mangle]
pub extern "C" fn tg_test_begin(name: *const c_char, len: u64) {
    TEST_FAILED.with(|f| f.set(false));
    if !name.is_null() {
        let slice = unsafe { std::slice::from_raw_parts(name.cast::<u8>(), len as usize) };
        let name_str = std::str::from_utf8(slice).unwrap_or("<invalid utf8>");
        TEST_NAME.with(|n| *n.borrow_mut() = name_str.to_string());
    }
}

/// Compare two Nat values. Sets failure flag and prints diff on mismatch.
#[no_mangle]
pub extern "C" fn tg_assert_eq_nat(left: u64, right: u64) {
    if left != right {
        eprintln!("  assertion failed: assert_eq_nat");
        eprintln!("    left:  {left}");
        eprintln!("    right: {right}");
        TEST_FAILED.with(|f| f.set(true));
    }
}

/// Compare two String values (ptr+len pairs). Sets failure flag on mismatch.
///
/// # Safety
/// - `left` must be a valid pointer to `left_len` bytes
/// - `right` must be a valid pointer to `right_len` bytes
#[no_mangle]
pub extern "C" fn tg_assert_eq_string(
    left: *const c_char,
    left_len: u64,
    right: *const c_char,
    right_len: u64,
) {
    let sl = if left.is_null() {
        ""
    } else {
        let slice = unsafe { std::slice::from_raw_parts(left.cast::<u8>(), left_len as usize) };
        std::str::from_utf8(slice).unwrap_or("<invalid utf8>")
    };

    let sr = if right.is_null() {
        ""
    } else {
        let slice = unsafe { std::slice::from_raw_parts(right.cast::<u8>(), right_len as usize) };
        std::str::from_utf8(slice).unwrap_or("<invalid utf8>")
    };

    if sl != sr {
        eprintln!("  assertion failed: assert_eq_string");
        eprintln!("    left:  \"{sl}\"");
        eprintln!("    right: \"{sr}\"");
        TEST_FAILED.with(|f| f.set(true));
    }
}

/// Compare two Bool values (encoded as 0=false, 1=true).
/// Sets failure flag and prints diff on mismatch.
#[no_mangle]
pub extern "C" fn tg_assert_eq_bool(left: u64, right: u64) {
    if left != right {
        let dl = if left == 0 { "false" } else { "true" };
        let dr = if right == 0 { "false" } else { "true" };
        eprintln!("  assertion failed: assert_eq_bool");
        eprintln!("    left:  {dl}");
        eprintln!("    right: {dr}");
        TEST_FAILED.with(|f| f.set(true));
    }
}

/// Check whether the current test has failed (1 = failed, 0 = passed).
#[no_mangle]
pub extern "C" fn tg_test_check_failure() -> u64 {
    TEST_FAILED.with(|f| u64::from(f.get()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nat_eq_pass() {
        tg_test_begin(std::ptr::null(), 0);
        tg_assert_eq_nat(42, 42);
        assert_eq!(tg_test_check_failure(), 0);
    }

    #[test]
    fn test_nat_eq_fail() {
        tg_test_begin(std::ptr::null(), 0);
        tg_assert_eq_nat(1, 2);
        assert_eq!(tg_test_check_failure(), 1);
    }

    #[test]
    fn test_begin_resets_failure() {
        tg_test_begin(std::ptr::null(), 0);
        tg_assert_eq_nat(1, 2); // fail
        assert_eq!(tg_test_check_failure(), 1);
        tg_test_begin(std::ptr::null(), 0); // reset
        assert_eq!(tg_test_check_failure(), 0);
    }

    #[test]
    fn test_bool_eq_pass() {
        tg_test_begin(std::ptr::null(), 0);
        tg_assert_eq_bool(1, 1);
        assert_eq!(tg_test_check_failure(), 0);
    }

    #[test]
    fn test_bool_eq_fail() {
        tg_test_begin(std::ptr::null(), 0);
        tg_assert_eq_bool(0, 1);
        assert_eq!(tg_test_check_failure(), 1);
    }

    #[test]
    fn test_string_eq_pass() {
        tg_test_begin(std::ptr::null(), 0);
        let a = b"hello";
        let b = b"hello";
        tg_assert_eq_string(
            a.as_ptr().cast(),
            a.len() as u64,
            b.as_ptr().cast(),
            b.len() as u64,
        );
        assert_eq!(tg_test_check_failure(), 0);
    }

    #[test]
    fn test_string_eq_fail() {
        tg_test_begin(std::ptr::null(), 0);
        let a = b"hello";
        let b = b"world";
        tg_assert_eq_string(
            a.as_ptr().cast(),
            a.len() as u64,
            b.as_ptr().cast(),
            b.len() as u64,
        );
        assert_eq!(tg_test_check_failure(), 1);
    }
}
