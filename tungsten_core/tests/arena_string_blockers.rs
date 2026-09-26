//! ADR 14.9.26b AC 6: the two things B1 (region-scoped deallocation) must
//! clear before it can wire `__tungsten_arena_reset` to a program point.
//!
//! An integration test on purpose: it flips the runtime's *process-global*
//! allocation mode to `bump`, and the crate's unit tests `free(3)` every
//! string they make — a bump-mode block reaching `free` would corrupt the
//! heap. A separate test binary is a separate process, so the mode set here
//! reaches nothing else. Every test still takes `MODE_LOCK`, because the
//! harness runs the tests in this binary on parallel threads.

// The subject is the raw-pointer FFI surface; the workspace denies `unsafe`
// by default and the `ffi` module opts in the same way.
#![allow(unsafe_code)]

use std::ffi::c_char;
use std::sync::{Mutex, MutexGuard, PoisonError};

use tungsten_core::ffi::{tg_string_concat_owned, tg_string_substring, TgString};
use tungsten_runtime::{
    __tungsten_alloc, apply_arena_setting, arena_mode, current_arena_stats, parse_arena_setting,
    ArenaMode, ArenaSetting, CLASS_STRING,
};

static MODE_LOCK: Mutex<()> = Mutex::new(());

/// Hold the lock, run in bump mode, and put the mode back to `off` on drop.
struct BumpMode(#[allow(dead_code)] MutexGuard<'static, ()>);

impl BumpMode {
    fn enter() -> Self {
        let guard = MODE_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
        apply_arena_setting(ArenaSetting::Recognised {
            mode: ArenaMode::Bump,
            chunk_bytes: 1024 * 1024,
        });
        assert_eq!(arena_mode(), ArenaMode::Bump);
        Self(guard)
    }
}

impl Drop for BumpMode {
    fn drop(&mut self) {
        apply_arena_setting(parse_arena_setting(None));
    }
}

/// A Tungsten string whose buffer is an arena block, as compiled code would
/// produce one in bump mode.
fn arena_string(text: &str) -> TgString {
    // SAFETY: an arena block of `text.len()` bytes, written once, never freed.
    let buf = unsafe { __tungsten_alloc(text.len() as u64, CLASS_STRING) }.cast::<u8>();
    assert!(!buf.is_null());
    unsafe { std::ptr::copy_nonoverlapping(text.as_ptr(), buf, text.len()) };
    TgString {
        ptr: buf as *const c_char,
        len: text.len() as u64,
    }
}

fn read(s: TgString) -> String {
    // SAFETY: `s` is a live string of `s.len` bytes.
    let bytes = unsafe { std::slice::from_raw_parts(s.ptr.cast::<u8>(), s.len as usize) };
    String::from_utf8_lossy(bytes).into_owned()
}

/// Blocker 1a — the append shape. The left buffer is the thread's most recent
/// allocation, so the owned-left concat grows it **in place** through
/// `Arena::grow_last`: same pointer, and the arena's `used` moves by exactly
/// the right operand's length. libc `realloc` never sees the pointer — if it
/// had, `used` would not move (libc knows nothing of the arena) and, on a
/// real heap, the call would be undefined behaviour.
#[test]
fn owned_left_concat_grows_the_last_arena_block_in_place() {
    let _bump = BumpMode::enter();
    // Allocate the right operand first, so the left buffer is the most recent
    // block when the concat runs.
    let right = arena_string("world");
    let left = arena_string("hello ");
    let used_before = current_arena_stats().used;
    let result = tg_string_concat_owned(left, right);
    assert_eq!(read(result), "hello world");
    assert_eq!(result.ptr, left.ptr, "grown in place");
    assert_eq!(
        current_arena_stats().used - used_before,
        right.len,
        "the arena accounted for exactly the growth"
    );
}

/// Blocker 1b — the copy shape. The left buffer is *not* the most recent
/// allocation, so `grow_last` allocates a fresh block and copies: the result
/// moves, the arena's `used` grows by the whole new length, and the old left
/// buffer stays reserved. That retained block is what B1 must reclaim, and
/// why `__tungsten_arena_reset` cannot be wired while a chain like this is
/// live.
#[test]
fn owned_left_concat_copies_when_the_left_block_is_not_last() {
    let _bump = BumpMode::enter();
    let left = arena_string("abc");
    // A 16-byte right operand keeps the bump cursor 16-aligned, so the copy
    // below costs exactly the new length and no alignment padding.
    let right = arena_string("0123456789abcdef");
    let used_before = current_arena_stats().used;
    let result = tg_string_concat_owned(left, right);
    assert_eq!(read(result), "abc0123456789abcdef");
    assert_ne!(result.ptr, left.ptr, "the block moved");
    assert_eq!(
        current_arena_stats().used - used_before,
        result.len,
        "a whole new block; the old left buffer is not reclaimed"
    );
}

/// Blocker 2 — `substring` borrows. Its result is an interior pointer into
/// the source buffer, not an allocation: the arena's `used` does not move and
/// the pointer lies inside the source's extent. Freeing the source (which a
/// region reset would do) invalidates every substring taken from it, so B1
/// needs an escape rule for borrowed strings before any bulk free.
#[test]
fn substring_yields_an_interior_pointer_not_an_allocation() {
    let _bump = BumpMode::enter();
    let source = arena_string("interior pointer");
    let used_before = current_arena_stats().used;
    let sub = tg_string_substring(source, 9, 7);
    assert_eq!(read(sub), "pointer");
    assert_eq!(current_arena_stats().used, used_before, "no allocation");
    let start = source.ptr as usize;
    let end = start + source.len as usize;
    let at = sub.ptr as usize;
    assert!(
        at >= start && at + sub.len as usize <= end,
        "the substring lies inside its source's buffer"
    );
    assert_eq!(at - start, 9);
}
