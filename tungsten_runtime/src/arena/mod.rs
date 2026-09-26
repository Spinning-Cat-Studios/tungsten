//! A bump-pointer, chunked arena (ADR 14.9.26b, F1 of the v2.0 plan).
//!
//! `Arena` owns no allocator: every chunk comes from an injected *chunk
//! source* (`FnMut(usize) -> *mut u8`) and goes back through an injected
//! *release*, so the runtime hands it `malloc(3)`/`free(3)` and a test hands it
//! a counting closure that can drive rollover, alignment and the
//! out-of-memory path without allocating gigabytes. The decisions that shape a
//! request — how large the next chunk is, whether a block can grow where it
//! sits — are free functions over plain integers, so the mutation gate can
//! score them.
//!
//! Nothing here frees a *block*: memory returns in bulk through [`Arena::reset`],
//! which B1 (region-scoped deallocation) wires to a program point. This ADR
//! exports it and wires it nowhere.
//!
//! The word "arena" is already taken twice in this tree — the compiler's handle
//! table (`tungsten_core::ffi::Arena`) and its `[arena]` retention line — and
//! this module accepts the overload rather than coining a fourth word.

/// One block of memory the chunk source handed us.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Chunk {
    base: *mut u8,
    len: usize,
}

/// A snapshot of an arena's bookkeeping, for the profiler's report line and
/// `__tungsten_arena_stats`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ArenaStats {
    /// Chunks currently held.
    pub chunks: usize,
    /// Bytes reserved from the chunk source across every held chunk.
    pub reserved: usize,
    /// Bytes handed out since the last reset (alignment padding included).
    pub used: usize,
    /// The largest `used` ever observed.
    pub high_water: usize,
}

/// A bump arena over injected chunks.
///
/// `const`-constructible so it can sit in a `const {}`-initialised thread-local:
/// a thread-local whose initialiser allocates recurses through the allocator
/// it backs.
#[derive(Debug)]
pub struct Arena {
    chunks: Vec<Chunk>,
    /// Next free byte in the current (last) chunk.
    cur: *mut u8,
    /// One past the last byte of the current chunk.
    end: *mut u8,
    /// Bytes handed out since the last reset.
    used: usize,
    high_water: usize,
}

impl Default for Arena {
    fn default() -> Self {
        Self::new()
    }
}

impl Arena {
    /// An arena holding no chunks; the first allocation requests one.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            chunks: Vec::new(),
            cur: core::ptr::null_mut(),
            end: core::ptr::null_mut(),
            used: 0,
            high_water: 0,
        }
    }

    /// Bump-allocate `size` bytes at `align`, requesting a chunk of
    /// `next_chunk_size(chunk_size, size)` bytes from `source` when the current
    /// one cannot hold the request. Returns null only when `source` does.
    ///
    /// `align` must be at most the source's own alignment (16 for a C
    /// allocator): a chunk's base is assumed aligned, and only the bump offset
    /// is rounded.
    ///
    /// A zero-byte request returns the current aligned cursor without moving
    /// it — a non-null pointer valid for zero bytes, which is what callers
    /// that never check for null need.
    pub fn alloc_with(
        &mut self,
        size: usize,
        align: usize,
        chunk_size: usize,
        source: &mut impl FnMut(usize) -> *mut u8,
    ) -> *mut u8 {
        debug_assert!(align.is_power_of_two() && align <= 16);
        if !self.fits(size, align) {
            let want = next_chunk_size(chunk_size, size);
            let base = source(want);
            if base.is_null() {
                return base;
            }
            self.chunks.push(Chunk { base, len: want });
            self.cur = base;
            // SAFETY: the source handed us `want` bytes starting at `base`.
            self.end = unsafe { base.add(want) };
        }
        let aligned = align_up(self.cur as usize, align) as *mut u8;
        let padding = aligned as usize - self.cur as usize;
        // SAFETY: `fits` (or the fresh chunk) guarantees `aligned + size <= end`.
        self.cur = unsafe { aligned.add(size) };
        self.used += padding + size;
        self.high_water = self.high_water.max(self.used);
        aligned
    }

    /// Whether the current chunk can hold `size` bytes at `align` from `cur`.
    fn fits(&self, size: usize, align: usize) -> bool {
        if self.cur.is_null() {
            return false;
        }
        let aligned = align_up(self.cur as usize, align);
        aligned
            .checked_add(size)
            .is_some_and(|end| end <= self.end as usize)
    }

    /// Resize the block at `ptr` (currently `old` bytes) to `new` bytes.
    ///
    /// In place when the block is the arena's most recent allocation and the
    /// chunk has room — the shape append-style string growth hits — otherwise
    /// a fresh allocation plus a copy of `min(old, new)` bytes. The old block
    /// is never reused. Returns null only when the source does.
    ///
    /// # Safety
    ///
    /// `ptr` must have come from this arena's `alloc_with` (or `grow_last`)
    /// with a size of at least `old`, and must not have been passed to a
    /// `grow_last` that moved it since.
    pub unsafe fn grow_last(
        &mut self,
        ptr: *mut u8,
        old: usize,
        new: usize,
        chunk_size: usize,
        source: &mut impl FnMut(usize) -> *mut u8,
    ) -> *mut u8 {
        if ptr.is_null() {
            // `realloc(NULL, n)` is `malloc(n)`.
            return self.alloc_with(new, 16, chunk_size, source);
        }
        if can_grow_in_place(ptr as usize, old, new, self.cur as usize, self.end as usize) {
            // SAFETY: `can_grow_in_place` checked `ptr + new <= end`.
            self.cur = unsafe { ptr.add(new) };
            self.used = self.used - old + new;
            self.high_water = self.high_water.max(self.used);
            return ptr;
        }
        let moved = self.alloc_with(new, 16, chunk_size, source);
        if !moved.is_null() {
            // SAFETY: both blocks are valid for `min(old, new)` bytes and are
            // distinct allocations, so they do not overlap.
            unsafe { core::ptr::copy_nonoverlapping(ptr, moved, old.min(new)) };
        }
        moved
    }

    /// Return every chunk but the first to `release` and rewind the cursor to
    /// the first chunk's base. Exported for B1; nothing in this ADR calls it.
    pub fn reset(&mut self, release: &mut impl FnMut(*mut u8, usize)) {
        // Keep the first chunk (if any); `split_off` at 0 on an empty vector
        // releases nothing rather than panicking as `drain(1..)` would.
        let keep = self.chunks.len().min(1);
        for chunk in self.chunks.split_off(keep) {
            release(chunk.base, chunk.len);
        }
        if let Some(first) = self.chunks.first() {
            self.cur = first.base;
            // SAFETY: `first.len` bytes were handed to us at `first.base`.
            self.end = unsafe { first.base.add(first.len) };
        }
        self.used = 0;
    }

    /// Current bookkeeping.
    #[must_use]
    pub fn stats(&self) -> ArenaStats {
        ArenaStats {
            chunks: self.chunks.len(),
            reserved: self.chunks.iter().map(|c| c.len).sum(),
            used: self.used,
            high_water: self.high_water,
        }
    }
}

/// Round `addr` up to a multiple of `align` (a power of two).
#[must_use]
pub fn align_up(addr: usize, align: usize) -> usize {
    (addr + align - 1) & !(align - 1)
}

/// How large the next chunk must be to hold a `request` of that many bytes:
/// the configured `chunk_size`, or the request itself when it is larger, so
/// one oversized allocation gets a chunk of its own rather than failing.
#[must_use]
pub fn next_chunk_size(chunk_size: usize, request: usize) -> usize {
    chunk_size.max(request)
}

/// Whether the block at `ptr` of `old` bytes can become `new` bytes where it
/// sits: it must be the most recent allocation (`ptr + old == cur`) and the
/// new extent must still fit the chunk (`ptr + new <= end`).
#[must_use]
pub fn can_grow_in_place(ptr: usize, old: usize, new: usize, cur: usize, end: usize) -> bool {
    ptr != 0
        && ptr.checked_add(old) == Some(cur)
        && ptr.checked_add(new).is_some_and(|new_end| new_end <= end)
}

#[cfg(test)]
mod tests;
