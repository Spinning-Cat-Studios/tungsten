//! The self-hosted termination mirror's FFI seam (ADR 19.8.26d).
//!
//! The soundness RULE is not reimplemented here. Both compilers call one
//! `termination::analyze_with_carried` in this crate — the *carried* form, not
//! the `analyze` wrapper, which discards the cross-module taint channel this
//! seam is built on (D2). What crosses the boundary is the definition
//! environment that rule runs over.
//!
//! ## Why the stream reduces instead of marshalling
//!
//! `DefView` borrows `ty: &Type` and `term: &Term`, so the obvious seam holds
//! every definition's whole elaborated body live at once. P0 measured that at
//! **+587 MiB** on `src/compiler/main.tg` against D1's +250 MiB ceiling — and
//! measured that **91.6% of it is terms**, held for an analysis that needs a
//! real term only inside a recursive component. [`registry`] therefore streams
//! and reduces, and retains only what descent actually reads; the protocol and
//! its invariants are documented there.
//!
//! ## Eight externs
//!
//! Five registration (`reset`, `note_item`, `declare`, `add_def`, `plan`) and
//! three readers (`check`, `failure_count`, `failure_render`) — the same
//! budget the positivity mirror spends, with `add_def` serving both payload
//! streams because the registry's phase already decides what it does.
//!
//! Every failure path stores a message reachable through `tg_get_last_error`
//! and returns `false`. Nothing here silently ignores a call: a dropped
//! definition is a program that passes the termination gate because the
//! offending recursion never arrived.

#![allow(unsafe_code)]

pub(crate) mod registry;
mod wire;

#[cfg(test)]
mod tests;

use std::cell::RefCell;
use std::ffi::{c_char, CStr, CString};

use crate::terms::termination::TerminationAnnotation;

use super::terms::nodes::materialize_term;
use super::types::nodes::materialize_type;
use super::{with_arena_ref, TermHandle, TypeHandle};
use registry::{ItemNote, TerminationRegistry};
use wire::{render_failure, ProtocolError};

thread_local! {
    /// The ambient environment. Thread-local like [`super::ARENA`], and for the
    /// same reason: the FFI is single-threaded by contract.
    static REGISTRY: RefCell<TerminationRegistry> =
        RefCell::new(TerminationRegistry::default());
}

/// Access the registry mutably.
fn with_registry<T>(body: impl FnOnce(&mut TerminationRegistry) -> T) -> T {
    REGISTRY.with(|cell| body(&mut cell.borrow_mut()))
}

/// Record a protocol error where `tg_get_last_error` will find it, and report
/// failure to the caller.
fn fail(error: &ProtocolError) -> bool {
    let message = error.message();
    super::with_arena!(|arena| arena.set_error(message));
    false
}

/// Turn a registry result into the `bool` the seam returns.
fn report(outcome: Result<(), ProtocolError>) -> bool {
    match outcome {
        Ok(()) => true,
        Err(error) => fail(&error),
    }
}

/// Read a C string argument. `None` on null or non-UTF-8.
///
/// # Safety
/// `ptr` must be null or a valid null-terminated C string.
unsafe fn read_cstr(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    CStr::from_ptr(ptr).to_str().ok().map(str::to_string)
}

// ============================================================================
// Registration (five externs)
// ============================================================================

/// Start a fresh definition environment, discarding the previous pass's
/// definitions, its plan and its rejections.
#[no_mangle]
pub extern "C" fn tg_termination_reset() {
    with_registry(TerminationRegistry::reset);
}

/// Record one item's attributes and role.
///
/// `decreasing` is the `#[decreasing(arg)]` parameter name, or **null** for
/// none — unlike the positivity seam's field names, an absent argument here is
/// not confusable with a handle, so null is the honest spelling. `proof` is
/// true for a theorem, lemma or axiom: the same four item kinds the bootstrap's
/// `record_termination_meta` matches on.
///
/// # Safety
/// `name` must be a valid null-terminated C string; `decreasing` must be null
/// or one.
#[no_mangle]
pub unsafe extern "C" fn tg_termination_note_item(
    name: *const c_char,
    partial: bool,
    decreasing: *const c_char,
    proof: bool,
) -> bool {
    let Some(name) = read_cstr(name) else {
        return fail(&ProtocolError::UnreadableName("note_item"));
    };
    let note = ItemNote {
        annotation: TerminationAnnotation {
            partial,
            decreasing: read_cstr(decreasing),
        },
        proof,
    };
    report(with_registry(|registry| registry.note_item(&name, note)))
}

/// Declare one definition name, fixing the node set.
///
/// Every definition must be declared before any is reduced: mention sets are
/// filtered against the declared names, so a definition reduced early would
/// record no edge to a callee declared later.
///
/// # Safety
/// `name` must be a valid null-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn tg_termination_declare(name: *const c_char) -> bool {
    let Some(name) = read_cstr(name) else {
        return fail(&ProtocolError::UnreadableName("declare"));
    };
    report(with_registry(|registry| registry.declare(&name)))
}

/// Offer one definition's payload.
///
/// Called twice over the same stream: before [`tg_termination_plan`] it is
/// reduced and dropped, after it is retained if the plan wants it.
///
/// # Safety
/// `name` must be a valid null-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn tg_termination_add_def(
    name: *const c_char,
    ty: TypeHandle,
    term: TermHandle,
) -> bool {
    let Some(name) = read_cstr(name) else {
        return fail(&ProtocolError::UnreadableName("add_def"));
    };
    let payload =
        with_arena_ref!(|arena| (materialize_type(arena, ty), materialize_term(arena, term)));
    let (Some(ty), Some(term)) = payload else {
        return fail(&ProtocolError::UnreadablePayload(name));
    };
    report(with_registry(|registry| registry.add_def(&name, ty, term)))
}

/// Close the reduction stream and decide what the second stream must retain.
///
/// Returns how many definitions the plan holds — the members of every
/// recursive component, which is what descent reads, or **0** when
/// `with_descent` is false and only the taint half is being mirrored. A caller
/// that gets 0 back has nothing to retain and may skip its second stream.
#[no_mangle]
pub extern "C" fn tg_termination_plan(with_descent: bool) -> u64 {
    match with_registry(|registry| registry.plan(with_descent)) {
        Ok(planned) => planned as u64,
        Err(error) => {
            fail(&error);
            0
        }
    }
}

// ============================================================================
// Readers (three)
// ============================================================================

/// Run the analysis over everything registered. Returns the rejection count.
///
/// Nullary because the environment is ambient: the engine's input is a map of
/// owned `String`s rather than an arena object, so there is no handle to pass.
#[no_mangle]
pub extern "C" fn tg_termination_check() -> u64 {
    match with_registry(TerminationRegistry::check) {
        Ok(failures) => failures as u64,
        Err(error) => {
            fail(&error);
            0
        }
    }
}

/// How many rejections the last [`tg_termination_check`] found.
#[no_mangle]
pub extern "C" fn tg_termination_failure_count() -> u64 {
    with_registry(|registry| registry.failures().len() as u64)
}

/// The `i`th rejection, as `"<kind>\n<function>\n<message>"`.
///
/// Indexed rather than handle-returning because the analysis produces a *list*:
/// one program can fail termination in several places, and a single-handle
/// return would render the first and drop the rest. Null on an out-of-range
/// index — the caller bounds its loop with [`tg_termination_failure_count`].
///
/// The returned pointer is leaked, like every other C string this FFI hands
/// out; rejections are bounded by the corpus's defect count, not by its size.
#[no_mangle]
pub extern "C" fn tg_termination_failure_render(index: u64) -> *const c_char {
    let rendered =
        with_registry(|registry| registry.failures().get(index as usize).map(render_failure));
    match rendered.and_then(|text| CString::new(text).ok()) {
        Some(cstring) => cstring.into_raw().cast_const(),
        None => std::ptr::null(),
    }
}
