//! ADR 14.9.26b AC 2: rollover, alignment, `grow_last` in place vs copy, high
//! water, and the failing-source path — all through an injected chunk source,
//! so no test allocates more than a few hundred bytes.

use super::{align_up, can_grow_in_place, next_chunk_size, Arena, ArenaStats};
use std::cell::RefCell;

/// A chunk source that records every request and serves it from libc, so a
/// test can assert how many chunks were taken and how large each was.
struct RecordingSource {
    requests: RefCell<Vec<usize>>,
    /// Requests at or beyond this index fail (return null).
    fail_from: usize,
}

impl RecordingSource {
    fn new() -> Self {
        Self {
            requests: RefCell::new(Vec::new()),
            fail_from: usize::MAX,
        }
    }

    fn failing_after(count: usize) -> Self {
        Self {
            requests: RefCell::new(Vec::new()),
            fail_from: count,
        }
    }

    fn source(&self) -> impl FnMut(usize) -> *mut u8 + '_ {
        move |size| {
            let index = self.requests.borrow().len();
            self.requests.borrow_mut().push(size);
            if index >= self.fail_from {
                return std::ptr::null_mut();
            }
            // SAFETY: `malloc` contract; the block is released by `free_all`.
            unsafe { libc::malloc(size) }.cast::<u8>()
        }
    }

    fn requests(&self) -> Vec<usize> {
        self.requests.borrow().clone()
    }
}

/// Free every chunk an arena still holds so the tests leak nothing, and
/// return how many were freed — the count is asserted by every caller, so a
/// helper that silently frees nothing (or the wrong number) is caught.
fn drop_arena(mut arena: Arena) -> usize {
    let mut freed = 0;
    arena.reset(&mut |base, _len| {
        // SAFETY: every chunk came from `libc::malloc` in `RecordingSource`.
        unsafe { libc::free(base.cast()) };
        freed += 1;
    });
    if let Some(first) = arena.chunks.first() {
        // SAFETY: as above.
        unsafe { libc::free(first.base.cast()) };
        freed += 1;
    }
    arena.chunks.clear();
    freed
}

#[test]
fn align_up_rounds_to_the_next_multiple() {
    assert_eq!(align_up(0, 16), 0);
    assert_eq!(align_up(1, 16), 16);
    assert_eq!(align_up(16, 16), 16);
    assert_eq!(align_up(17, 16), 32);
    assert_eq!(align_up(7, 8), 8);
    assert_eq!(align_up(9, 1), 9);
}

#[test]
fn next_chunk_size_is_the_configured_size_unless_the_request_is_larger() {
    assert_eq!(next_chunk_size(1024, 16), 1024);
    assert_eq!(next_chunk_size(1024, 1024), 1024);
    assert_eq!(next_chunk_size(1024, 1025), 1025);
    assert_eq!(next_chunk_size(0, 48), 48);
}

#[test]
fn can_grow_in_place_requires_last_block_and_room() {
    // Block at 100 of 20 bytes, cursor at 120, chunk ends at 200.
    assert!(can_grow_in_place(100, 20, 50, 120, 200));
    assert!(
        can_grow_in_place(100, 20, 100, 120, 200),
        "exactly to the end fits"
    );
    assert!(
        !can_grow_in_place(100, 20, 101, 120, 200),
        "one past the end does not"
    );
    assert!(
        !can_grow_in_place(100, 20, 50, 130, 200),
        "not the last block"
    );
    assert!(
        can_grow_in_place(100, 20, 10, 120, 200),
        "shrinking in place is a grow"
    );
    assert!(
        !can_grow_in_place(0, 0, 8, 0, 200),
        "a null block never grows in place"
    );
    assert!(
        !can_grow_in_place(usize::MAX - 4, 8, 8, 4, 200),
        "overflow is not room"
    );
}

#[test]
fn first_allocation_takes_one_chunk_of_the_configured_size() {
    let recorder = RecordingSource::new();
    let mut arena = Arena::new();
    let p = arena.alloc_with(24, 16, 256, &mut recorder.source());
    assert!(!p.is_null());
    assert_eq!(recorder.requests(), vec![256]);
    assert_eq!(
        arena.stats(),
        ArenaStats {
            chunks: 1,
            reserved: 256,
            used: 24,
            high_water: 24,
        }
    );
    let held = arena.stats().chunks;
    assert_eq!(drop_arena(arena), held, "every held chunk is freed");
}

#[test]
fn allocations_are_sixteen_byte_aligned_and_padding_counts_as_used() {
    let recorder = RecordingSource::new();
    let mut arena = Arena::new();
    let a = arena.alloc_with(1, 16, 256, &mut recorder.source());
    let b = arena.alloc_with(1, 16, 256, &mut recorder.source());
    let c = arena.alloc_with(8, 8, 256, &mut recorder.source());
    assert_eq!(a as usize % 16, 0);
    assert_eq!(b as usize % 16, 0);
    assert_eq!(b as usize - a as usize, 16, "one byte at align 16 costs 16");
    assert_eq!(c as usize % 8, 0);
    assert_eq!(
        c as usize - b as usize,
        8,
        "1 byte + 7 padding to the next multiple of 8"
    );
    assert_eq!(
        arena.stats().used,
        1 + (15 + 1) + (7 + 8),
        "padding counts as used"
    );
    assert_eq!(recorder.requests().len(), 1, "all three fit one chunk");
    let held = arena.stats().chunks;
    assert_eq!(drop_arena(arena), held, "every held chunk is freed");
}

#[test]
fn rollover_takes_a_new_chunk_when_the_current_cannot_hold_the_request() {
    let recorder = RecordingSource::new();
    let mut arena = Arena::new();
    let first = arena.alloc_with(48, 16, 64, &mut recorder.source());
    let second = arena.alloc_with(32, 16, 64, &mut recorder.source());
    assert!(!first.is_null() && !second.is_null());
    assert_eq!(
        recorder.requests(),
        vec![64, 64],
        "48 + 32 > 64: second chunk"
    );
    assert_eq!(arena.stats().chunks, 2);
    assert_eq!(arena.stats().reserved, 128);
    assert_eq!(arena.stats().used, 80);
    let held = arena.stats().chunks;
    assert_eq!(drop_arena(arena), held, "every held chunk is freed");
}

#[test]
fn an_oversized_request_gets_a_chunk_of_its_own_size() {
    let recorder = RecordingSource::new();
    let mut arena = Arena::new();
    let big = arena.alloc_with(1000, 16, 64, &mut recorder.source());
    assert!(!big.is_null());
    assert_eq!(recorder.requests(), vec![1000]);
    // The oversized chunk is exactly full, so the next small request rolls over.
    let small = arena.alloc_with(8, 16, 64, &mut recorder.source());
    assert!(!small.is_null());
    assert_eq!(recorder.requests(), vec![1000, 64]);
    let held = arena.stats().chunks;
    assert_eq!(drop_arena(arena), held, "every held chunk is freed");
}

#[test]
fn a_zero_byte_request_is_non_null_and_moves_nothing() {
    let recorder = RecordingSource::new();
    let mut arena = Arena::new();
    let a = arena.alloc_with(0, 16, 64, &mut recorder.source());
    let b = arena.alloc_with(0, 16, 64, &mut recorder.source());
    assert!(!a.is_null());
    assert_eq!(a, b);
    assert_eq!(arena.stats().used, 0);
    let held = arena.stats().chunks;
    assert_eq!(drop_arena(arena), held, "every held chunk is freed");
}

#[test]
fn grow_last_extends_the_most_recent_block_in_place() {
    let recorder = RecordingSource::new();
    let mut arena = Arena::new();
    let block = arena.alloc_with(8, 16, 256, &mut recorder.source());
    // SAFETY: fresh block, in bounds.
    unsafe { block.write_bytes(0xAB, 8) };
    // SAFETY: `block` is the last allocation of 8 bytes.
    let grown = unsafe { arena.grow_last(block, 8, 40, 256, &mut recorder.source()) };
    assert_eq!(grown, block, "in place: the pointer does not move");
    assert_eq!(recorder.requests().len(), 1, "no new chunk");
    assert_eq!(arena.stats().used, 40);
    // SAFETY: 8 bytes were written before the grow.
    assert!(unsafe { std::slice::from_raw_parts(grown, 8) }
        .iter()
        .all(|&b| b == 0xAB));
    // A later allocation lands after the grown extent, not inside it.
    let next = arena.alloc_with(8, 16, 256, &mut recorder.source());
    assert!(next as usize >= grown as usize + 40);
    let held = arena.stats().chunks;
    assert_eq!(drop_arena(arena), held, "every held chunk is freed");
}

#[test]
fn grow_last_copies_when_the_block_is_not_the_most_recent() {
    let recorder = RecordingSource::new();
    let mut arena = Arena::new();
    let block = arena.alloc_with(8, 16, 256, &mut recorder.source());
    // SAFETY: fresh block, in bounds.
    unsafe {
        for i in 0..8 {
            *block.add(i) = i as u8;
        }
    }
    let _later = arena.alloc_with(8, 16, 256, &mut recorder.source());
    // SAFETY: `block` holds 8 initialised bytes.
    let grown = unsafe { arena.grow_last(block, 8, 24, 256, &mut recorder.source()) };
    assert_ne!(grown, block, "not last: a copy");
    for i in 0..8 {
        // SAFETY: the prefix was copied.
        assert_eq!(unsafe { *grown.add(i) }, i as u8);
    }
    assert_eq!(
        arena.stats().used,
        8 + (8 + 8) + (8 + 24),
        "the old block is not reclaimed, and each block's padding to 16 counts"
    );
    let held = arena.stats().chunks;
    assert_eq!(drop_arena(arena), held, "every held chunk is freed");
}

#[test]
fn grow_last_copies_when_the_chunk_has_no_room() {
    let recorder = RecordingSource::new();
    let mut arena = Arena::new();
    let block = arena.alloc_with(48, 16, 64, &mut recorder.source());
    // SAFETY: fresh block, in bounds.
    unsafe { block.write_bytes(0x5C, 48) };
    // Last block, but 48 → 80 overruns the 64-byte chunk.
    // SAFETY: `block` holds 48 initialised bytes.
    let grown = unsafe { arena.grow_last(block, 48, 80, 64, &mut recorder.source()) };
    assert_ne!(grown, block);
    assert_eq!(
        recorder.requests(),
        vec![64, 80],
        "a chunk sized to the request"
    );
    // SAFETY: 48 bytes were copied.
    assert!(unsafe { std::slice::from_raw_parts(grown, 48) }
        .iter()
        .all(|&b| b == 0x5C));
    let held = arena.stats().chunks;
    assert_eq!(drop_arena(arena), held, "every held chunk is freed");
}

#[test]
fn grow_last_copies_only_the_shorter_extent_when_shrinking_off_place() {
    let recorder = RecordingSource::new();
    let mut arena = Arena::new();
    let block = arena.alloc_with(16, 16, 256, &mut recorder.source());
    // SAFETY: fresh block, in bounds.
    unsafe { block.write_bytes(0x11, 16) };
    let _later = arena.alloc_with(8, 16, 256, &mut recorder.source());
    // SAFETY: `block` holds 16 initialised bytes.
    let shrunk = unsafe { arena.grow_last(block, 16, 4, 256, &mut recorder.source()) };
    assert_ne!(shrunk, block);
    // SAFETY: 4 bytes were copied.
    assert!(unsafe { std::slice::from_raw_parts(shrunk, 4) }
        .iter()
        .all(|&b| b == 0x11));
    let held = arena.stats().chunks;
    assert_eq!(drop_arena(arena), held, "every held chunk is freed");
}

#[test]
fn high_water_survives_a_reset_and_used_does_not() {
    let recorder = RecordingSource::new();
    let mut arena = Arena::new();
    for _ in 0..5 {
        assert!(!arena
            .alloc_with(32, 16, 64, &mut recorder.source())
            .is_null());
    }
    assert_eq!(arena.stats().chunks, 3, "5 × 32 over 64-byte chunks");
    assert_eq!(arena.stats().high_water, 160);
    let released = RefCell::new(Vec::new());
    arena.reset(&mut |base, len| {
        released.borrow_mut().push(len);
        // SAFETY: every chunk came from `libc::malloc` in `RecordingSource`.
        unsafe { libc::free(base.cast()) };
    });
    assert_eq!(released.borrow().len(), 2, "every chunk but the first");
    assert_eq!(
        arena.stats(),
        ArenaStats {
            chunks: 1,
            reserved: 64,
            used: 0,
            high_water: 160,
        }
    );
    // The first chunk is reused from its base.
    let again = arena.alloc_with(8, 16, 64, &mut recorder.source());
    assert_eq!(again, arena.chunks[0].base);
    assert_eq!(recorder.requests().len(), 3, "no new chunk after reset");
    let held = arena.stats().chunks;
    assert_eq!(drop_arena(arena), held, "every held chunk is freed");
}

#[test]
fn reset_on_an_empty_arena_is_a_no_op() {
    let mut arena = Arena::new();
    let mut calls = 0;
    arena.reset(&mut |_, _| calls += 1);
    assert_eq!(calls, 0);
    assert_eq!(arena.stats(), ArenaStats::default());
}

#[test]
fn a_failing_source_yields_null_and_leaves_the_arena_consistent() {
    let recorder = RecordingSource::failing_after(1);
    let mut arena = Arena::new();
    let ok = arena.alloc_with(32, 16, 64, &mut recorder.source());
    assert!(!ok.is_null());
    let failed = arena.alloc_with(48, 16, 64, &mut recorder.source());
    assert!(failed.is_null(), "the second chunk request fails");
    assert_eq!(recorder.requests(), vec![64, 64]);
    assert_eq!(arena.stats().chunks, 1, "the failed chunk is not recorded");
    assert_eq!(arena.stats().used, 32);
    // The arena still serves what fits the chunk it has.
    let small = arena.alloc_with(16, 16, 64, &mut recorder.source());
    assert!(!small.is_null());
    let held = arena.stats().chunks;
    assert_eq!(drop_arena(arena), held, "every held chunk is freed");
}

#[test]
fn grow_last_reports_a_failing_source_as_null_without_copying() {
    let recorder = RecordingSource::failing_after(1);
    let mut arena = Arena::new();
    let block = arena.alloc_with(48, 16, 64, &mut recorder.source());
    // SAFETY: `block` is the last allocation of 48 bytes.
    let grown = unsafe { arena.grow_last(block, 48, 200, 64, &mut recorder.source()) };
    assert!(grown.is_null());
    assert_eq!(arena.stats().used, 48);
    let held = arena.stats().chunks;
    assert_eq!(drop_arena(arena), held, "every held chunk is freed");
}

#[test]
fn grow_last_of_a_null_block_is_an_allocation() {
    let recorder = RecordingSource::new();
    let mut arena = Arena::new();
    // SAFETY: null is documented as "allocate `new` bytes".
    let block = unsafe { arena.grow_last(std::ptr::null_mut(), 0, 24, 64, &mut recorder.source()) };
    assert!(!block.is_null());
    assert_eq!(recorder.requests(), vec![64]);
    assert_eq!(arena.stats().used, 24);
    let held = arena.stats().chunks;
    assert_eq!(drop_arena(arena), held, "every held chunk is freed");
}

#[test]
fn default_is_the_empty_arena() {
    let arena = Arena::default();
    assert_eq!(arena.stats(), ArenaStats::default());
    assert_eq!(arena.stats(), Arena::new().stats());
}
