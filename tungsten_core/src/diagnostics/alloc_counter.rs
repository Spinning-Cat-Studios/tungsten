//! Thread-local allocation-volume counter (ADR 8.7.26a §2.2).
//!
//! [`CountingAllocator`] wraps the system allocator and adds each requested
//! allocation size to a thread-local byte counter. Allocation *volume* — not
//! RSS — is the load-bearing per-unit memory signal (ADR 3.7.26b §2.2: the
//! pathological-unit explosion is transient clone churn, freed immediately),
//! and unlike RSS it attributes correctly under a work-stealing worker pool
//! because each codegen unit runs wholly on one thread.
//!
//! The counter is monotone per thread; callers take deltas around the region
//! they want to attribute (see `compile_unit_at` in the bootstrap binary).
//! Overhead when nobody reads the counter is one thread-local add per
//! allocation call.

// A GlobalAlloc impl is inherently unsafe; same module-level allowance as
// the ffi modules (the workspace denies unsafe_code elsewhere).
#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! {
    // const-initialized so first access never allocates (an allocating TLS
    // initializer inside a global allocator would recurse).
    static THREAD_ALLOCATED_BYTES: Cell<u64> = const { Cell::new(0) };
    static THREAD_DEALLOCATED_BYTES: Cell<u64> = const { Cell::new(0) };
}

/// Total bytes this thread has requested from the allocator so far.
///
/// Monotone (wrapping at `u64::MAX`, unreachable in practice); take deltas
/// with `wrapping_sub` to attribute a region.
pub fn thread_allocated_bytes() -> u64 {
    // try_with: TLS is unavailable during thread teardown; report the last
    // observable state (0) rather than panicking inside the allocator's user.
    THREAD_ALLOCATED_BYTES.try_with(Cell::get).unwrap_or(0)
}

/// Total bytes this thread has explicitly freed so far (realloc shrinkage is
/// not counted). `thread_allocated_bytes() - thread_deallocated_bytes()` over
/// a region approximates its net retention — a leak signal to complement the
/// allocation-volume (churn) signal.
pub fn thread_deallocated_bytes() -> u64 {
    THREAD_DEALLOCATED_BYTES.try_with(Cell::get).unwrap_or(0)
}

fn record_allocation(size: usize) {
    let _ = THREAD_ALLOCATED_BYTES.try_with(|counter| {
        counter.set(counter.get().wrapping_add(size as u64));
    });
}

fn record_deallocation(size: usize) {
    let _ = THREAD_DEALLOCATED_BYTES.try_with(|counter| {
        counter.set(counter.get().wrapping_add(size as u64));
    });
}

/// System-allocator wrapper that counts requested bytes per thread.
///
/// Registered via `#[global_allocator]` in the bootstrap binary; usable as a
/// plain `GlobalAlloc` value in tests without registration.
pub struct CountingAllocator;

// SAFETY: pure pass-through to `System`; the only added behaviour is a
// thread-local counter update, which never allocates or unwinds.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record_allocation(layout.size());
        System.alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        record_deallocation(layout.size());
        System.dealloc(ptr, layout);
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record_allocation(layout.size());
        System.alloc_zeroed(layout)
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // Count only growth: the retained prefix was already counted when
        // first allocated, so counting `new_size` would double-count it.
        record_allocation(new_size.saturating_sub(layout.size()));
        System.realloc(ptr, layout, new_size)
    }
}

// Tests: alloc_counter_tests.rs
#[cfg(test)]
#[path = "alloc_counter_tests.rs"]
mod alloc_counter_tests;
