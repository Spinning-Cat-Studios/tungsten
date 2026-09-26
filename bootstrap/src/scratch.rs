//! A temp directory that removes itself, for the bootstrap's own scratch.
//!
//! ADR 29.8.26b. The bootstrap held five hand-rolled scratch paths built from
//! the process temp directory: four in tests that created a directory and only
//! ever removed the *file* inside it, so they leaked on every green run, and one
//! in production — the case-sensitivity probe of `doctor check ir
//! self-compile-readiness` — that cleaned up on every reachable path but named a
//! fixed directory, so two concurrent probes shared it and one's cleanup could
//! delete the other's fixture mid-assertion.
//!
//! Both are the shape ADR 18.8.26d converted in `code-health` and
//! `mutant-schemata`: a path with no owner. The remedy is the same one, and it
//! is deliberately available to *production* code here rather than gated behind
//! `#[cfg(test)]`, because one of the five callers is production.
//!
//! Every directory is named `tungsten-bootstrap-<tag>-<pid>`, which is the
//! prefix `make/private/reap-test-scratch.sh` sweeps: a run killed by a signal — the
//! mutation runner's `killpg`, above all — cannot run `Drop`, and no call-site
//! ceremony can change that.

use std::path::{Path, PathBuf};

/// The owned prefix every scratch directory in this crate carries.
///
/// Public so the self-tests can assert the name they are sweeping under is the
/// name the sweeper is told about; `make reap-test-scratch-selftest` reads the
/// citation from the other end.
pub const SCRATCH_PREFIX: &str = "tungsten-bootstrap-";

/// A scratch directory unique to this process, removed when the guard drops.
#[derive(Debug)]
pub struct ScratchDir {
    path: PathBuf,
}

impl ScratchDir {
    /// A fresh, empty scratch directory for `tag`, unique to this process.
    ///
    /// Any predecessor is wiped first, so a run of this same process id that
    /// died before `Drop` cannot leak stale files into these assertions.
    ///
    /// # Panics
    ///
    /// If the directory cannot be created — a caller that cannot get scratch
    /// has nothing useful to fall back on, and a silent empty path would be
    /// written into the working directory instead.
    #[must_use]
    pub fn new(tag: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("{SCRATCH_PREFIX}{tag}-{}", std::process::id()));
        assert!(
            path.is_absolute(),
            "a scratch dir must be absolute or its callers write into the CWD; got {}",
            path.display()
        );
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create bootstrap scratch dir");
        ScratchDir { path }
    }

    /// The directory itself.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// A path *inside* the scratch directory. Nothing is created.
    #[must_use]
    pub fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pid is what keeps two concurrent processes — a mutation sweep runs
    /// several copies of one test binary, and two `doctor` runs are no
    /// different — out of each other's fixtures.
    #[test]
    fn a_scratch_dir_is_absolute_empty_and_keyed_on_the_process() {
        let dir = ScratchDir::new("selftest-shape");
        assert!(dir.path().is_absolute());
        assert_eq!(
            dir.path().file_name().unwrap().to_str().unwrap(),
            format!("tungsten-bootstrap-selftest-shape-{}", std::process::id())
        );
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    /// The name the sweeper is told about must be the name that is created;
    /// otherwise `make reap-test-scratch-selftest`'s citation arm passes while
    /// the sweep matches nothing.
    #[test]
    fn a_scratch_dir_carries_the_swept_prefix() {
        let dir = ScratchDir::new("selftest-prefix");
        assert!(dir
            .path()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with(SCRATCH_PREFIX));
    }

    /// `Drop` IS the cleanup. Without this the guard could be emptied and every
    /// caller would still pass, while the temp directory filled up.
    #[test]
    fn dropping_a_scratch_dir_removes_it_and_its_contents() {
        let path = {
            let dir = ScratchDir::new("selftest-drop");
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
            let first = ScratchDir::new("selftest-wipe");
            let stale = first.join("stale.txt");
            std::fs::write(&stale, "left by a crashed run").unwrap();
            std::mem::forget(first); // a run that never dropped
            stale
        };
        let dir = ScratchDir::new("selftest-wipe");
        assert!(!stale.exists(), "a predecessor's contents must be wiped");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    /// `join` names a path and creates nothing — the missing-file fixture the
    /// bad-input tests need, under a guard that still removes the parent.
    #[test]
    fn join_creates_nothing() {
        let dir = ScratchDir::new("selftest-join");
        let inner = dir.join("no-such-file.tg");
        assert!(!inner.exists());
        assert!(inner.starts_with(dir.path()));
    }
}
