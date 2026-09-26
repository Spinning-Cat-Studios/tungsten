//! Reproduction and regression test for the SysV semaphore leak
//! (ADR 18.8.26d P0).
//!
//! On macOS LMDB locks each environment with one SysV semaphore set of two
//! semaphores, keyed by `ftok(<dir>/lock.mdb, 'M')`. SysV sets are
//! kernel-persistent: nothing reclaims them on process exit, so a store
//! dropped without `mdb_env_close` strands its set until reboot. At
//! `kern.sysv.semmns` exhaustion `semget` returns `ENOSPC` and every store
//! open fails with "No space left on device" on a machine with free disk.
//!
//! The census here is scoped to the exact keys this test's own stores derive,
//! not a global count, so concurrent tests (or other processes) allocating
//! sets can neither mask a leak nor fake one.

use std::collections::HashSet;
use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::process::Command;

use crate::sidecar::store::ExperienceStore;

/// Enough stores that a leak is unambiguous, few enough that a failing run
/// contributes negligibly toward the machine-wide semaphore ceiling.
const STORE_COUNT: usize = 5;

/// The SysV key LMDB derives for the environment whose lock file this is —
/// `ftok(path, 'M')`, mirroring `mdb_env_setup_locks`.
#[allow(unsafe_code)]
fn sysv_key_for(lock_file: &Path) -> u32 {
    let path = CString::new(lock_file.as_os_str().as_bytes()).unwrap();
    // SAFETY: `path` is a valid NUL-terminated C string outliving the call.
    let key = unsafe { libc::ftok(path.as_ptr(), 'M' as libc::c_int) };
    assert!(key != -1, "ftok({}) failed", lock_file.display());
    key as u32
}

/// Which of `keys` appear in `ipcs -s` output. Pure — the `ipcs` invocation
/// stays in the caller — so the parsing is testable against in-memory strings.
fn keys_still_allocated(ipcs_output: &str, keys: &HashSet<u32>) -> Vec<u32> {
    ipcs_output
        .lines()
        .filter(|line| line.starts_with("s "))
        .filter_map(|line| line.split_whitespace().nth(2))
        .filter_map(|hex| u32::from_str_radix(hex.trim_start_matches("0x"), 16).ok())
        .filter(|key| keys.contains(key))
        .collect()
}

fn ipcs_snapshot() -> String {
    let out = Command::new("ipcs")
        .arg("-s")
        .output()
        .expect("running ipcs -s");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn dropped_store_releases_its_sysv_semaphore_set() {
    let mut keys = HashSet::new();
    for _ in 0..STORE_COUNT {
        let dir = tempfile::Builder::new()
            .prefix("tungsten-store-test-")
            .tempdir()
            .unwrap();
        let store = ExperienceStore::open(dir.path()).unwrap();
        let key = sysv_key_for(&dir.path().join("lock.mdb"));
        // Non-vacuity: while the store is open its set must be visible, or
        // the ftok mirror and the parser above have drifted from what the
        // kernel reports, and the absence asserted below would be an
        // artifact of that drift rather than evidence of release.
        let live = keys_still_allocated(&ipcs_snapshot(), &HashSet::from([key]));
        assert_eq!(
            live,
            vec![key],
            "open store's semaphore set not visible in `ipcs -s`"
        );
        keys.insert(key);
        // The directory must outlive the close: releasing the set requires
        // an exclusive lock on lock.mdb, so the store drops first.
        drop(store);
        drop(dir);
    }

    let stranded = keys_still_allocated(&ipcs_snapshot(), &keys);
    assert!(
        stranded.is_empty(),
        "{} of {STORE_COUNT} dropped stores left their SysV semaphore set \
         allocated (keys {stranded:08x?}) — the LMDB environment was not \
         closed on drop",
        stranded.len(),
    );
}

#[test]
fn parser_reports_only_the_requested_keys() {
    let output = "IPC status from <running system> as of Tue Aug 18 12:00:00\n\
                  T     ID     KEY        MODE       OWNER    GROUP\n\
                  Semaphores:\n\
                  s 262144 0x4d1135ea --ra-------    chris    staff\n\
                  s 262145 0x4d1135ee --ra-------    chris    staff\n\
                  s 262146 0x0000beef --ra-------    other    staff\n";
    let keys = HashSet::from([0x4d1135ea, 0x0000beef, 0x12345678]);
    let mut found = keys_still_allocated(output, &keys);
    found.sort_unstable();
    assert_eq!(found, vec![0x0000beef, 0x4d1135ea]);
}

#[test]
fn parser_ignores_headers_and_malformed_rows() {
    let output = "Semaphores:\n\
                  s malformed\n\
                  m 4096 0x4d1135ea --rw------- chris staff\n\
                  s 262147 notahexkey --ra------- chris staff\n";
    let keys = HashSet::from([0x4d1135ea]);
    assert!(keys_still_allocated(output, &keys).is_empty());
}
