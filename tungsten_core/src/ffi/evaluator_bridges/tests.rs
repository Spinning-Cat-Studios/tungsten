//! Round-trip tests for the safe wrappers (ADR 7.8.26c D5; the `StringBuilder`
//! pair, ADR 14.9.26a).
//!
//! These are the only place `unsafe` sits between the evaluator and the arena
//! or the builder runtime, so each wrapper is pinned at both ends: the value it
//! produces on a real input, and the sentinel it produces on the degenerate one.

use super::{
    cstr_address_of, mu_type_from_cstr, string_at_cstr_address, string_builder_push_text,
    string_builder_take_text,
};
use crate::ffi::tg_string_builder_new;
use crate::ffi::types::accessors::tg_type_get_mu_body;
use crate::ffi::types::accessors_introspection::tg_type_get_mu_var;
use crate::ffi::types::constructors::tg_type_nat;
use crate::ffi::types::predicates::tg_type_tag;
use crate::ffi::INVALID_HANDLE;

/// A string survives the C-string round trip byte for byte.
#[test]
fn a_string_round_trips_through_a_cstr_address() {
    let address = cstr_address_of("List");
    assert_ne!(address, 0, "a plain ASCII name must convert");
    assert_eq!(string_at_cstr_address(address), "List");
}

/// Non-ASCII too — the wrapper copies bytes, it does not re-encode.
#[test]
fn a_multibyte_string_round_trips_unchanged() {
    let address = cstr_address_of("μα_List");
    assert_eq!(string_at_cstr_address(address), "μα_List");
}

/// The empty string is a real input on this path, not an error.
#[test]
fn the_empty_string_round_trips_to_itself() {
    let address = cstr_address_of("");
    assert_eq!(string_at_cstr_address(address), "");
}

/// A null address reads back as empty rather than dereferencing.
#[test]
fn a_null_address_reads_back_as_the_empty_string() {
    assert_eq!(string_at_cstr_address(0), "");
}

/// An interior NUL cannot become a C string; the wrapper reports the symbol's
/// null rather than truncating silently.
#[test]
fn an_interior_nul_yields_a_null_address() {
    assert_eq!(cstr_address_of("a\0b"), 0);
}

/// The μ wrapper builds a real arena node, readable back through the
/// accessors — the whole point of routing through the real symbol.
#[test]
fn mu_type_from_cstr_builds_a_readable_mu_node() {
    let body = tg_type_nat();
    let mu = mu_type_from_cstr(cstr_address_of("List"), body);

    assert_eq!(tg_type_tag(mu), 11, "tag 11 is Mu");
    assert_eq!(tg_type_get_mu_body(mu), body);
    assert_eq!(
        string_at_cstr_address(tg_type_get_mu_var(mu) as usize),
        "List"
    );
}

/// A null name address is refused by the symbol, and the wrapper passes that
/// verdict through instead of inventing a node.
#[test]
fn a_null_name_address_yields_an_invalid_mu_handle() {
    assert_eq!(mu_type_from_cstr(0, tg_type_nat()), INVALID_HANDLE);
}

// ---------------------------------------------------------------------------
// The StringBuilder pair (ADR 14.9.26a)
// ---------------------------------------------------------------------------

/// Text pushed through the bridge comes back through the bridge, byte for
/// byte, and the push returns the handle it was given.
#[test]
fn builder_text_round_trips_through_push_and_take() {
    let sb = tg_string_builder_new();
    assert_eq!(string_builder_push_text(sb, "μα_"), sb);
    assert_eq!(string_builder_push_text(sb, "List"), sb);
    assert_eq!(string_builder_take_text(sb), "μα_List");
}

/// An empty builder yields the empty string — the symbol's null `TgString`
/// is read as `""`, not dereferenced.
#[test]
fn an_empty_builder_takes_as_the_empty_string() {
    assert_eq!(string_builder_take_text(tg_string_builder_new()), "");
}
