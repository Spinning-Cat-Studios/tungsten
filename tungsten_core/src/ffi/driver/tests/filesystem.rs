//! Filesystem FFI: existence, directories, paths, read/write, removal.
//!
//! Every fixture directory here comes from [`ScratchDir`], which keys the path
//! on the process id. That is not tidiness: these tests are the ones ADR
//! 31.8.26c's concurrency probe caught. A mutation sweep runs `jobs`
//! simultaneous copies of this binary, and the fixed names these fixtures used
//! to share meant one copy's opening `remove_dir_all` deleted the directory a
//! sibling copy was mid-way through asserting on — reported as a failing test,
//! which the mutation gate reads as a *caught* mutant.

use std::ffi::{CStr, CString};

use crate::ffi::driver::io::{
    tg_file_exists, tg_free_bytes, tg_is_directory, tg_list_directory, tg_mkdir_p,
    tg_parent_directory, tg_path_join, tg_read_file, tg_read_file_bytes, tg_remove_file,
    tg_write_file, tg_write_file_bytes,
};
use crate::ffi::driver::tg_free_string;
use crate::ffi::test_support::{scratch_missing_dir, ScratchDir};

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
fn test_mkdir_p_and_binary_roundtrip() {
    // `tg_mkdir_p` must create the directory itself, so the guard owns a
    // parent and the target is a fresh child of it.
    let scratch = ScratchDir::new("cache_ffi");
    let dir = scratch.join("created-by-mkdir-p");

    let dir_cstr = CString::new(dir.to_str().unwrap()).unwrap();
    assert_eq!(tg_mkdir_p(dir_cstr.as_ptr()), 0);
    assert!(dir.exists());

    // Write binary data
    let file_path = dir.join("test.bin");
    let file_cstr = CString::new(file_path.to_str().unwrap()).unwrap();
    let data: Vec<u8> = vec![0x00, 0x01, 0xFF, 0xFE, 0x42];
    assert_eq!(
        tg_write_file_bytes(file_cstr.as_ptr(), data.as_ptr(), data.len() as u64),
        0
    );

    // Read it back
    let mut out_data: *mut u8 = std::ptr::null_mut();
    let mut out_len: u64 = 0;
    assert_eq!(
        tg_read_file_bytes(file_cstr.as_ptr(), &mut out_data, &mut out_len),
        0
    );
    assert!(!out_data.is_null());
    assert_eq!(out_len, 5);

    let read_back = unsafe { std::slice::from_raw_parts(out_data, out_len as usize) };
    assert_eq!(read_back, &[0x00, 0x01, 0xFF, 0xFE, 0x42]);

    tg_free_bytes(out_data, out_len);
}

/// The three outcomes the clean-up funnel depends on (ADR 7.8.26b §2.3).
///
/// The middle one is the load-bearing case: the funnel runs on the path where
/// the harness was never written, so **absent must be success**. If it returned
/// -1 there, every failed-to-write run would print a spurious removal error.
#[test]
fn test_remove_file_present_absent_and_null() {
    let scratch = ScratchDir::new("remove_file_ffi");
    let file_path = scratch.join("victim.tg");
    std::fs::write(&file_path, "unit test").unwrap();
    let file_cstr = CString::new(file_path.to_str().unwrap()).unwrap();

    assert_eq!(tg_remove_file(file_cstr.as_ptr()), 0);
    assert!(!file_path.exists());

    // Already gone — still success, and still gone.
    assert_eq!(tg_remove_file(file_cstr.as_ptr()), 0);
    assert!(!file_path.exists());

    assert_eq!(tg_remove_file(std::ptr::null()), -1);
}

/// A path that is not valid UTF-8 is refused, not passed through.
///
/// The only arm of `tg_remove_file` the other two tests cannot reach: `CStr`
/// accepts the bytes, `to_str` rejects them. Worth pinning because the failure
/// mode if it were dropped is deleting *some other* path.
#[test]
fn test_remove_file_rejects_a_non_utf8_path() {
    let bad = CString::new(vec![0xff_u8, 0xfe_u8]).unwrap();
    assert_eq!(tg_remove_file(bad.as_ptr()), -1);
}

/// A directory is not a file: removal must FAIL rather than recurse.
///
/// `fs::remove_file` on a directory is an error on every supported platform,
/// and this pins that we do not paper over it — a funnel handed a directory
/// path should report, not delete a tree.
#[test]
fn test_remove_file_refuses_a_directory() {
    let scratch = ScratchDir::new("remove_file_dir");
    let dir = scratch.path();

    let dir_cstr = CString::new(dir.to_str().unwrap()).unwrap();
    assert_eq!(tg_remove_file(dir_cstr.as_ptr()), -1);
    assert!(dir.exists());
}

#[test]
fn test_read_file_bytes_nonexistent() {
    let path = CString::new("/tmp/tungsten_test_nonexistent_12345.bin").unwrap();
    let mut out_data: *mut u8 = std::ptr::null_mut();
    let mut out_len: u64 = 0;
    assert_eq!(
        tg_read_file_bytes(path.as_ptr(), &mut out_data, &mut out_len),
        -1
    );
    assert!(out_data.is_null());
    assert_eq!(out_len, 0);
}

// ===========================================================================
// SUCCESS-PATH SENTINELS
//
// The four tests below exist to kill `-> Default::default()` mutants that
// survived a file-scoped sweep (ADR 7.8.26b `/check-adr`). Each of these
// functions signals failure with a sentinel — a null pointer, or -1 — and
// `Default::default()` IS that sentinel. So the tests above, which only ever
// assert the failure path, cannot tell a working function from one that always
// fails. Asserting the SUCCESS path is what distinguishes them.
// ===========================================================================

/// `tg_read_file` returns content, not the null it uses for errors.
#[test]
fn test_read_file_returns_content_on_success() {
    let scratch = ScratchDir::new("read_file_success");
    let path = scratch.join("greeting.txt");
    std::fs::write(&path, "hello ffi").unwrap();

    let cstr = CString::new(path.to_str().unwrap()).unwrap();
    let result = tg_read_file(cstr.as_ptr());
    assert!(
        !result.is_null(),
        "a readable file must not return the error sentinel"
    );
    let text = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
    assert_eq!(text, "hello ffi");
    tg_free_string(result);
}

/// `tg_write_file` reports -1 when the directory does not exist.
///
/// The mirror of the roundtrip test: without a failing case, a function that
/// always returned 0 would pass every other assertion in this file.
#[test]
fn test_write_file_reports_failure_on_a_missing_directory() {
    // The guard owns a real directory; the fixture is a path under a child of
    // it that nothing ever creates — so no sibling process can create it either.
    let (_scratch, path) = scratch_missing_dir("no_such_dir");
    let path = path.join("out.txt");
    let cstr = CString::new(path.to_str().unwrap()).unwrap();
    let content = CString::new("unwritable").unwrap();
    assert_eq!(tg_write_file(cstr.as_ptr(), content.as_ptr(), 10), -1);
    assert!(!path.exists());
}

/// `tg_list_directory` returns a listing, not the null it uses for errors.
#[test]
fn test_list_directory_returns_entries_on_success() {
    let scratch = ScratchDir::new("list_dir_success");
    std::fs::write(scratch.join("alpha.txt"), "a").unwrap();

    let cstr = CString::new(scratch.path().to_str().unwrap()).unwrap();
    let result = tg_list_directory(cstr.as_ptr());
    assert!(
        !result.is_null(),
        "a real directory must not return the error sentinel"
    );
    let listing = unsafe { CStr::from_ptr(result) }
        .to_str()
        .unwrap()
        .to_string();
    assert!(listing.contains("alpha.txt"), "listing was {listing:?}");
    tg_free_string(result);
}

/// `tg_parent_directory` returns the parent, not the null it uses for errors.
///
/// Both directions: a path WITH a parent yields it, and a bare filename — which
/// `Path::parent` reports as the empty string — yields null rather than "".
#[test]
fn test_parent_directory_returns_the_parent_or_null() {
    let nested = CString::new("/tmp/tungsten/deep/file.txt").unwrap();
    let result = tg_parent_directory(nested.as_ptr());
    assert!(
        !result.is_null(),
        "a nested path must not return the error sentinel"
    );
    let parent = unsafe { CStr::from_ptr(result) }.to_str().unwrap();
    assert_eq!(parent, "/tmp/tungsten/deep");
    tg_free_string(result);

    let bare = CString::new("file.txt").unwrap();
    assert!(tg_parent_directory(bare.as_ptr()).is_null());
}

/// `tg_write_file` writes non-empty content and reports 0.
///
/// The success half of the pair above, and the only test that exercises the
/// `content.is_null() && len > 0` guard with a NON-null content pointer — flip
/// that `&&` to `||` and every non-empty write starts failing, which nothing
/// else here would notice. Asserting the bytes landed also pins the
/// `len == 0` branch that chooses between the real slice and an empty one.
#[test]
fn test_write_file_writes_content_and_reports_success() {
    let scratch = ScratchDir::new("write_file_success");
    let path = scratch.join("out.txt");

    let cstr = CString::new(path.to_str().unwrap()).unwrap();
    let content = CString::new("twelve bytes").unwrap();
    assert_eq!(tg_write_file(cstr.as_ptr(), content.as_ptr(), 12), 0);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "twelve bytes");
}
