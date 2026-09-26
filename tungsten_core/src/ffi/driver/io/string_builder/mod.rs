//! `StringBuilder`: an internally mutable byte buffer behind an opaque handle
//! (ADR 14.9.26a, Mutable References Phase 1).
//!
//! The handle a `.tg` program holds is the **address of a header** — a
//! [`TgStringBuilder`] — marshalled as `Nat`, exactly as `tg_string_to_cstring`
//! hands out a C-string address. The header owns a buffer; pushes append into
//! it and grow it by amortised doubling; `to_string` hands the buffer out as a
//! Tungsten `String` **without copying** and poisons the header.
//!
//! ## Allocation
//!
//! Header and buffer both come from the C allocator shim (`c_allocator`),
//! never Rust's: the `String` that leaves `to_string` is owned by compiled
//! Tungsten code, which frees it with `free(3)`, and a Rust-allocated buffer
//! there would be a cross-allocator free (ADR 18.5.26f discipline). Growth is
//! recorded to the allocation profiler under `CLASS_STRING`, the same class
//! owned concat records under — there is no new class.
//!
//! On `wasm32-unknown-unknown` the shim returns null for every allocation, so
//! `tg_string_builder_new` aborts there. Knowingly: that target has no C
//! allocator to be honest about (ADR 28.7.26a), and aborting is the route every
//! string FFI already takes on a null allocation.
//!
//! ## The poison
//!
//! A consumed builder is **dead**. `to_string` sets `state` to
//! [`CONSUMED`] and nulls `ptr` rather than overloading `cap` or `len` with a
//! sentinel that means something else elsewhere. Every entry point classifies
//! its handle through the pure [`header_state`] first and aborts — with one
//! line naming the builder — on anything but [`HeaderState::Live`], so a use
//! after consumption cannot alias the buffer the program now owns as a `String`.
//!
//! The `abort()` is not exercised in-process (it would kill the harness);
//! what the tests pin is the classifier and the line's text.

use std::ffi::c_void;
use std::ptr;

use super::c_allocator;
use super::strings::TgString;

/// The header word for a builder that still owns its buffer.
pub const LIVE: u64 = 1;
/// The header word for a builder whose buffer `to_string` has handed out.
pub const CONSUMED: u64 = 2;

/// The smallest buffer a first push allocates, so a builder pushed one byte
/// at a time does not realloc on every one of its first few pushes.
pub const MIN_CAPACITY: u64 = 16;

/// The header a `StringBuilder` handle points at.
///
/// `#[repr(C)]` because compiled code never reads it — but the layout is the
/// contract a future native reimplementation (I3) would have to honour, and a
/// Rust-default layout would leave that contract unstated.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TgStringBuilder {
    /// The buffer, from the C allocator; null while `cap == 0` and after
    /// consumption.
    pub ptr: *mut u8,
    /// Bytes used.
    pub len: u64,
    /// Bytes allocated.
    pub cap: u64,
    /// [`LIVE`] or [`CONSUMED`].
    pub state: u64,
}

/// What a handle points at, as the entry points decide it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderState {
    /// A builder that still owns its buffer — the only state an entry point
    /// proceeds on.
    Live,
    /// A builder `to_string` has already consumed.
    Consumed,
    /// A null handle (0), which points at no header at all.
    Null,
}

/// Classify a handle. The only decision procedure the entry points use.
///
/// Pure over injected data: the `unsafe` read of the address happens in the
/// entry point, which passes `None` for a null handle so this can be asserted
/// without fabricating a pointer. Any `state` word that is not [`LIVE`] reads
/// as consumed — a header that has been freed and reused would carry
/// arbitrary bits, and refusing it is the safe reading.
#[must_use]
pub fn header_state(handle: u64, header: Option<TgStringBuilder>) -> HeaderState {
    match header {
        None => HeaderState::Null,
        Some(_) if handle == 0 => HeaderState::Null,
        Some(h) if h.state == LIVE => HeaderState::Live,
        Some(_) => HeaderState::Consumed,
    }
}

/// The one line written before an abort on a non-live handle.
///
/// Names the entry point and the handle, so the message says *which* builder
/// and *which* call rather than the bare fact of an abort.
#[must_use]
pub fn dead_handle_message(entry_point: &str, handle: u64, state: HeaderState) -> String {
    let reason = match state {
        HeaderState::Live => "live",
        HeaderState::Consumed => "already consumed by to_string",
        HeaderState::Null => "null",
    };
    format!("tungsten: {entry_point}: StringBuilder handle {handle:#x} is {reason}; aborting")
}

/// The capacity to grow to when a buffer of `cap` bytes holding `len` must
/// take `add` more.
///
/// `max(2 * cap, len + add, MIN_CAPACITY)`: amortised doubling, but never
/// short of what the push needs, and the first push from `cap = 0` is a named
/// case rather than a doubling of zero.
#[must_use]
pub fn grow_policy(len: u64, cap: u64, add: u64) -> u64 {
    let doubled = cap.saturating_mul(2);
    let needed = len.saturating_add(add);
    doubled.max(needed).max(MIN_CAPACITY)
}

/// The bytes a regrowth from `cap` to `new_cap` adds to the allocation
/// profile: the growth only, since `realloc` reuses the old buffer when it
/// can (the owned-concat precedent, ADR 18.5.26f).
///
/// Its own function because the profiler is inert in an unprofiled build, so
/// the arithmetic inside the growth path is otherwise unobservable to any
/// test — the mutation sweep found exactly that survivor.
#[must_use]
pub fn growth_recorded(cap: u64, new_cap: u64) -> u64 {
    new_cap - cap
}

/// The UTF-8 encoding of the scalar `code`, or `None` if it is not one
/// (a surrogate, or a value above `0x10FFFF`).
///
/// Pure, so the rejection is assertable without going through the abort.
#[must_use]
pub fn encode_scalar(code: u64) -> Option<([u8; 4], usize)> {
    let scalar = char::from_u32(u32::try_from(code).ok()?)?;
    let mut bytes = [0u8; 4];
    let written = scalar.encode_utf8(&mut bytes).len();
    Some((bytes, written))
}

/// The line written before an abort on a value `push_char` cannot encode.
#[must_use]
pub fn invalid_scalar_message(handle: u64, code: u64) -> String {
    format!(
        "tungsten: tg_string_builder_push_char: {code:#x} is not a Unicode scalar value \
         (StringBuilder handle {handle:#x}); aborting"
    )
}

// ===========================================================================
// Entry points
// ===========================================================================

/// A new, empty builder: `cap = 0`, `ptr = null`.
///
/// Aborts if the header cannot be allocated — the `concat.rs` precedent.
#[no_mangle]
pub extern "C" fn tg_string_builder_new() -> u64 {
    allocate_header(0)
}

/// A new builder whose buffer already holds `capacity` bytes.
#[no_mangle]
pub extern "C" fn tg_string_builder_with_capacity(capacity: u64) -> u64 {
    allocate_header(capacity)
}

/// Append `s`'s bytes; returns the same handle.
///
/// # Safety
/// - `s.ptr` must be a valid pointer to at least `s.len` bytes, or null
#[no_mangle]
pub extern "C" fn tg_string_builder_push_str(handle: u64, s: TgString) -> u64 {
    let header = live_header_or_abort("tg_string_builder_push_str", handle);
    // A null pointer is the empty string, as it is for every string FFI; a
    // zero-length push through a live pointer is a zero-byte copy, which is
    // valid on any pointer, so no second guard is needed (or testable).
    if !s.ptr.is_null() {
        // SAFETY: the caller's contract for `s`; `header` was just classified
        // live, so it points at a header this module allocated.
        let bytes = unsafe { std::slice::from_raw_parts(s.ptr.cast::<u8>(), s.len as usize) };
        unsafe { append_bytes(header, bytes) };
    }
    handle
}

/// Append one Unicode scalar as UTF-8; returns the same handle.
///
/// A surrogate or a value above `0x10FFFF` aborts with a named line rather
/// than being silently replaced: a replacement character in the output would
/// be a wrong answer that looks like a right one.
#[no_mangle]
pub extern "C" fn tg_string_builder_push_char(handle: u64, code: u64) -> u64 {
    let header = live_header_or_abort("tg_string_builder_push_char", handle);
    let Some((bytes, written)) = encode_scalar(code) else {
        eprintln!("{}", invalid_scalar_message(handle, code));
        std::process::abort();
    };
    // SAFETY: `header` was just classified live.
    unsafe { append_bytes(header, &bytes[..written]) };
    handle
}

/// Bytes used.
#[no_mangle]
pub extern "C" fn tg_string_builder_len(handle: u64) -> u64 {
    let header = live_header_or_abort("tg_string_builder_len", handle);
    // SAFETY: `header` was just classified live.
    unsafe { (*header).len }
}

/// Hand the buffer out as a `String` **without copying**, and poison the
/// header. The builder is dead after this call; any further use aborts.
///
/// Capacity slack stays allocated behind the returned string and is reclaimed
/// when the program frees it — the same shape every other string leaves the
/// runtime with. An empty builder yields the null empty string, as
/// `tg_string_concat` does for an empty result.
#[no_mangle]
pub extern "C" fn tg_string_builder_to_string(handle: u64) -> TgString {
    let header = live_header_or_abort("tg_string_builder_to_string", handle);
    // SAFETY: `header` was just classified live; after this block nothing
    // reads the buffer through it again, because `state` is poisoned.
    unsafe {
        let taken = TgString {
            ptr: (*header).ptr.cast_const().cast(),
            len: (*header).len,
        };
        (*header).ptr = ptr::null_mut();
        (*header).len = 0;
        (*header).cap = 0;
        (*header).state = CONSUMED;
        taken
    }
}

// ===========================================================================
// The unsafe half, kept below the pure decisions above
// ===========================================================================

/// Allocate a header (and, when `capacity > 0`, its buffer) from the C
/// allocator and return its address. Aborts on a null allocation.
fn allocate_header(capacity: u64) -> u64 {
    // SAFETY: `malloc` contract; the size is a compile-time constant.
    let header = unsafe { c_allocator::malloc(std::mem::size_of::<TgStringBuilder>()) }
        .cast::<TgStringBuilder>();
    if header.is_null() {
        std::process::abort();
    }
    let buffer = if capacity == 0 {
        ptr::null_mut()
    } else {
        // SAFETY: `malloc` contract. The shim is the runtime symbol, which
        // records the request to the profiler itself (ADR 14.9.26b).
        let buffer = unsafe { c_allocator::malloc(capacity as usize) }.cast::<u8>();
        if buffer.is_null() {
            std::process::abort();
        }
        buffer
    };
    // SAFETY: `header` is a fresh, non-null allocation of the right size.
    unsafe {
        header.write(TgStringBuilder {
            ptr: buffer,
            len: 0,
            cap: capacity,
            state: LIVE,
        });
    }
    header as u64
}

/// Read the header at `handle`, or `None` for a null handle.
fn read_header(handle: u64) -> Option<TgStringBuilder> {
    if handle == 0 {
        return None;
    }
    // SAFETY: a non-zero handle came from `allocate_header`, which never
    // frees; a forged address is the caller's contract violation, as it is for
    // every other address-as-Nat extern in this crate.
    Some(unsafe { *(handle as *const TgStringBuilder) })
}

/// The header pointer for a live handle, or abort with one named line.
fn live_header_or_abort(entry_point: &str, handle: u64) -> *mut TgStringBuilder {
    let state = header_state(handle, read_header(handle));
    if state != HeaderState::Live {
        eprintln!("{}", dead_handle_message(entry_point, handle, state));
        std::process::abort();
    }
    handle as *mut TgStringBuilder
}

/// Append `bytes` to the buffer behind `header`, growing it by [`grow_policy`]
/// when it does not fit.
///
/// # Safety
/// - `header` must point at a live header this module allocated
unsafe fn append_bytes(header: *mut TgStringBuilder, bytes: &[u8]) {
    let add = bytes.len() as u64;
    let (len, cap) = unsafe { ((*header).len, (*header).cap) };
    if len + add > cap {
        let new_cap = grow_policy(len, cap, add);
        // SAFETY: `realloc` contract — `ptr` is null (cap 0) or from `malloc`
        // with `cap` bytes; the old size is what an arena copy needs.
        let grown = unsafe {
            c_allocator::realloc(
                (*header).ptr.cast::<c_void>(),
                cap as usize,
                new_cap as usize,
            )
        }
        .cast::<u8>();
        if grown.is_null() {
            std::process::abort();
        }
        // Record only the growth — realloc reuses the buffer when it can.
        tungsten_runtime::alloc_profile_record_external(
            growth_recorded(cap, new_cap),
            tungsten_runtime::CLASS_STRING,
        );
        unsafe {
            (*header).ptr = grown;
            (*header).cap = new_cap;
        }
    }
    // SAFETY: the buffer now holds at least `len + add` bytes.
    unsafe {
        ptr::copy_nonoverlapping(bytes.as_ptr(), (*header).ptr.add(len as usize), bytes.len());
        (*header).len = len + add;
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
