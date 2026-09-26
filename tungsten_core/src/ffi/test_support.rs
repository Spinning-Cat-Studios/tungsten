//! Shared helpers for the FFI test modules (test builds only).

use std::os::raw::c_char;
use std::path::{Path, PathBuf};

use super::TypeHandle;

/// A temp directory unique to this **process** that removes itself on drop.
///
/// The filesystem FFI fixtures used to name a fixed path
/// (`temp_dir()/tungsten_test_write_file_success`), wipe it, work in it, and
/// wipe it again. Within one process that is fine — libtest runs each test in
/// its own thread and the names are distinct. Across processes it is not, and
/// ADR 31.8.26c is what made that matter: a mutation sweep runs `jobs`
/// simultaneous copies of this very binary, so one copy's opening
/// `remove_dir_all` deletes the fixture another copy is mid-way through
/// asserting on. Two such tests failed under the four-up probe.
///
/// That failure mode is worse than a flake in the sweep: a test that fails
/// from contention is indistinguishable, at the exit code, from a test that
/// detected a mutation, so the mutant is recorded `Caught` and the gate reads
/// uncovered code as covered.
///
/// The pid in the name is therefore the whole point, and the `tag` keeps two
/// tests inside one process apart. `Drop` does the cleanup, so no test carries
/// a trailing `remove_dir_all` that an early `panic!` would skip.
pub(crate) struct ScratchDir {
    path: PathBuf,
}

impl ScratchDir {
    /// A fresh, empty scratch directory for `tag`, unique to this process.
    ///
    /// Wipes any predecessor first, so a crashed earlier run of this same
    /// process id cannot leak stale files into the assertions.
    pub(crate) fn new(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!("tungsten_test_{tag}_{}", std::process::id()));
        assert!(
            path.is_absolute(),
            "a scratch dir must be absolute or its callers write into the CWD; got {}",
            path.display()
        );
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create scratch dir");
        ScratchDir { path }
    }

    /// The directory's path, unique per process — the property the fixtures
    /// depend on, so callers name it rather than reaching through a `Deref`.
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// A path *inside* the scratch directory. Nothing creates it.
    pub(crate) fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// A path under a scratch directory that is deliberately **never created** —
/// the missing-directory fixture. Returned with its guard, because the guard is
/// what keeps a sibling process from ever creating it either.
pub(crate) fn scratch_missing_dir(tag: &str) -> (ScratchDir, PathBuf) {
    let scratch = ScratchDir::new(tag);
    let missing = scratch.join("no-such-dir");
    (scratch, missing)
}

/// Build `∀<name>. <body>` from a C-string binder address.
///
/// Test-only, and here rather than in `evaluator_bridges` for that reason:
/// the evaluator claims `tg_type_get_forall_var`/`_body` — both on the six
/// tests' static path — but never `tg_type_forall`, so a production wrapper
/// would have no production caller. Lives in the FFI module because the
/// workspace allows `unsafe_code` here and nowhere the callers sit.
pub(crate) fn forall_type_from_cstr(name_address: usize, body: TypeHandle) -> TypeHandle {
    // SAFETY: the address came from `cstr_address_of`, which leaks a
    // null-terminated `CString`; `tg_type_forall` null-checks 0 itself.
    unsafe { super::types::constructors::tg_type_forall(name_address as *const c_char, body) }
}

/// Build the type variable `<name>` from a C-string address. Test-only, for
/// the same reason as [`forall_type_from_cstr`].
pub(crate) fn tyvar_type_from_cstr(name_address: usize) -> TypeHandle {
    // SAFETY: as above — a leaked, null-terminated `CString` address.
    unsafe { super::types::constructors::tg_type_var(name_address as *const c_char) }
}

/// A `String` whose capacity provably equals its length, so byte-exact
/// retention assertions can use `text.len()` as the expected capacity
/// (used by the arena-stats and surface test suites).
pub(crate) fn sized_name(text: &str) -> String {
    let mut s = String::from(text);
    s.shrink_to_fit();
    assert_eq!(s.capacity(), text.len());
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pid in the path is the whole reason this type exists (ADR
    /// 31.8.26c): two concurrent copies of this binary must not name the same
    /// fixture directory. Asserted rather than assumed, because the failure it
    /// prevents is silent — a contended test failure reads as a caught mutant.
    #[test]
    fn a_scratch_dir_is_absolute_empty_and_keyed_on_the_process() {
        let dir = ScratchDir::new("selftest_shape");
        assert!(dir.path().is_absolute());
        assert_eq!(
            dir.path().file_name().unwrap().to_str().unwrap(),
            format!("tungsten_test_selftest_shape_{}", std::process::id())
        );
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    /// `Drop` IS the cleanup, so no fixture carries a trailing
    /// `remove_dir_all` that a failing assertion would skip past. Without this
    /// the guard could be emptied and every test would still pass — while the
    /// temp dir filled up and the isolation the type promises quietly lapsed.
    #[test]
    fn dropping_a_scratch_dir_removes_it_and_its_contents() {
        let path = {
            let dir = ScratchDir::new("selftest_drop");
            std::fs::write(dir.join("left-behind.txt"), "content").unwrap();
            dir.path().to_path_buf()
        };
        assert!(!path.exists(), "the directory outlived its guard");
    }

    /// Construction wipes a predecessor, so a crashed earlier run of this same
    /// process id cannot leak stale files into a later run's assertions.
    #[test]
    fn construction_wipes_a_predecessor() {
        let stale = {
            let first = ScratchDir::new("selftest_wipe");
            let stale = first.join("stale.txt");
            std::fs::write(&stale, "left by a crashed run").unwrap();
            std::mem::forget(first); // a run that never dropped
            stale
        };
        let dir = ScratchDir::new("selftest_wipe");
        assert!(!stale.exists(), "a predecessor's contents must be wiped");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    /// The missing-directory fixture must be a path that does NOT exist, under
    /// a guard that does — the shape `tg_write_file`'s failure test needs.
    #[test]
    fn a_missing_dir_fixture_is_absent_under_a_present_guard() {
        let (scratch, missing) = scratch_missing_dir("selftest_missing");
        assert!(scratch.path().is_dir());
        assert!(!missing.exists());
        assert!(missing.starts_with(scratch.path()));
    }
}
