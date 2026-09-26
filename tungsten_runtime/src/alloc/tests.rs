//! ADR 14.9.26b AC 2 / AC 8: env parsing, `off` delegating to the platform
//! allocator, `bump` bumping the thread arena, the realloc shim never handing
//! an arena pointer to libc, mode `off` when init never ran, and the aliased
//! profiler symbol no longer self-activating.
//!
//! `ARENA_MODE` and `CHUNK_BYTES` are process globals, so every test that sets
//! or reads them holds `MODE_LOCK` and leaves the mode `Off` on the way out —
//! the state a process in which `__tungsten_arena_init` never ran is in.

use super::*;
use crate::alloc_profile::{alloc_profile_is_active, CLASS_MU, CLASS_STRING};
use std::sync::{Mutex, MutexGuard};

static MODE_LOCK: Mutex<()> = Mutex::new(());

/// Hold the global-mode lock and restore `Off` on drop, even on a panic.
struct ModeGuard(#[allow(dead_code)] MutexGuard<'static, ()>);

impl ModeGuard {
    fn take() -> Self {
        Self(
            MODE_LOCK
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
    }

    /// Set the mode under the lock the guard holds — `&self` is the proof.
    #[allow(clippy::unused_self)]
    fn set(&self, setting: ArenaSetting) {
        apply_arena_setting(setting);
    }
}

impl Drop for ModeGuard {
    fn drop(&mut self) {
        apply_arena_setting(parse_arena_setting(None));
    }
}

const BUMP_1MIB: ArenaSetting = ArenaSetting::Recognised {
    mode: ArenaMode::Bump,
    chunk_bytes: 1024 * 1024,
};

#[test]
fn unset_empty_and_off_parse_as_off() {
    let off = ArenaSetting::Recognised {
        mode: ArenaMode::Off,
        chunk_bytes: DEFAULT_CHUNK_BYTES,
    };
    assert_eq!(parse_arena_setting(None), off);
    assert_eq!(parse_arena_setting(Some("")), off);
    assert_eq!(parse_arena_setting(Some("  ")), off);
    assert_eq!(parse_arena_setting(Some("off")), off);
    assert_eq!(parse_arena_setting(Some(" off ")), off);
}

#[test]
fn bump_parses_with_default_and_explicit_chunk_sizes() {
    assert_eq!(
        parse_arena_setting(Some("bump")),
        ArenaSetting::Recognised {
            mode: ArenaMode::Bump,
            chunk_bytes: DEFAULT_CHUNK_BYTES,
        }
    );
    assert_eq!(
        parse_arena_setting(Some("bump:16")),
        ArenaSetting::Recognised {
            mode: ArenaMode::Bump,
            chunk_bytes: 16 * 1024 * 1024,
        }
    );
    assert_eq!(parse_arena_setting(Some("bump:1")), BUMP_1MIB);
}

#[test]
fn unrecognised_values_are_reported_not_silently_off() {
    for bad in ["on", "bmup", "bump:", "bump:0", "bump:x", "bump16", "arena"] {
        assert_eq!(
            parse_arena_setting(Some(bad)),
            ArenaSetting::Unrecognised,
            "{bad:?}"
        );
    }
}

#[test]
fn mode_is_off_when_init_never_ran() {
    let _guard = ModeGuard::take();
    assert_eq!(arena_mode(), ArenaMode::Off);
    assert_eq!(current_arena_stats().mode, MODE_OFF);
}

/// ADR 18.9.26c AC 2: generated code branches on the exported flag, so it must
/// hold exactly the discriminant `apply_arena_setting` last stored.
#[test]
fn exported_mode_flag_tracks_apply_arena_setting() {
    let guard = ModeGuard::take();
    assert_eq!(ARENA_MODE.load(Ordering::Relaxed), MODE_OFF);
    guard.set(BUMP_1MIB);
    assert_eq!(ARENA_MODE.load(Ordering::Relaxed), MODE_BUMP);
    guard.set(ArenaSetting::Unrecognised);
    assert_eq!(ARENA_MODE.load(Ordering::Relaxed), MODE_OFF);
    guard.set(BUMP_1MIB);
    guard.set(parse_arena_setting(Some("off")));
    assert_eq!(ARENA_MODE.load(Ordering::Relaxed), MODE_OFF);
}

#[test]
fn applying_an_unrecognised_setting_leaves_the_mode_off() {
    let guard = ModeGuard::take();
    guard.set(BUMP_1MIB);
    assert_eq!(arena_mode(), ArenaMode::Bump);
    guard.set(ArenaSetting::Unrecognised);
    assert_eq!(arena_mode(), ArenaMode::Off);
}

#[test]
fn off_delegates_to_the_platform_allocator_and_takes_no_chunk() {
    let _guard = ModeGuard::take();
    let before = current_arena_stats();
    // SAFETY: `malloc` contract; freed below.
    let p = unsafe { __tungsten_alloc(64, CLASS_MU) };
    assert!(!p.is_null());
    assert_eq!(
        current_arena_stats().chunks,
        before.chunks,
        "no arena chunk"
    );
    assert_eq!(current_arena_stats().used, before.used);
    // A libc block is a libc block: `free` is the proof it did not come from
    // the arena (an arena pointer here would corrupt the heap).
    // SAFETY: `p` came from `platform_malloc` in mode `Off`.
    unsafe { libc::free(p) };
}

#[test]
fn bump_hands_out_aligned_blocks_from_the_thread_arena() {
    let guard = ModeGuard::take();
    guard.set(BUMP_1MIB);
    let before = current_arena_stats();
    // SAFETY: arena blocks; never freed.
    let a = unsafe { __tungsten_alloc(24, CLASS_MU) };
    let b = unsafe { __tungsten_alloc(8, CLASS_STRING) };
    assert!(!a.is_null() && !b.is_null());
    assert_eq!(a as usize % 16, 0);
    assert_eq!(b as usize % 16, 0);
    assert_eq!(b as usize - a as usize, 32, "24 rounds up to the next 16");
    let after = current_arena_stats();
    assert_eq!(after.mode, MODE_BUMP);
    assert!(after.chunks >= 1);
    assert_eq!(after.used - before.used, 32 + 8);
    assert!(after.high_water >= after.used);
}

#[test]
fn bump_realloc_grows_the_last_block_in_place_and_copies_otherwise() {
    let guard = ModeGuard::take();
    guard.set(BUMP_1MIB);
    // SAFETY: arena blocks; never freed.
    let block = unsafe { __tungsten_alloc(8, CLASS_STRING) }.cast::<u8>();
    unsafe { block.write_bytes(0x42, 8) };
    let grown = unsafe { __tungsten_realloc(block.cast(), 8, 40) }.cast::<u8>();
    assert_eq!(grown, block, "last block: in place");
    let _later = unsafe { __tungsten_alloc(16, CLASS_MU) };
    let moved = unsafe { __tungsten_realloc(grown.cast(), 40, 80) }.cast::<u8>();
    assert_ne!(moved, grown, "no longer last: a copy");
    // SAFETY: the first 8 bytes were written and copied twice.
    assert!(unsafe { std::slice::from_raw_parts(moved, 8) }
        .iter()
        .all(|&x| x == 0x42));
}

#[test]
fn bump_realloc_of_null_is_an_allocation() {
    let guard = ModeGuard::take();
    guard.set(BUMP_1MIB);
    let before = current_arena_stats().used;
    // SAFETY: null is allowed; the block is never freed.
    let p = unsafe { __tungsten_realloc(core::ptr::null_mut(), 0, 24) };
    assert!(!p.is_null());
    assert_eq!(current_arena_stats().used - before, 24);
}

#[test]
fn off_realloc_is_libc_realloc() {
    let _guard = ModeGuard::take();
    // SAFETY: `malloc`/`realloc`/`free` contracts.
    unsafe {
        let p = __tungsten_alloc(4, CLASS_STRING).cast::<u8>();
        p.write_bytes(0x7, 4);
        let q = __tungsten_realloc(p.cast(), 4, 1024).cast::<u8>();
        assert!(!q.is_null());
        assert!(std::slice::from_raw_parts(q, 4).iter().all(|&x| x == 0x7));
        libc::free(q.cast());
    }
}

#[test]
fn zero_byte_requests_are_non_null_in_bump_mode() {
    let guard = ModeGuard::take();
    guard.set(BUMP_1MIB);
    // SAFETY: a zero-byte block.
    let p = unsafe { __tungsten_alloc(0, CLASS_MU) };
    assert!(!p.is_null(), "generated code never null-checks");
}

#[test]
fn reset_returns_every_chunk_but_the_first() {
    let guard = ModeGuard::take();
    guard.set(BUMP_1MIB);
    // Three requests of 3/4 MiB force three chunks of 1 MiB.
    for _ in 0..3 {
        // SAFETY: arena blocks; released by reset.
        assert!(!unsafe { __tungsten_alloc(768 * 1024, CLASS_MU) }.is_null());
    }
    let before = current_arena_stats();
    assert!(before.chunks >= 3);
    __tungsten_arena_reset();
    let after = current_arena_stats();
    assert_eq!(after.chunks, 1);
    assert_eq!(after.used, 0);
    assert_eq!(after.high_water, before.high_water);
}

#[test]
fn arena_stats_writes_through_the_out_pointer_and_ignores_null() {
    let _guard = ModeGuard::take();
    let mut out = ArenaStatsOut {
        mode: 99,
        ..ArenaStatsOut::default()
    };
    // SAFETY: `out` is a valid, writable local.
    unsafe { __tungsten_arena_stats(&mut out) };
    assert_eq!(out.mode, MODE_OFF);
    assert_eq!(out, current_arena_stats());
    // SAFETY: null is documented as a no-op.
    unsafe { __tungsten_arena_stats(core::ptr::null_mut()) };
}

#[test]
fn mode_discriminants_are_the_published_constants() {
    assert_eq!(ArenaMode::Off as u32, MODE_OFF);
    assert_eq!(ArenaMode::Bump as u32, MODE_BUMP);
    assert_ne!(MODE_OFF, MODE_BUMP);
}

/// ADR 14.9.26b AC 8: the aliased profiler symbol records only through the
/// activation gate, so calling it does not switch the profiler on.
#[test]
fn aliased_profiler_symbol_no_longer_self_activates() {
    let _guard = ModeGuard::take();
    assert!(
        !alloc_profile_is_active(),
        "precondition: nothing activated it"
    );
    // SAFETY: `malloc` contract; freed below.
    let p = unsafe { __tungsten_alloc_profile_malloc_class(32, CLASS_MU) };
    let q = unsafe { __tungsten_alloc_profile_malloc(32) };
    assert!(!p.is_null() && !q.is_null());
    assert!(
        !alloc_profile_is_active(),
        "the alias must not activate the profiler as a side effect"
    );
    // SAFETY: both came from `platform_malloc` in mode `Off`.
    unsafe {
        libc::free(p);
        libc::free(q);
    }
}

/// `__tungsten_arena_init` reads `TUNGSTEN_ARENA` from the process
/// environment — the prologue's route. Set under the lock, restored after.
#[test]
fn arena_init_reads_the_environment_variable() {
    let _guard = ModeGuard::take();
    // SAFETY: the environment is process-global; every test that reads the
    // mode holds `MODE_LOCK`, and the variable is removed before the lock drops.
    unsafe { std::env::set_var("TUNGSTEN_ARENA", "bump:2") };
    __tungsten_arena_init();
    unsafe { std::env::remove_var("TUNGSTEN_ARENA") };
    assert_eq!(arena_mode(), ArenaMode::Bump);
    assert_eq!(CHUNK_BYTES.load(Ordering::Relaxed), 2 * 1024 * 1024);
    __tungsten_arena_init();
    assert_eq!(arena_mode(), ArenaMode::Off, "unset reads as off");
}

/// The abort decision: null for a non-zero request fails; null for zero bytes
/// is a legal `malloc(0)` answer; a block never fails.
#[test]
fn allocation_failed_only_for_a_null_non_zero_request() {
    let block = 16usize as *mut u8;
    assert!(allocation_failed(core::ptr::null_mut(), 1));
    assert!(!allocation_failed(core::ptr::null_mut(), 0));
    assert!(!allocation_failed(block, 0));
    assert!(!allocation_failed(block, 1));
}
