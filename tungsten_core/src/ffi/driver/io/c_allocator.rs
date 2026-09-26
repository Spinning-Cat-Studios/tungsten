//! The allocator the string FFI allocates through, per target.
//!
//! Every buffer handed across the string FFI boundary is owned by *compiled
//! Tungsten code* — so these allocations must come from the same allocator
//! codegen emits calls to, never from Rust's (ADR 18.5.26f allocator
//! discipline). Since ADR 14.9.26b that allocator is the runtime's one
//! symbol, `__tungsten_alloc(size, class)`, with `__tungsten_realloc` beside
//! it: in mode `off` they are `malloc(3)`/`realloc(3)`, and in mode `bump`
//! they are the thread's arena. Routing the shim through the symbol is what
//! keeps one string chain from mixing arena memory with libc memory — the
//! owned-left concat grows its left buffer through `realloc`, and handing an
//! arena pointer to libc `realloc` is undefined behaviour. That is what these
//! thin wrappers exist to keep honest; they are not a portability layer.
//!
//! `realloc` therefore takes the block's *old* size: an arena has no header to
//! read it from, and the copy path needs it. Both callers know it (the left
//! string's `len`, the builder's `cap`).
//!
//! `wasm32-unknown-unknown` has no C allocator at all: its `libc` exposes no
//! `malloc`/`realloc` (ADR 28.7.26a Phase 0), and the runtime's mode-`off`
//! arm returns null there. The stub arm below is reachable only in principle —
//! the sole callers are `#[no_mangle]` entry points that compiled Tungsten
//! code calls, and emitting that code needs LLVM, which is not on this target.
//! Allocation failure is already each caller's abort path, so a null return
//! degrades along a route they all already handle.

use std::ffi::c_void;

/// Allocate `size` bytes through the runtime allocation symbol, tagged as a
/// string buffer.
///
/// Returns null when the allocation fails, exactly as `malloc(3)` does — in
/// practice the runtime aborts first, on its own out-of-memory line.
///
/// # Safety
///
/// Same contract as `malloc(3)`: the returned pointer is uninitialized, and
/// the caller owns it. In mode `bump` it must never reach `free(3)`.
#[cfg(unix)]
pub(super) unsafe fn malloc(size: usize) -> *mut c_void {
    unsafe { tungsten_runtime::__tungsten_alloc(size as u64, tungsten_runtime::CLASS_STRING) }
}

/// Resize the allocation at `ptr`, currently `old_size` bytes, to `size` bytes.
///
/// Returns null when the reallocation fails, exactly as `realloc(3)` does.
///
/// # Safety
///
/// Same contract as `realloc(3)`: `ptr` must have come from [`malloc`] (or be
/// null) with a size of at least `old_size`, and is invalidated by a
/// successful call.
#[cfg(unix)]
pub(super) unsafe fn realloc(ptr: *mut c_void, old_size: usize, size: usize) -> *mut c_void {
    unsafe { tungsten_runtime::__tungsten_realloc(ptr, old_size as u64, size as u64) }
}

/// Allocation stub for targets with no C allocator — always fails.
///
/// See the module docs for why failing is the right answer here rather than
/// falling back to Rust's allocator: the buffer's eventual `free(3)` would be
/// a cross-allocator free.
///
/// # Safety
///
/// Trivially safe: allocates nothing.
#[cfg(not(unix))]
pub(super) unsafe fn malloc(_size: usize) -> *mut c_void {
    std::ptr::null_mut()
}

/// Reallocation stub for targets with no C allocator — always fails.
///
/// # Safety
///
/// Trivially safe: reallocates nothing and does not read `ptr`.
#[cfg(not(unix))]
pub(super) unsafe fn realloc(_ptr: *mut c_void, _old_size: usize, _size: usize) -> *mut c_void {
    std::ptr::null_mut()
}

#[cfg(all(test, unix))]
mod tests {
    use super::{malloc, realloc};

    /// A successful `malloc` must hand back a writable buffer of the
    /// requested size — the property every string-FFI caller relies on.
    #[test]
    fn malloc_returns_a_writable_buffer() {
        let buf = unsafe { malloc(8) }.cast::<u8>();
        assert!(!buf.is_null(), "malloc(8) should succeed");
        unsafe {
            for i in 0..8 {
                *buf.add(i) = i as u8;
            }
            for i in 0..8 {
                assert_eq!(*buf.add(i), i as u8);
            }
            libc::free(buf.cast());
        }
    }

    /// `realloc` must preserve the existing prefix when it grows a buffer.
    #[test]
    fn realloc_preserves_the_existing_prefix() {
        let buf = unsafe { malloc(4) }.cast::<u8>();
        assert!(!buf.is_null(), "malloc(4) should succeed");
        unsafe {
            for i in 0..4 {
                *buf.add(i) = 0xA0 + i as u8;
            }
            let grown = realloc(buf.cast(), 4, 16).cast::<u8>();
            assert!(!grown.is_null(), "realloc(16) should succeed");
            for i in 0..4 {
                assert_eq!(*grown.add(i), 0xA0 + i as u8);
            }
            libc::free(grown.cast());
        }
    }

    /// `malloc(0)` may return null or a unique pointer; either is a valid
    /// `malloc(3)` answer, so callers must not treat null here as failure
    /// without also checking the length (they all check length first).
    #[test]
    fn malloc_zero_is_not_treated_as_an_error_here() {
        let buf = unsafe { malloc(0) };
        if !buf.is_null() {
            unsafe { libc::free(buf) };
        }
    }

    /// The shim is the runtime symbol, not a private libc call: in a process
    /// whose prologue never ran the mode is `off`, so the block is a libc
    /// block (freeable) and the thread arena takes no chunk for it.
    #[test]
    fn shim_routes_through_the_runtime_symbol_in_mode_off() {
        assert_eq!(
            tungsten_runtime::arena_mode(),
            tungsten_runtime::ArenaMode::Off,
            "precondition: this test binary has no Tungsten prologue"
        );
        let before = tungsten_runtime::current_arena_stats();
        let buf = unsafe { malloc(32) };
        assert!(!buf.is_null());
        let after = tungsten_runtime::current_arena_stats();
        assert_eq!(after.chunks, before.chunks);
        assert_eq!(after.used, before.used);
        unsafe { libc::free(buf) };
    }
}
