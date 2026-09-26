//! The strict-positivity mirror's FFI seam (ADR 18.8.26b P1).
//!
//! The self-hosted elaborator holds per-field `TypeHandle`s at exactly the
//! point registration needs them — `elaborate_adt_body` has its
//! `List<AdtVariant>` before `encode_fields_to_product` folds it — so these
//! externs read *arena objects*, never a decomposed product. Registering from
//! the folded product would mean re-splitting it against the encoder's nesting
//! convention, which is a fidelity risk with no upside.
//!
//! **Six registration externs and three readers.** The registration API is
//! what the parent ADR bounded at ≤ 8; the readers are carved out of that
//! budget because the analysis returns a *list* — one cluster can violate in
//! several fields, so a single-handle return would render the first violation
//! and silently drop the rest.
//!
//! The environment is **ambient** (D6): [`PositivityDefs`] is a map of owned
//! `String`s rather than an arena object, so handing it back as a handle would
//! mean arena-allocating a structure nothing else in the arena reads. That is
//! sound only while a collection pass is single-threaded and registration is
//! not re-entrant — R2 tracks the premise, and the fix if it breaks is a
//! handle-per-environment, not a lock.
//!
//! Every failure path stores a message reachable through `tg_get_last_error`
//! and returns `false`, in line with the rest of this FFI. Nothing here
//! silently ignores a call: a dropped field is a definition that passes
//! positivity because the offending occurrence never arrived.

#![allow(unsafe_code)]

mod registry;
mod wire;

#[cfg(test)]
mod tests;

use std::cell::RefCell;
use std::ffi::{c_char, CStr, CString};

use super::types::nodes::materialize_type;
use super::{with_arena_ref, TypeHandle, INVALID_HANDLE};
use registry::PositivityRegistry;
use wire::{render_violation, ProtocolError};

thread_local! {
    /// The ambient environment (D6). Thread-local like [`super::ARENA`], and
    /// for the same reason: the FFI is single-threaded by contract.
    static REGISTRY: RefCell<PositivityRegistry> = RefCell::new(PositivityRegistry::default());
}

/// Access the registry mutably.
fn with_registry<T>(body: impl FnOnce(&mut PositivityRegistry) -> T) -> T {
    REGISTRY.with(|cell| body(&mut cell.borrow_mut()))
}

/// Record a protocol error where `tg_get_last_error` will find it, and report
/// failure to the caller.
fn fail(error: &ProtocolError) -> bool {
    let message = error.message();
    super::with_arena!(|arena| arena.set_error(message));
    false
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

/// Rebuild the owned `Type` a handle names. `None` on a dangling handle.
fn read_type(handle: TypeHandle) -> Option<crate::types::Type> {
    with_arena_ref!(|arena| materialize_type(arena, handle))
}

// ============================================================================
// Registration (six externs — D1)
// ============================================================================

/// Start a fresh definition environment, discarding the previous pass's
/// definitions **and** its violations.
#[no_mangle]
pub extern "C" fn tg_positivity_reset() {
    with_registry(PositivityRegistry::reset);
}

/// Open a definition. `kind` is `0 = ADT, 1 = record, 2 = alias, 3 = stub`;
/// any other value fails rather than defaulting.
///
/// # Safety
/// `name` must be a valid null-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn tg_positivity_def_begin(name: *const c_char, kind: u64) -> bool {
    let Some(name) = read_cstr(name) else {
        return fail(&ProtocolError::NoCurrentDefinition("def_begin"));
    };
    match with_registry(|registry| registry.def_begin(&name, kind)) {
        Ok(()) => true,
        Err(error) => fail(&error),
    }
}

/// Add one type parameter to the open definition, in declaration order.
///
/// # Safety
/// `name` must be a valid null-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn tg_positivity_add_param(name: *const c_char) -> bool {
    let Some(name) = read_cstr(name) else {
        return fail(&ProtocolError::NoCurrentDefinition("add_param"));
    };
    match with_registry(|registry| registry.add_param(&name)) {
        Ok(()) => true,
        Err(error) => fail(&error),
    }
}

/// Open a constructor on the current definition.
///
/// # Safety
/// `name` must be a valid null-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn tg_positivity_ctor_begin(name: *const c_char) -> bool {
    let Some(name) = read_cstr(name) else {
        return fail(&ProtocolError::NoCurrentDefinition("ctor_begin"));
    };
    match with_registry(|registry| registry.ctor_begin(&name)) {
        Ok(()) => true,
        Err(error) => fail(&error),
    }
}

/// Add one field to the open constructor.
///
/// `name` is the field's name for a record and **`INVALID_HANDLE` cast to a
/// pointer** for a positional ADT field. `INVALID_HANDLE` is the *only*
/// positional spelling: a **null** pointer fails, because `0` is a valid arena
/// index and a seam that accepted it as "unnamed" would turn a lost name into
/// a silently positional field rather than an error.
///
/// # Safety
/// `name` must be `INVALID_HANDLE`, null, or a valid null-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn tg_positivity_add_field(name: *const c_char, ty: TypeHandle) -> bool {
    let named = if name as u64 == INVALID_HANDLE {
        None
    } else {
        match read_cstr(name) {
            Some(name) => Some(name),
            None => return fail(&ProtocolError::UnreadableFieldName),
        }
    };
    let Some(ty) = read_type(ty) else {
        return fail(&ProtocolError::NoCurrentConstructor);
    };
    match with_registry(|registry| registry.add_field(named.as_deref(), ty)) {
        Ok(()) => true,
        Err(error) => fail(&error),
    }
}

/// Set the open alias's body.
#[no_mangle]
pub extern "C" fn tg_positivity_set_alias_body(ty: TypeHandle) -> bool {
    let Some(ty) = read_type(ty) else {
        return fail(&ProtocolError::NoCurrentDefinition("set_alias_body"));
    };
    match with_registry(|registry| registry.set_alias_body(ty)) {
        Ok(()) => true,
        Err(error) => fail(&error),
    }
}

// ============================================================================
// Readers (three — D6's carve-out)
// ============================================================================

/// Close the stream and run the analysis. Returns the violation count.
///
/// Nullary because the environment is ambient: there is no `defs` handle to
/// pass, and inventing one would arena-allocate a structure nothing else in
/// the arena reads.
#[no_mangle]
pub extern "C" fn tg_positivity_check() -> u64 {
    with_registry(|registry| registry.check() as u64)
}

/// How many violations the last [`tg_positivity_check`] found.
#[no_mangle]
pub extern "C" fn tg_positivity_violation_count() -> u64 {
    with_registry(|registry| registry.violation_count() as u64)
}

/// The `i`th violation, as `"<type name>\n<message>"`.
///
/// Indexed rather than handle-returning because `analyze` produces a *list*:
/// one cluster can violate in several fields, and a single-handle return would
/// render the first and drop the rest. Null on an out-of-range index — the
/// caller bounds its loop with [`tg_positivity_violation_count`].
///
/// The returned pointer is leaked, like every other C string this FFI hands
/// out; violations are bounded by the corpus's defect count, not by its size.
#[no_mangle]
pub extern "C" fn tg_positivity_violation_render(index: u64) -> *const c_char {
    let rendered =
        with_registry(|registry| registry.violation_at(index as usize).map(render_violation));
    match rendered.and_then(|text| CString::new(text).ok()) {
        Some(cstring) => cstring.into_raw().cast_const(),
        None => std::ptr::null(),
    }
}
