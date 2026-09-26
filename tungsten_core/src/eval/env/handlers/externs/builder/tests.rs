//! Tests for the `StringBuilder` arms (ADR 14.9.26a): marshalling per arm,
//! against the real runtime.

use crate::eval::{nat_to_term, term_to_nat, StepResult};
use crate::terms::Term;

use super::step_builder_extern;

/// Step `name` with `values` and read the resulting `Nat` back as a `usize`.
///
/// Panics rather than returning `Option` so a `Stuck` arm names itself in the
/// failure, instead of surfacing as a `None` three assertions later.
fn stepped_nat(name: &str, values: &[Term]) -> usize {
    match stepped(name, values) {
        Term::StringLit(text) => panic!("`{name}` stepped to a String, not a Nat: {text:?}"),
        term => {
            term_to_nat(&term).unwrap_or_else(|| panic!("`{name}` stepped to a non-Nat: {term:?}"))
        }
    }
}

/// Step `name` with `values` and return the term it stepped to.
fn stepped(name: &str, values: &[Term]) -> Term {
    let result = step_builder_extern(name, values)
        .unwrap_or_else(|| panic!("`{name}` is not claimed by the builder dispatcher"));
    match result {
        StepResult::Stepped(term) => term,
        other => panic!("`{name}` did not step on {values:?}: {other:?}"),
    }
}

/// A fresh builder's handle, as the `.tg` side would hold it.
fn fresh_builder() -> Term {
    nat_to_term(stepped_nat("tg_string_builder_new", &[]))
}

fn string_lit(text: &str) -> Term {
    Term::StringLit(text.to_string())
}

// ===========================================================================
// The six arms
// ===========================================================================

#[test]
fn new_steps_to_a_non_zero_handle_with_length_zero() {
    let sb = fresh_builder();
    assert_ne!(term_to_nat(&sb), Some(0));
    assert_eq!(
        stepped_nat("tg_string_builder_len", std::slice::from_ref(&sb)),
        0
    );
}

#[test]
fn with_capacity_steps_to_an_empty_builder() {
    let sb = nat_to_term(stepped_nat(
        "tg_string_builder_with_capacity",
        &[nat_to_term(64)],
    ));
    assert_eq!(
        stepped_nat("tg_string_builder_len", std::slice::from_ref(&sb)),
        0
    );
    assert_eq!(
        stepped("tg_string_builder_to_string", &[sb]),
        string_lit("")
    );
}

/// `push_str` takes the `String` as ONE `StringLit` and steps to the same
/// handle it was given — the fold-through contract.
#[test]
fn push_str_returns_the_same_handle_and_len_tracks_bytes() {
    let sb = fresh_builder();
    let returned = stepped(
        "tg_string_builder_push_str",
        &[sb.clone(), string_lit("héllo")],
    );
    assert_eq!(returned, sb, "push_str must return the handle it was given");
    assert_eq!(
        stepped_nat("tg_string_builder_len", std::slice::from_ref(&sb)),
        6,
        "len is bytes, and é is two of them"
    );
}

#[test]
fn push_char_appends_the_scalar_as_utf8() {
    let sb = fresh_builder();
    let returned = stepped(
        "tg_string_builder_push_char",
        &[sb.clone(), nat_to_term(0x1F600)],
    );
    assert_eq!(returned, sb);
    assert_eq!(
        stepped("tg_string_builder_to_string", &[sb]),
        string_lit("😀")
    );
}

/// The whole fold, end to end, through the arms only.
#[test]
fn pushes_then_to_string_equal_the_concatenation() {
    let sb = fresh_builder();
    for piece in ["one", ", ", "two", ", ", "three"] {
        stepped(
            "tg_string_builder_push_str",
            &[sb.clone(), string_lit(piece)],
        );
    }
    assert_eq!(
        stepped("tg_string_builder_to_string", &[sb]),
        string_lit("one, two, three")
    );
}

// ===========================================================================
// What the dispatcher refuses
// ===========================================================================

/// A wrong-arity call is not recognised, so the caller's other arms get a
/// turn — the same contract as the arena and console dispatchers.
#[test]
fn a_wrong_arity_call_is_not_claimed() {
    assert!(step_builder_extern("tg_string_builder_new", &[nat_to_term(1)]).is_none());
    assert!(step_builder_extern("tg_string_builder_len", &[]).is_none());
    assert!(step_builder_extern("tg_string_builder_push_str", &[fresh_builder()]).is_none());
}

/// A `push_str` whose second operand is not a `StringLit` is not claimed
/// either: there is no `String` to push.
#[test]
fn push_str_with_a_non_string_operand_is_not_claimed() {
    assert!(step_builder_extern(
        "tg_string_builder_push_str",
        &[fresh_builder(), nat_to_term(7)]
    )
    .is_none());
}

#[test]
fn an_unrelated_name_is_not_claimed() {
    assert!(step_builder_extern("tg_string_concat", &[]).is_none());
}

/// A non-`Nat` handle is claimed and left `Stuck` rather than coerced — the
/// runtime would otherwise read a header at whatever address the bad cast
/// produced.
#[test]
fn a_non_nat_handle_stays_stuck() {
    let not_a_handle = string_lit("not a handle");
    for (name, values) in [
        ("tg_string_builder_len", vec![not_a_handle.clone()]),
        ("tg_string_builder_to_string", vec![not_a_handle.clone()]),
        (
            "tg_string_builder_with_capacity",
            vec![not_a_handle.clone()],
        ),
        (
            "tg_string_builder_push_str",
            vec![not_a_handle.clone(), string_lit("x")],
        ),
        (
            "tg_string_builder_push_char",
            vec![not_a_handle.clone(), nat_to_term(0x41)],
        ),
        (
            "tg_string_builder_push_char",
            vec![fresh_builder(), not_a_handle.clone()],
        ),
    ] {
        assert_eq!(
            step_builder_extern(name, &values),
            Some(StepResult::Stuck),
            "`{name}` must be claimed and Stuck on a non-Nat operand"
        );
    }
}
