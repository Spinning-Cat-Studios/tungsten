//! The post-elaboration diagnostics externs (`tg_diagnostic_*`).
//!
//! [`tg_diagnostic_free_term_vars`] is the one under real load: it answers
//! "is this elaborated body closed?", a question nothing asked until ADR
//! 19.8.26d found a compiler emitting terms whose match arms reference
//! variables the arm never binds. These tests drive it through the arena, the
//! way its `.tg` caller does, because a `Term::free_vars` unit test would not
//! exercise the handle or the C-string it hands back.

use std::ffi::CStr;

use crate::ffi::driver::{
    tg_diagnostic_core_term, tg_diagnostic_free_term_vars, tg_diagnostic_shape_mismatches,
    tg_free_string,
};
use crate::ffi::terms::core::{
    tg_term_app, tg_term_global, tg_term_lambda, tg_term_let, tg_term_var_named,
};
use crate::ffi::terms::core_data::{tg_term_fst, tg_term_zero};
use crate::ffi::types::constructors::{tg_type_nat, tg_type_product};
use crate::ffi::{tg_init, TermHandle, INVALID_HANDLE};

/// A leaked null-terminated C string, as every caller of this seam supplies.
fn cstr(text: &str) -> *const std::ffi::c_char {
    std::ffi::CString::new(text)
        .expect("no interior nul")
        .into_raw()
        .cast_const()
}

/// The extern's answer, as the `.tg` caller reads it: `None` when closed.
fn free_vars_of(term: TermHandle) -> Option<String> {
    let ptr = tg_diagnostic_free_term_vars(term);
    if ptr.is_null() {
        return None;
    }
    let text = unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .expect("utf-8")
        .to_string();
    tg_free_string(ptr);
    Some(text)
}

#[test]
fn a_closed_term_reports_nothing() {
    tg_init();
    // λx:Nat. x — the variable is bound, so the term is closed.
    let body = unsafe { tg_term_var_named(cstr("x")) };
    let lambda = unsafe { tg_term_lambda(cstr("x"), tg_type_nat(), body) };
    assert_eq!(
        free_vars_of(lambda),
        None,
        "a closed term answers null, not an empty string — the caller's \
         clean test is a null check"
    );
}

#[test]
fn an_unbound_variable_is_reported_by_name() {
    tg_init();
    // λx:Nat. t — `t` is free, which is exactly the shape a match arm takes
    // when its payload projections are never emitted.
    let body = unsafe { tg_term_var_named(cstr("t")) };
    let lambda = unsafe { tg_term_lambda(cstr("x"), tg_type_nat(), body) };
    assert_eq!(free_vars_of(lambda), Some("t".to_string()));
}

#[test]
fn several_free_variables_come_back_sorted_and_deduplicated() {
    tg_init();
    // (h t) t — `t` occurs twice and must be named once, and the order is
    // sorted so a caller can diff two censuses.
    let inner = unsafe { tg_term_app(tg_term_var_named(cstr("h")), tg_term_var_named(cstr("t"))) };
    let outer = tg_term_app(inner, unsafe { tg_term_var_named(cstr("t")) });
    assert_eq!(free_vars_of(outer), Some("h, t".to_string()));
}

#[test]
fn a_global_reference_is_not_a_free_variable() {
    tg_init();
    // `global:f 0` — a definition mentioning another definition is closed.
    // Without this the census would flag every call site in the corpus.
    let call = unsafe { tg_term_app(tg_term_global(cstr("f")), tg_term_zero()) };
    assert_eq!(free_vars_of(call), None);
}

#[test]
fn an_invalid_handle_reports_nothing_rather_than_claiming_the_term_is_closed() {
    tg_init();
    // Both answer null, which is the seam's one weakness: the caller cannot
    // tell "closed" from "unreadable" by the return value alone. It is
    // recorded here rather than papered over, because the `.tg` caller counts
    // definitions EXAMINED separately for exactly this reason.
    assert_eq!(free_vars_of(INVALID_HANDLE), None);
}

/// The rendered term, as the differ's `.tg` caller reads it.
fn core_term_of(term: TermHandle) -> Option<String> {
    let ptr = tg_diagnostic_core_term(term);
    if ptr.is_null() {
        return None;
    }
    let text = unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .expect("utf-8")
        .to_string();
    tg_free_string(ptr);
    Some(text)
}

#[test]
fn a_term_renders_unframed() {
    tg_init();
    // The differ compares this against the bootstrap's own `format!("{term}")`,
    // so the output must be the term ALONE — no box, no label, no type line.
    let lambda = unsafe { tg_term_lambda(cstr("x"), tg_type_nat(), tg_term_zero()) };
    let rendered = core_term_of(lambda).expect("a valid handle renders");
    assert!(rendered.contains('x'), "{rendered}");
    assert!(
        !rendered.contains('│') && !rendered.contains("Definition:"),
        "the framed form would break the differ's comparison: {rendered}"
    );
}

#[test]
fn the_rendering_matches_the_terms_own_display() {
    tg_init();
    // The premise the whole comparison rests on: both compilers render through
    // one `impl Display for Term`, so equal terms give equal strings. If this
    // ever diverges, `diff selfhost-core` reports differences that are not real.
    let handle = unsafe { tg_term_app(tg_term_global(cstr("f")), tg_term_zero()) };
    let materialized = crate::ffi::ARENA
        .with(|cell| crate::ffi::terms::nodes::materialize_term(&cell.borrow(), handle))
        .expect("valid handle");
    assert_eq!(core_term_of(handle), Some(format!("{materialized}")));
}

#[test]
fn an_invalid_handle_renders_nothing() {
    tg_init();
    assert_eq!(core_term_of(INVALID_HANDLE), None);
}

/// The shape census's answer, as the `.tg` caller reads it (ADR 3.9.26h).
fn shape_mismatches_of(term: TermHandle) -> Option<String> {
    let ptr = tg_diagnostic_shape_mismatches(term);
    if ptr.is_null() {
        return None;
    }
    let text = unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .expect("utf-8")
        .to_string();
    tg_free_string(ptr);
    Some(text)
}

/// `let x : ty = 0 in fst x`, built through the arena the way the self-host
/// builds it.
fn projection_over(ty: crate::ffi::TypeHandle) -> TermHandle {
    let body = tg_term_fst(unsafe { tg_term_var_named(cstr("x")) });
    unsafe { tg_term_let(cstr("x"), ty, tg_term_zero(), body) }
}

#[test]
fn a_projection_over_a_product_reports_nothing() {
    tg_init();
    let pair = tg_type_product(tg_type_nat(), tg_type_nat());
    assert_eq!(
        shape_mismatches_of(projection_over(pair)),
        None,
        "a clean term answers null, not an empty string"
    );
}

#[test]
fn a_projection_over_a_scalar_names_the_eliminator_and_the_former() {
    tg_init();
    // ADR 3.9.26g's shape: `fst` of something no recorded type says is a pair.
    // Closed, well-typed through the environment, and Stuck at run time.
    assert_eq!(
        shape_mismatches_of(projection_over(tg_type_nat())),
        Some("fst over Nat".to_string())
    );
}

#[test]
fn a_closed_term_can_still_be_ill_shaped() {
    tg_init();
    // The premise of the whole check: `closed-terms` passes this and the shape
    // census does not, over one and the same handle.
    let term = projection_over(tg_type_nat());
    assert_eq!(free_vars_of(term), None, "closed");
    assert!(shape_mismatches_of(term).is_some(), "and ill-shaped");
}

#[test]
fn an_invalid_handle_reports_no_shape_findings() {
    tg_init();
    assert_eq!(shape_mismatches_of(INVALID_HANDLE), None);
}
