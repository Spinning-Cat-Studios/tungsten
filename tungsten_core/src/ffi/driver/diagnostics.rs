//! Diagnostic and tracing FFI — bridge from self-hosted driver to Rust diagnostic tooling.
//!
//! Provides three capabilities for the self-hosted compiler:
//! 1. **dump-core**: Pretty-print a definition's term, type, and free TyVars
//! 2. **check-tyvar-escape**: Check whether a term has free TyVar escapes
//! 3. **adt-trace**: Runtime ADT construct/match tracing (T3, ADR 16.4.26a)
//!
//! See ADR 18.4.26e for design rationale.

use std::ffi::{c_char, CStr, CString};
use std::io::{self, Write};
use std::ptr;

use super::set_driver_error;
use crate::ffi::{with_arena_ref, TermHandle, TypeHandle, INVALID_HANDLE};

// ============================================================================
// dump-core: Pretty-print a definition
// ============================================================================

/// Dump Core IR for a single definition.
///
/// Returns a formatted string showing the term, type, and free type variables.
/// The caller must free the returned string with `tg_free_string`.
///
/// # Arguments
/// * `name` - Null-terminated definition name
/// * `term_handle` - Handle to the elaborated term
/// * `type_handle` - Handle to the elaborated type
///
/// # Returns
/// * Formatted string, or null on error
#[no_mangle]
pub unsafe extern "C" fn tg_diagnostic_dump_core(
    name: *const c_char,
    term_handle: TermHandle,
    type_handle: TypeHandle,
) -> *mut c_char {
    if name.is_null() {
        set_driver_error("tg_diagnostic_dump_core: name is null");
        return ptr::null_mut();
    }

    let name_str = if let Ok(s) = CStr::from_ptr(name).to_str() {
        s
    } else {
        set_driver_error("tg_diagnostic_dump_core: invalid UTF-8 in name");
        return ptr::null_mut();
    };

    let result = with_arena_ref!(|arena| {
        let term = arena.get_term(term_handle);
        let ty = arena.get_type(type_handle);

        match (term, ty) {
            (Some(t), Some(tp)) => {
                let free = t.free_type_vars();
                let free_str = if free.is_empty() {
                    "∅".to_string()
                } else {
                    let mut vars: Vec<&str> =
                        free.iter().map(std::string::String::as_str).collect();
                    vars.sort_unstable();
                    format!("{{{}}}", vars.join(", "))
                };

                let output = format!(
                    "┌─────────────────────────────────────────────────────────────┐\n\
                     │  Definition: {:<47}│\n\
                     │  Type: {:<53}│\n\
                     │{:61}│\n\
                     │  Term: {:<53}│\n\
                     │  Free TyVars: {:<46}│\n\
                     └─────────────────────────────────────────────────────────────┘",
                    name_str,
                    format!("{tp}"),
                    "",
                    format!("{t}"),
                    free_str,
                );
                Some(output)
            }
            (None, _) => {
                set_driver_error(format!(
                    "tg_diagnostic_dump_core: invalid term handle {term_handle}"
                ));
                None
            }
            (_, None) => {
                set_driver_error(format!(
                    "tg_diagnostic_dump_core: invalid type handle {type_handle}"
                ));
                None
            }
        }
    });

    match result {
        Some(s) => CString::new(s)
            .map(CString::into_raw)
            .unwrap_or(ptr::null_mut()),
        None => ptr::null_mut(),
    }
}

// ============================================================================
// check-tyvar-escape: Detect free TyVars in a term
// ============================================================================

/// Check whether a term has free TyVar escapes.
///
/// Returns a comma-separated list of escaped TyVar names (excluding `@`-prefixed
/// internal variables), or null if there are no escapes.
///
/// # Arguments
/// * `term_handle` - Handle to the elaborated term
///
/// # Returns
/// * Comma-separated escaped TyVar names, or null if clean.
///   The caller must free the returned string with `tg_free_string`.
#[no_mangle]
pub extern "C" fn tg_diagnostic_check_tyvar_escape(term_handle: TermHandle) -> *mut c_char {
    if term_handle == INVALID_HANDLE {
        set_driver_error("tg_diagnostic_check_tyvar_escape: invalid handle");
        return ptr::null_mut();
    }

    let result = with_arena_ref!(|arena| {
        if let Some(term) = arena.get_term(term_handle) {
            let free = term.free_type_vars();
            let genuine: Vec<String> = free.into_iter().filter(|v| !v.starts_with('@')).collect();
            if genuine.is_empty() {
                None
            } else {
                let mut sorted = genuine;
                sorted.sort();
                Some(sorted.join(", "))
            }
        } else {
            set_driver_error(format!(
                "tg_diagnostic_check_tyvar_escape: invalid term handle {term_handle}"
            ));
            None
        }
    });

    match result {
        Some(s) => CString::new(s)
            .map(CString::into_raw)
            .unwrap_or(ptr::null_mut()),
        None => ptr::null_mut(),
    }
}

/// Count free TyVar escapes in a term (excluding `@`-prefixed internal variables).
///
/// # Returns
/// * Number of genuine TyVar escapes, or `u64::MAX` on error
#[no_mangle]
pub extern "C" fn tg_diagnostic_tyvar_escape_count(term_handle: TermHandle) -> u64 {
    if term_handle == INVALID_HANDLE {
        set_driver_error("tg_diagnostic_tyvar_escape_count: invalid handle");
        return u64::MAX;
    }

    with_arena_ref!(|arena| {
        if let Some(term) = arena.get_term(term_handle) {
            let free = term.free_type_vars();
            free.into_iter().filter(|v| !v.starts_with('@')).count() as u64
        } else {
            set_driver_error(format!(
                "tg_diagnostic_tyvar_escape_count: invalid term handle {term_handle}"
            ));
            u64::MAX
        }
    })
}

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
