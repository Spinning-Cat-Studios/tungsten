//! Diagnostic FFI — bridge from the self-hosted driver to Rust term analysis.
//!
//! Provides the capabilities a developer build of `tungsten1` exposes as
//! `check --<flag>`:
//! 1. **dump-core**: Pretty-print a definition's term, type, and free TyVars
//! 2. **check-tyvar-escape**: Check whether a term has free TyVar escapes
//! 3. **check-free-vars**: Check whether a term is closed (ADR 19.8.26d)
//! 4. **check-well-typed**: Check every eliminator's operand former (ADR 3.9.26h)
//!
//! Runtime tracing and the allocation-profile marker live in [`trace`].
//!
//! See ADR 18.4.26e for design rationale.

mod trace;

pub use trace::*;

use std::ffi::{c_char, CStr, CString};
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
        // Diagnostic dump is a cold path: materialize both node-arena sides.
        let term = crate::ffi::terms::nodes::materialize_term(arena, term_handle);
        let ty = crate::ffi::types::nodes::materialize_type(arena, type_handle);

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

    into_c_string(result)
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
        if let Some(term) = crate::ffi::terms::nodes::materialize_term(arena, term_handle) {
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

    into_c_string(result)
}

// ============================================================================
// core-term: the term alone, unframed
// ============================================================================

/// One term rendered with nothing around it — no frame, no type, no label.
///
/// The machine-readable counterpart to [`tg_diagnostic_dump_core`], whose
/// box-drawn output is for humans and is *not* parseable: its border is padded
/// to a minimum width, so a long term makes it ragged. Rather than teach a
/// consumer to unpick that, this hands back exactly what `Display` produced.
///
/// That matters for `tungsten diff selfhost-core`, which compares this
/// compiler's rendering of a definition against the self-host's: both sides go
/// through the same `impl Display for Term`, so a textual comparison is a
/// structural one.
///
/// # Returns
/// * The rendered term, or null on an unreadable handle.
///   The caller must free the returned string with `tg_free_string`.
#[no_mangle]
pub extern "C" fn tg_diagnostic_core_term(term_handle: TermHandle) -> *mut c_char {
    if term_handle == INVALID_HANDLE {
        set_driver_error("tg_diagnostic_core_term: invalid handle");
        return ptr::null_mut();
    }

    let rendered = with_arena_ref!(|arena| {
        crate::ffi::terms::nodes::materialize_term(arena, term_handle).map(|term| format!("{term}"))
    });

    if rendered.is_none() {
        set_driver_error(format!(
            "tg_diagnostic_core_term: invalid term handle {term_handle}"
        ));
    }
    into_c_string(rendered)
}

// ============================================================================
// check-free-vars: Detect free VALUE variables in a term
// ============================================================================

/// The free *value* variables of a term, comma-separated, or null if it is
/// closed.
///
/// The term-level twin of [`tg_diagnostic_check_tyvar_escape`], and it exists
/// because the type-level one was not enough: an elaborated body whose match
/// arms reference pattern variables the arm never binds is *well-typed* — the
/// environment that resolved those names is still in scope when the check runs
/// — and nothing downstream asks whether the term it produced is closed. ADR
/// 19.8.26d found exactly that shape in the self-hosted elaborator, where it
/// had been invisible because the compiled path rebuilds the bindings from the
/// pattern and never reads the Core term.
///
/// A closed term returns null rather than an empty string, matching the
/// tyvar-escape convention: the caller's "is this clean" test is a null check.
///
/// # Returns
/// * Comma-separated free variable names, sorted, or null when closed.
///   The caller must free the returned string with `tg_free_string`.
#[no_mangle]
pub extern "C" fn tg_diagnostic_free_term_vars(term_handle: TermHandle) -> *mut c_char {
    if term_handle == INVALID_HANDLE {
        set_driver_error("tg_diagnostic_free_term_vars: invalid handle");
        return ptr::null_mut();
    }

    let result = with_arena_ref!(|arena| {
        if let Some(term) = crate::ffi::terms::nodes::materialize_term(arena, term_handle) {
            let mut free: Vec<String> = term.free_vars().into_iter().collect();
            if free.is_empty() {
                None
            } else {
                free.sort();
                Some(free.join(", "))
            }
        } else {
            set_driver_error(format!(
                "tg_diagnostic_free_term_vars: invalid term handle {term_handle}"
            ));
            None
        }
    });

    into_c_string(result)
}

// ============================================================================
// check-well-typed: eliminators standing over the wrong former (ADR 3.9.26h)
// ============================================================================

/// The separator between one definition's shape findings.
///
/// A semicolon rather than a comma because [`tg_diagnostic_free_term_vars`]'s
/// consumer splits on commas, and a finding's own text ("fst over a product")
/// must be free to contain one later without silently splitting in two.
const FINDING_SEPARATOR: &str = "; ";

/// Every eliminator in a term whose operand's recorded type refuses it, or
/// null when the term's shapes all agree.
///
/// The sibling of [`tg_diagnostic_free_term_vars`], and the reason ADR 3.9.26h
/// exists: `closed-terms` answers "is every name bound", and **both** defects
/// that surfaced while closing ADR 21.8.26a passed it. A saturated constructor
/// application elaborated to `App(App(λx:(A × B). …, 9), N2)` — closed, and
/// applying a unary lambda's *result* to a second argument. A tuple projection
/// emitted `fst` of a scalar. Codegen never reads the Core term and the type
/// checker resolves through an environment still in scope, so nothing else in
/// the repo can see either.
///
/// Findings are separated by [`FINDING_SEPARATOR`] and each reads
/// `<eliminator> over <former>`. A clean term returns null rather than an empty
/// string, matching the convention of every diagnostic wrapper beside it.
///
/// # Returns
/// * The findings, or null when there are none.
///   The caller must free the returned string with `tg_free_string`.
#[no_mangle]
pub extern "C" fn tg_diagnostic_shape_mismatches(term_handle: TermHandle) -> *mut c_char {
    if term_handle == INVALID_HANDLE {
        set_driver_error("tg_diagnostic_shape_mismatches: invalid handle");
        return ptr::null_mut();
    }

    let result = with_arena_ref!(|arena| {
        if let Some(term) = crate::ffi::terms::nodes::materialize_term(arena, term_handle) {
            let labels: Vec<String> = term
                .shape_mismatches()
                .iter()
                .map(crate::terms::analysis::ShapeMismatch::label)
                .collect();
            if labels.is_empty() {
                None
            } else {
                Some(labels.join(FINDING_SEPARATOR))
            }
        } else {
            set_driver_error(format!(
                "tg_diagnostic_shape_mismatches: invalid term handle {term_handle}"
            ));
            None
        }
    });

    into_c_string(result)
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
        if let Some(term) = crate::ffi::terms::nodes::materialize_term(arena, term_handle) {
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

/// Hand a `String` result across the boundary, or null when there is none.
///
/// Every wrapper above ends the same way, and a copy of this three-line dance
/// per wrapper is how one of them would eventually leak or return a dangling
/// pointer without any test noticing.
fn into_c_string(result: Option<String>) -> *mut c_char {
    match result {
        Some(text) => CString::new(text)
            .map(CString::into_raw)
            .unwrap_or(ptr::null_mut()),
        None => ptr::null_mut(),
    }
}
