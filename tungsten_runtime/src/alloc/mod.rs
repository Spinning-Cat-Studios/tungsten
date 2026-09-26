//! The one allocation symbol compiled programs call (ADR 14.9.26b).
//!
//! Every heap request a compiled Tungsten program makes — closure environment,
//! μ/ADT node, ref cell, string buffer, and the string runtime's C-allocator
//! shim — reaches `__tungsten_alloc(size, class)`. What it does is decided by a
//! **process-global mode**, read once from `TUNGSTEN_ARENA` by
//! `__tungsten_arena_init()` in the program prologue:
//!
//! | `TUNGSTEN_ARENA`      | mode   | `__tungsten_alloc`         | `__tungsten_realloc`     |
//! |-----------------------|--------|----------------------------|--------------------------|
//! | unset / `off`         | `Off`  | `malloc(3)`                | `realloc(3)`             |
//! | `bump` / `bump:<MiB>` | `Bump` | thread-local bump arena    | `Arena::grow_last`       |
//!
//! The mode lives in [`ARENA_MODE`], exported as `__tungsten_arena_mode`, which generated
//! code also reads at each allocation site (ADR 18.9.26c) so that mode `off`
//! calls `malloc(3)` directly and never enters `__tungsten_alloc`.
//!
//! A process in which init never ran — the bootstrap compiler, which links
//! this crate but has no Tungsten prologue — is in mode `Off`, byte-identical
//! to the pre-ADR `malloc` path.
//!
//! The mode is global and only the *arena* is thread-local, on purpose: a
//! per-thread mode would let a buffer allocated in bump mode on one thread be
//! grown on another through libc `realloc`, which is undefined behaviour. A
//! global mode makes the question total — a pointer either always came from an
//! arena or never did.
//!
//! Profiler recording (ADR 2.7.26a) is a mode *inside* the symbol, not a
//! different callee, and is gated on [`alloc_profile_is_active`] because
//! `Profiler::record` activates the profiler as a side effect, and activation
//! is what turns on the external-record path and the 1 GiB interim dumps.
//!
//! Out of memory in either mode is one stderr line and `std::process::abort()`:
//! generated code never null-checks the return, and the v2.0 plan forbids
//! unwinding out of the allocator (§9).

use core::cell::UnsafeCell;
use core::ffi::c_void;
use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use crate::alloc_profile::{alloc_profile_is_active, alloc_profile_record_external, CLASS_OTHER};
use crate::arena::{Arena, ArenaStats};

/// The allocation mode, chosen once per process.
///
/// Discriminants cross the FFI boundary in `__tungsten_arena_stats` and MUST
/// match `ArenaMode` in `tungsten_codegen/src/codegen/data/mod.rs`.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArenaMode {
    /// Every request goes to the platform `malloc(3)` — today's behaviour.
    Off = 0,
    /// Every request bumps the calling thread's arena.
    Bump = 1,
}

pub const MODE_OFF: u32 = ArenaMode::Off as u32;
pub const MODE_BUMP: u32 = ArenaMode::Bump as u32;

/// Chunk size when `TUNGSTEN_ARENA=bump` names none: 4 MiB. Large enough
/// that a self-compile's ~1 GB of ~47-byte requests takes a few hundred
/// chunks, small enough that a hello-world reserves little.
pub const DEFAULT_CHUNK_BYTES: usize = 4 * 1024 * 1024;

/// Alignment every arena block gets — the C allocator's guarantee, which
/// generated code relies on for `{ ptr, i64 }` and 16-byte closure pairs.
const BLOCK_ALIGN: usize = 16;

/// The process-global mode, exported so generated code can branch on it at
/// each allocation site (ADR 18.9.26c): mode `off` then calls `malloc(3)`
/// directly instead of paying a call level through `__tungsten_alloc`.
///
/// Generated code reads it with a plain `load i32`: `AtomicU32` is four bytes,
/// four-aligned, and the prologue's store precedes the first allocation on the
/// same thread, with nothing storing afterwards. Exported under the symbol
/// name codegen declares, `__tungsten_arena_mode`.
#[export_name = "__tungsten_arena_mode"]
pub static ARENA_MODE: AtomicU32 = AtomicU32::new(MODE_OFF);
static CHUNK_BYTES: AtomicUsize = AtomicUsize::new(DEFAULT_CHUNK_BYTES);

/// One arena per thread. `const`-initialised: an initialiser that allocated
/// would recurse through the allocator it backs.
struct ArenaCell(UnsafeCell<Arena>);

thread_local! {
    static CURRENT_ARENA: ArenaCell = const { ArenaCell(UnsafeCell::new(Arena::new())) };
}

/// The current process-global mode.
#[must_use]
#[inline]
pub fn arena_mode() -> ArenaMode {
    match ARENA_MODE.load(Ordering::Relaxed) {
        MODE_BUMP => ArenaMode::Bump,
        _ => ArenaMode::Off,
    }
}

/// How `TUNGSTEN_ARENA` was read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArenaSetting {
    /// A recognised value: the mode and, for `Bump`, the chunk size in bytes.
    Recognised { mode: ArenaMode, chunk_bytes: usize },
    /// An unrecognised value; the mode stays `Off` and one line says so.
    Unrecognised,
}

/// Parse a `TUNGSTEN_ARENA` value: unset, empty or `off` → `Off`; `bump` →
/// `Bump` at the default chunk size; `bump:<n>` → `Bump` with `n` MiB chunks
/// (`n` ≥ 1). Anything else is `Unrecognised`, never a silent `Off`.
#[must_use]
pub fn parse_arena_setting(value: Option<&str>) -> ArenaSetting {
    let value = value.map_or("", str::trim);
    if value.is_empty() || value == "off" {
        return ArenaSetting::Recognised {
            mode: ArenaMode::Off,
            chunk_bytes: DEFAULT_CHUNK_BYTES,
        };
    }
    let Some(rest) = value.strip_prefix("bump") else {
        return ArenaSetting::Unrecognised;
    };
    let chunk_bytes = match rest.strip_prefix(':') {
        None if rest.is_empty() => DEFAULT_CHUNK_BYTES,
        Some(mib) => match mib.parse::<usize>() {
            Ok(n) if n >= 1 => n.saturating_mul(1024 * 1024),
            _ => return ArenaSetting::Unrecognised,
        },
        None => return ArenaSetting::Unrecognised,
    };
    ArenaSetting::Recognised {
        mode: ArenaMode::Bump,
        chunk_bytes,
    }
}

/// Apply a parsed setting to the process globals — the programmatic half of
/// [`__tungsten_arena_init`], for hosts that embed the runtime and for tests
/// that need a mode without an environment. Process-global: a test that sets
/// `Bump` must serialise against every other test that frees a block.
pub fn apply_arena_setting(setting: ArenaSetting) {
    match setting {
        ArenaSetting::Recognised { mode, chunk_bytes } => {
            CHUNK_BYTES.store(chunk_bytes, Ordering::Relaxed);
            ARENA_MODE.store(mode as u32, Ordering::Relaxed);
        }
        ArenaSetting::Unrecognised => {
            eprintln!("tungsten runtime: TUNGSTEN_ARENA not recognised (want off | bump[:chunk_mib]); arena stays off");
            ARENA_MODE.store(MODE_OFF, Ordering::Relaxed);
        }
    }
}

/// Read `TUNGSTEN_ARENA` once and set the process-global mode.
///
/// Emitted unconditionally in every compiled program's `__tungsten_inner_main`
/// prologue (the `tg_init_args_c` precedent). Never called by the bootstrap,
/// which therefore stays `Off` even with the variable exported.
#[no_mangle]
pub extern "C" fn __tungsten_arena_init() {
    let value = std::env::var("TUNGSTEN_ARENA").ok();
    apply_arena_setting(parse_arena_setting(value.as_deref()));
}

/// The `malloc(3)` mode `Off` delegates to, on targets that have one.
#[cfg(unix)]
fn platform_malloc(size: usize) -> *mut u8 {
    // SAFETY: `malloc` contract.
    unsafe { libc::malloc(size) }.cast::<u8>()
}

/// `wasm32-unknown-unknown` has no C allocator (ADR 28.7.26a); null is the
/// honest answer, and nothing reaches this arm — its only caller is a
/// codegen-emitted site, and codegen needs LLVM.
#[cfg(not(unix))]
fn platform_malloc(_size: usize) -> *mut u8 {
    core::ptr::null_mut()
}

#[cfg(unix)]
fn platform_realloc(ptr: *mut u8, size: usize) -> *mut u8 {
    // SAFETY: `realloc` contract — `ptr` came from `platform_malloc` or is null.
    unsafe { libc::realloc(ptr.cast(), size) }.cast::<u8>()
}

#[cfg(not(unix))]
fn platform_realloc(_ptr: *mut u8, _size: usize) -> *mut u8 {
    core::ptr::null_mut()
}

/// Run `f` on the calling thread's arena.
fn with_thread_arena<R>(f: impl FnOnce(&mut Arena) -> R) -> R {
    // SAFETY: the cell is thread-local and `f` does not re-enter it — the
    // chunk source is the platform allocator, never this symbol.
    CURRENT_ARENA.with(|cell| f(unsafe { &mut *cell.0.get() }))
}

/// One stderr line, then abort: the v2.0 plan's no-unwinding invariant (§9).
#[cold]
#[inline(never)]
fn abort_out_of_memory(what: &str, bytes: usize) -> ! {
    eprintln!("tungsten runtime: out of memory ({what} of {bytes} bytes)");
    std::process::abort()
}

/// Allocate `size` bytes in the current mode, or abort.
///
/// Inlined into the exported symbol on purpose: every generated allocation
/// pays this path, and measured on `closure_chain` (6.9 M allocations) each
/// extra call level in mode `Off` cost ~1–2 ns per allocation on top of
/// `malloc(3)` — AC 4's branch-cost probe.
#[inline(always)] // Reason: measured; `#[inline]` alone left the call level in the release build
#[allow(clippy::inline_always)]
fn alloc_in_mode(mode: ArenaMode, size: usize) -> *mut u8 {
    let ptr = match mode {
        ArenaMode::Off => platform_malloc(size),
        ArenaMode::Bump => with_thread_arena(|arena| {
            let chunk = CHUNK_BYTES.load(Ordering::Relaxed);
            arena.alloc_with(size, BLOCK_ALIGN, chunk, &mut platform_malloc)
        }),
    };
    if allocation_failed(ptr, size) {
        abort_out_of_memory("alloc", size);
    }
    ptr
}

/// The allocation symbol every generated allocation site calls.
///
/// The request is recorded against the profiler only when a profiled build
/// has activated it (`alloc_profile_record_external` is that gate), so the
/// aliased profiler symbol no longer self-activates (ADR 14.9.26b §2.2).
///
/// # Safety
///
/// Same contract as `malloc(3)`: the memory is uninitialised. `class` is one of
/// the `CLASS_*` constants (out-of-range values record as `CLASS_OTHER`). In
/// mode `Bump` the block must never be passed to `free(3)`; nothing generated
/// does so today.
#[no_mangle]
pub unsafe extern "C" fn __tungsten_alloc(size: u64, class: u32) -> *mut c_void {
    let ptr = alloc_in_mode(arena_mode(), size as usize);
    if alloc_profile_is_active() {
        alloc_profile_record_external(size, class);
    }
    ptr.cast()
}

/// Resize a block `__tungsten_alloc` returned from `old` to `new` bytes.
///
/// Mode `Off` is `realloc(3)`. Mode `Bump` is `Arena::grow_last`: in place
/// when the block is the thread's most recent allocation, else a fresh block
/// plus a copy — so an arena pointer is never handed to libc.
///
/// # Safety
///
/// `ptr` must be null or a block from `__tungsten_alloc`/`__tungsten_realloc`
/// on this thread of at least `old` bytes, and is invalidated by this call.
#[no_mangle]
pub unsafe extern "C" fn __tungsten_realloc(ptr: *mut c_void, old: u64, new: u64) -> *mut c_void {
    let (old, new) = (old as usize, new as usize);
    let block = ptr.cast::<u8>();
    let grown = match arena_mode() {
        ArenaMode::Off => platform_realloc(block, new),
        // A null block is an allocation — `grow_last` treats it so, as
        // `realloc(3)` does.
        ArenaMode::Bump => with_thread_arena(|arena| {
            let chunk = CHUNK_BYTES.load(Ordering::Relaxed);
            // SAFETY: the caller's contract on `ptr` and `old`.
            unsafe { arena.grow_last(block, old, new, chunk, &mut platform_malloc) }
        }),
    };
    if allocation_failed(grown, new) {
        abort_out_of_memory("realloc", new);
    }
    grown.cast()
}

/// Whether a request must abort: the allocator returned null for a non-zero
/// request. A null for zero bytes is a legal `malloc(0)` answer and is handed
/// back as-is.
fn allocation_failed(ptr: *mut u8, size: usize) -> bool {
    ptr.is_null() && size > 0
}

/// Return the calling thread's arena chunks (all but the first) to the
/// platform and rewind. Exported for B1 (region-scoped deallocation); nothing
/// in this ADR calls it, and calling it while any block is live is unsound —
/// the owned-left string `realloc` and the interior-pointer `substring` are
/// the two blockers B1 must clear first.
#[no_mangle]
pub extern "C" fn __tungsten_arena_reset() {
    with_thread_arena(|arena| {
        arena.reset(&mut |base, _len| {
            // SAFETY: every chunk came from `platform_malloc`.
            #[cfg(unix)]
            unsafe {
                libc::free(base.cast());
            }
            #[cfg(not(unix))]
            let _ = base;
        });
    });
}

/// The C-ABI shape of [`ArenaStats`] plus the mode, for tooling.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ArenaStatsOut {
    pub mode: u32,
    pub chunks: u64,
    pub reserved: u64,
    pub used: u64,
    pub high_water: u64,
}

impl ArenaStatsOut {
    fn from_stats(mode: ArenaMode, stats: ArenaStats) -> Self {
        Self {
            mode: mode as u32,
            chunks: stats.chunks as u64,
            reserved: stats.reserved as u64,
            used: stats.used as u64,
            high_water: stats.high_water as u64,
        }
    }
}

/// The calling thread's arena statistics and the process mode.
#[must_use]
pub fn current_arena_stats() -> ArenaStatsOut {
    ArenaStatsOut::from_stats(arena_mode(), with_thread_arena(|arena| arena.stats()))
}

/// Write the calling thread's arena statistics to `out`.
///
/// # Safety
///
/// `out` must be null (a no-op) or valid for a write of `ArenaStatsOut`.
#[no_mangle]
pub unsafe extern "C" fn __tungsten_arena_stats(out: *mut ArenaStatsOut) {
    if !out.is_null() {
        // SAFETY: the caller's contract on `out`.
        unsafe { out.write(current_arena_stats()) };
    }
}

/// Class-tagged profiling malloc (ADR 2.7.26a), kept as a thin alias of
/// [`__tungsten_alloc`] for ABI continuity. It no longer self-activates the
/// profiler — recording is gated on an activated profiler, as every other
/// route through the symbol is.
///
/// # Safety
///
/// Same contract as [`__tungsten_alloc`].
#[no_mangle]
pub unsafe extern "C" fn __tungsten_alloc_profile_malloc_class(
    size: u64,
    class: u32,
) -> *mut c_void {
    unsafe { __tungsten_alloc(size, class) }
}

/// Legacy class-blind profiling malloc: records as `other`.
///
/// # Safety
///
/// Same contract as [`__tungsten_alloc`].
#[no_mangle]
pub unsafe extern "C" fn __tungsten_alloc_profile_malloc(size: u64) -> *mut c_void {
    unsafe { __tungsten_alloc(size, CLASS_OTHER) }
}

#[cfg(test)]
mod tests;
