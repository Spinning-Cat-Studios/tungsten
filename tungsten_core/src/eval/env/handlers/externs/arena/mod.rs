//! Type-arena externs the evaluator executes (ADR 7.8.26c).
//!
//! `type_handle_to_codegen_type` (`src/compiler/elab/cir/constructors.tg`)
//! destructures a `TypeHandle` through the type-arena FFI. Without these arms
//! the whole expression is a residual, so the six
//! `test_type_handle_to_codegen_type_*` tests reached their `assert_eq_string`
//! with a Stuck argument and reported `ok` while asserting nothing.
//!
//! ## Arena lifetime
//!
//! The arena is `thread_local!` and **grow-only** (ADR 2.7.26a): handles are
//! plain `Vec` indices and nothing is ever freed. So evaluated test bodies
//! share one arena and **no reset is performed** — a per-test reset would be
//! *unsound*, not merely wasteful, because truncating `types` makes an
//! already-issued handle silently resolve to a different node.
//! `INVALID_HANDLE` (`u64::MAX`) can only catch an out-of-range index, so it
//! would never fire on that aliasing. `tests::an_earlier_handle_survives_later_allocations`
//! is the executable half of that argument, and `tg_init` — the one reset
//! entry point in the crate — is deliberately **not** claimed here.
//!
//! Cross-test visibility is therefore permitted and unreachable in practice: a
//! handle is an opaque index no test body can name without having been handed
//! it, and the arena exposes no enumeration API.
//!
//! ## Every arm calls the real symbol
//!
//! No arm re-derives what a `tg_*` function does — in particular none of them
//! transcribes the tag table, which ADR 13.8.26a just collapsed to a single
//! authority (`tg_type_tag`'s own `match`). Calling through means the
//! evaluator cannot drift from codegen by construction. The three symbols
//! that are `unsafe` or hand back a raw fat pointer are reached through the
//! safe wrappers in `crate::ffi::evaluator_bridges`, because the workspace
//! allows `unsafe_code` in FFI modules only.
//!
//! ## Leaks
//!
//! `tg_type_get_mu_var` and friends leak a `CString` per call (`into_raw`),
//! and the evaluator has no free. Bounded by the test run and identical to
//! what the compiled path already does; accepted, not fixed.

use std::os::raw::c_char;

use crate::eval::{nat_to_term, term_to_nat, StepResult};
use crate::ffi::types::{accessors, accessors_introspection, constructors, predicates};
use crate::ffi::TypeHandle;
use crate::terms::Term;

/// Execute a type-arena extern, or return `None` if `name` is not one.
///
/// `None` (rather than `Stuck`) so the caller goes on to try its other
/// dispatch arms — this module claims only the calls it recognizes, and a
/// wrong-arity call is *not* recognized.
pub(super) fn step_arena_extern(name: &str, values: &[Term]) -> Option<StepResult> {
    step_type_constructor(name, values)
        .or_else(|| step_handle_accessor(name, values))
        .or_else(|| step_cstring_bridge(name, values))
}

// ===========================================================================
// Constructors — the arms that allocate
// ===========================================================================

/// The six constructors the six tests build their handles with.
fn step_type_constructor(name: &str, values: &[Term]) -> Option<StepResult> {
    match (name, values) {
        ("tg_type_nat", []) => Some(stepped_handle(constructors::tg_type_nat())),
        ("tg_type_bool", []) => Some(stepped_handle(constructors::tg_type_bool())),
        ("tg_type_unit", []) => Some(stepped_handle(constructors::tg_type_unit())),
        ("tg_type_arrow", [domain, codomain]) => Some(step_binary_constructor(
            domain,
            codomain,
            constructors::tg_type_arrow,
        )),
        ("tg_type_sum", [left, right]) => Some(step_binary_constructor(
            left,
            right,
            constructors::tg_type_sum,
        )),
        ("tg_type_mu", [name_address, body]) => Some(step_type_mu(name_address, body)),
        _ => None,
    }
}

/// Marshal two handle operands and allocate the node `construct` names.
///
/// Named for the *constructor* it drives rather than merely its arity:
/// this crate already has a `step_binary_*` family in `eval/helpers.rs`
/// (`step_binary_nat`, `step_binary_bool`, `step_binary_nat_compare` and
/// their `_env` variants) meaning "step a binary OPERATOR over Nat/Bool
/// operands". A bare `step_binary` here reads as one of those and is not.
///
/// A non-`Nat` operand stays `Stuck` rather than being coerced: a
/// misinterpreted handle would allocate a node pointing at whatever index the
/// bad cast produced, which reads back as a plausible type.
fn step_binary_constructor(
    left: &Term,
    right: &Term,
    construct: extern "C" fn(TypeHandle, TypeHandle) -> TypeHandle,
) -> StepResult {
    match (term_to_nat(left), term_to_nat(right)) {
        (Some(left), Some(right)) => {
            stepped_handle(construct(left as TypeHandle, right as TypeHandle))
        }
        _ => StepResult::Stuck,
    }
}

/// `μ<name>. <body>`, whose binder arrives as a C-string address rather than
/// a handle — hence its own arm instead of a slot in [`step_binary_constructor`].
fn step_type_mu(name_address: &Term, body: &Term) -> StepResult {
    match (term_to_nat(name_address), term_to_nat(body)) {
        (Some(name_address), Some(body)) => stepped_handle(crate::ffi::mu_type_from_cstr(
            name_address,
            body as TypeHandle,
        )),
        _ => StepResult::Stuck,
    }
}

// ===========================================================================
// Accessors — the pure reads
// ===========================================================================

/// The handle-in / handle-out reads, by name.
///
/// `tg_type_tag` shares the table because `TypeHandle` *is* `u64`: its
/// signature is identical, and its result is a tag rather than a handle only
/// by convention at the call site.
fn handle_accessor(name: &str) -> Option<extern "C" fn(TypeHandle) -> TypeHandle> {
    Some(match name {
        "tg_type_tag" => predicates::tg_type_tag,
        "tg_type_get_arrow_domain" => accessors::tg_type_get_arrow_domain,
        "tg_type_get_arrow_codomain" => accessors::tg_type_get_arrow_codomain,
        "tg_type_get_sum_left" => accessors::tg_type_get_sum_left,
        "tg_type_get_sum_right" => accessors::tg_type_get_sum_right,
        "tg_type_get_product_left" => accessors::tg_type_get_product_left,
        "tg_type_get_product_right" => accessors::tg_type_get_product_right,
        "tg_type_get_mu_body" => accessors::tg_type_get_mu_body,
        "tg_type_get_forall_body" => accessors::tg_type_get_forall_body,
        _ => return None,
    })
}

/// Read one component out of a node, or `None` if `name` is not an accessor.
fn step_handle_accessor(name: &str, values: &[Term]) -> Option<StepResult> {
    let read = handle_accessor(name)?;
    let [handle] = values else {
        return None;
    };
    let Some(handle) = term_to_nat(handle) else {
        return Some(StepResult::Stuck);
    };
    Some(stepped_handle(read(handle as TypeHandle)))
}

// ===========================================================================
// C-string bridges — the arms that allocate a leaked CString
// ===========================================================================

/// The binder-name accessors, by name. Each leaks the `CString` it returns.
fn cstring_accessor(name: &str) -> Option<extern "C" fn(TypeHandle) -> *const c_char> {
    Some(match name {
        "tg_type_get_mu_var" => accessors_introspection::tg_type_get_mu_var,
        "tg_type_get_forall_var" => accessors_introspection::tg_type_get_forall_var,
        "tg_type_get_tyvar_name" => accessors_introspection::tg_type_get_tyvar_name,
        _ => return None,
    })
}

/// The three arms that move a name across the `String` ↔ C-string boundary.
///
/// `tg_string_to_cstr` is declared `(s: String) -> Nat` in `.tg`, so it
/// arrives as **one** `StringLit`, the way `tg_string_to_cstring` does — its
/// Rust `(ptr, len)` pair is the C ABI, not the Core arity. `assert_eq_string`
/// matching four terms is a different thing: *its* `.tg` declaration really
/// does take four parameters.
fn step_cstring_bridge(name: &str, values: &[Term]) -> Option<StepResult> {
    match (name, values) {
        ("tg_string_to_cstr", [Term::StringLit(text)]) => Some(stepped_handle(
            crate::ffi::cstr_address_of(text) as TypeHandle,
        )),
        ("tg_cstring_to_string", [address]) => Some(step_cstring_to_string(address)),
        (_, [handle]) => step_cstring_accessor(name, handle),
        _ => None,
    }
}

/// Read a binder name out of a node as a C-string address.
fn step_cstring_accessor(name: &str, handle: &Term) -> Option<StepResult> {
    let read = cstring_accessor(name)?;
    let Some(handle) = term_to_nat(handle) else {
        return Some(StepResult::Stuck);
    };
    Some(stepped_handle(
        read(handle as TypeHandle) as usize as TypeHandle
    ))
}

/// Read the C string at an address back into a `.tg` `String`.
fn step_cstring_to_string(address: &Term) -> StepResult {
    match term_to_nat(address) {
        Some(address) => {
            StepResult::Stepped(Term::StringLit(crate::ffi::string_at_cstr_address(address)))
        }
        None => StepResult::Stuck,
    }
}

/// Step to a handle (or a tag, or an address — all `Nat` on the `.tg` side).
///
/// Routed through `nat_to_term`, and so through `Term::nat_smart`: a
/// hand-rolled `Succ` chain would re-open ADR 21.7.26e's deep-Peano wedge the
/// moment an accessor returned `INVALID_HANDLE` (`u64::MAX`).
fn stepped_handle(handle: TypeHandle) -> StepResult {
    StepResult::Stepped(nat_to_term(handle as usize))
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
