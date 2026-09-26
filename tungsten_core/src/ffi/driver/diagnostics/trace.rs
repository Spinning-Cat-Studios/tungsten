//! Runtime tracing and profiling markers the self-hosted driver calls.
//!
//! Split from the term-inspection FFI next door because the two answer
//! different questions: everything in [`super`] reads an elaborated term and
//! hands back a string, while these emit lines *as the compiled program runs*.
//! See ADR 16.4.26a (T3) for the ADT trace and ADR 2.7.26a for the marker.

use std::ffi::{c_char, CStr};
use std::io::{self, Write};

// ============================================================================
// ADT Trace (T3, ADR 16.4.26a)
// ============================================================================

/// Maximum bytes to hex-dump from the data field.
const HEX_DUMP_LIMIT: usize = 64;

/// Trace an ADT construct operation.
///
/// Prints: `[adt-trace] construct <type_name> variant=<idx> data=<ptr> size=<n>`
/// followed by a hex dump of the first N bytes.
///
/// # Safety
/// - `type_name` must be a valid null-terminated C string
/// - `data_ptr` must be valid for `data_size` bytes (or null)
#[no_mangle]
pub extern "C" fn __tungsten_trace_adt_construct(
    type_name: *const u8,
    variant_idx: i32,
    data_ptr: *const u8,
    data_size: u64,
) {
    if type_name.is_null() {
        return;
    }
    let name = unsafe { CStr::from_ptr(type_name.cast()) };
    let name_str = name.to_string_lossy();
    let _ = writeln!(
        io::stderr(),
        "[adt-trace] construct {name_str} variant={variant_idx} data={data_ptr:?} size={data_size}"
    );
    if !data_ptr.is_null() && data_size > 0 {
        hex_dump_to_stderr(data_ptr, data_size as usize);
    }
}

/// Trace an ADT match operation.
///
/// Prints: `[adt-trace] match <type_name> tag=<tag> data=<ptr> size=<n>`
/// followed by a hex dump of the first N bytes.
///
/// # Safety
/// - `type_name` must be a valid null-terminated C string
/// - `data_ptr` must be valid for `data_size` bytes (or null)
#[no_mangle]
pub extern "C" fn __tungsten_trace_adt_match(
    type_name: *const u8,
    tag: i32,
    data_ptr: *const u8,
    data_size: u64,
) {
    if type_name.is_null() {
        return;
    }
    let name = unsafe { CStr::from_ptr(type_name.cast()) };
    let name_str = name.to_string_lossy();
    let _ = writeln!(
        io::stderr(),
        "[adt-trace] match {name_str} tag={tag} data={data_ptr:?} size={data_size}"
    );
    if !data_ptr.is_null() && data_size > 0 {
        hex_dump_to_stderr(data_ptr, data_size as usize);
    }
}

/// Print a hex dump of the first N bytes of a data region to stderr.
///
/// Format: `  bytes[0..16]: aa bb cc dd ee ff 00 11  22 33 44 55 66 77 88 99`
fn hex_dump_to_stderr(ptr: *const u8, size: usize) {
    let dump_size = size.min(HEX_DUMP_LIMIT);
    let data = unsafe { std::slice::from_raw_parts(ptr, dump_size) };

    for chunk_start in (0..dump_size).step_by(16) {
        let chunk_end = (chunk_start + 16).min(dump_size);
        let chunk = &data[chunk_start..chunk_end];

        let hex: Vec<String> = chunk.iter().map(|b| format!("{b:02x}")).collect();
        // Insert a gap at the 8-byte boundary for readability
        let (left, right) = if hex.len() > 8 {
            (hex[..8].join(" "), format!(" {}", hex[8..].join(" ")))
        } else {
            (hex.join(" "), String::new())
        };

        let _ = writeln!(
            io::stderr(),
            "  bytes[{chunk_start}..{chunk_end}]: {left}{right}",
        );
    }
}

// ============================================================================
// alloc-profile marker: phase/module boundary snapshot (ADR 2.7.26a)
// ============================================================================

/// Emit an allocation-profile marker line with cumulative totals.
///
/// Called from the self-hosted compiler driver at elaboration phase
/// boundaries (Stub Registration / Signature Collection / per-module Body Elaboration) so allocation slope
/// correlates to compiler structure. No-op when the allocation profiler
/// is inactive (i.e. in non-`--alloc-profile` builds), so unconditional
/// call sites in the self-host driver cost one branch.
///
/// **Delta-attribution convention:** the `phaseB:module <path>` marker fires
/// at the *start* of that module's elaboration
/// (`elaborate_single_module`, `src/compiler/elab/items/per_module/mod.tg`).
/// When computing per-module deltas from consecutive marker/`[arena]` lines,
/// a delta therefore belongs to the module named in the *earlier* marker —
/// pairing it with the later marker's name misattributes every module by one.
///
/// # Safety
/// - `name` must be a valid null-terminated C string, or null (ignored)
#[no_mangle]
pub unsafe extern "C" fn tg_alloc_profile_marker(name: *const c_char) {
    tungsten_runtime::__tungsten_alloc_profile_marker(name);
    // Arena retention snapshot (ADR 2.7.26a §3.4): the marker above only
    // covers codegen-emitted allocation classes; this line attributes the
    // Rust-allocator side (arena deep clones) and anchors it to VmRSS.
    if tungsten_runtime::alloc_profile_is_active() {
        crate::ffi::with_arena_ref!(|arena| {
            eprintln!("{}", crate::ffi::arena_stats::marker_line(arena));
        });
    }
}
