//! Tests for the thread-local allocation counter (ADR 8.7.26a §2.2).

// Exercising a GlobalAlloc impl requires unsafe calls (see alloc_counter.rs).
#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout};

use super::{thread_allocated_bytes, CountingAllocator};

fn layout(bytes: usize) -> Layout {
    Layout::from_size_align(bytes, 8).expect("valid layout")
}

#[test]
fn alloc_increments_counter_by_requested_size() {
    let before = thread_allocated_bytes();
    let l = layout(4096);
    unsafe {
        let p = CountingAllocator.alloc(l);
        assert!(!p.is_null());
        CountingAllocator.dealloc(p, l);
    }
    let delta = thread_allocated_bytes().wrapping_sub(before);
    assert!(
        delta >= 4096,
        "counter must grow by at least the requested 4096 bytes, grew {delta}"
    );
}

#[test]
fn dealloc_does_not_decrement_counter() {
    let l = layout(1024);
    let p = unsafe { CountingAllocator.alloc(l) };
    let after_alloc = thread_allocated_bytes();
    unsafe { CountingAllocator.dealloc(p, l) };
    assert_eq!(
        thread_allocated_bytes(),
        after_alloc,
        "the counter tracks allocation volume, not live bytes"
    );
}

#[test]
fn dealloc_increments_the_dealloc_counter_by_freed_size() {
    let l = layout(1024);
    let p = unsafe { CountingAllocator.alloc(l) };
    let before = super::thread_deallocated_bytes();
    unsafe { CountingAllocator.dealloc(p, l) };
    assert_eq!(
        super::thread_deallocated_bytes().wrapping_sub(before),
        1024,
        "dealloc must record exactly the freed 1024 bytes"
    );
}

#[test]
fn alloc_zeroed_increments_counter() {
    let before = thread_allocated_bytes();
    let l = layout(2048);
    unsafe {
        let p = CountingAllocator.alloc_zeroed(l);
        assert!(!p.is_null());
        CountingAllocator.dealloc(p, l);
    }
    assert!(thread_allocated_bytes().wrapping_sub(before) >= 2048);
}

#[test]
fn realloc_counts_only_growth() {
    let l = layout(1000);
    let p = unsafe { CountingAllocator.alloc(l) };
    let before = thread_allocated_bytes();
    let grown = unsafe { CountingAllocator.realloc(p, l, 3000) };
    assert!(!grown.is_null());
    let delta = thread_allocated_bytes().wrapping_sub(before);
    assert_eq!(
        delta, 2000,
        "growth from 1000 to 3000 bytes must count exactly the 2000-byte delta"
    );
    unsafe { CountingAllocator.dealloc(grown, layout(3000)) };
}

#[test]
fn realloc_shrink_counts_nothing() {
    let l = layout(3000);
    let p = unsafe { CountingAllocator.alloc(l) };
    let before = thread_allocated_bytes();
    let shrunk = unsafe { CountingAllocator.realloc(p, l, 100) };
    assert!(!shrunk.is_null());
    assert_eq!(
        thread_allocated_bytes(),
        before,
        "shrinking reallocations must not add allocation volume"
    );
    unsafe { CountingAllocator.dealloc(shrunk, layout(100)) };
}

#[test]
fn counter_is_thread_local() {
    let main_before = thread_allocated_bytes();
    std::thread::spawn(|| {
        let l = layout(8192);
        unsafe {
            let p = CountingAllocator.alloc(l);
            assert!(!p.is_null());
            CountingAllocator.dealloc(p, l);
        }
        assert!(thread_allocated_bytes() >= 8192);
    })
    .join()
    .expect("worker thread");
    assert_eq!(
        thread_allocated_bytes(),
        main_before,
        "another thread's allocations must not appear in this thread's counter"
    );
}
