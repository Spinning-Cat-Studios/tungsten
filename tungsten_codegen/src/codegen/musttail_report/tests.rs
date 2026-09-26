//! Unit tests for the structured musttail decision records.
//!
//! Split out of `mod.rs` (ADR 5.8.26a) when the file crossed the 400-line
//! ceiling — the seam is types-and-aggregation vs their assertions, not an
//! arbitrary halving.

use super::*;

use super::*;

fn skip(fn_name: &str, reasons: Vec<ReasonCode>) -> MusttailDecision {
    MusttailDecision {
        function: fn_name.to_string(),
        decision: Decision::Skip,
        reasons,
        blockers: Vec::new(),
        lowered_sig: String::new(),
        param_abi: Vec::new(),
        sret: false,
        slot_attrs: Vec::new(),
    }
}

fn outcome(fn_name: &str, d: Decision) -> MusttailDecision {
    MusttailDecision {
        function: fn_name.to_string(),
        decision: d,
        reasons: Vec::new(),
        blockers: Vec::new(),
        lowered_sig: String::new(),
        param_abi: Vec::new(),
        sret: false,
        slot_attrs: Vec::new(),
    }
}

#[test]
fn reason_codes_are_stable() {
    assert_eq!(ReasonCode::StructReturn.code(), "STRUCT_RETURN");
    assert_eq!(ReasonCode::StructParam.code(), "STRUCT_PARAM");
    assert_eq!(
        ReasonCode::NonFlattenableParam.code(),
        "NON_FLATTENABLE_PARAM"
    );
    assert_eq!(
        ReasonCode::AbiSignatureMismatch.code(),
        "ABI_SIGNATURE_MISMATCH"
    );
}

#[test]
fn base_name_strips_direct_suffixes() {
    assert_eq!(skip("foo$direct", vec![]).base_name(), "foo");
    assert_eq!(skip("foo$direct_mt", vec![]).base_name(), "foo");
    assert_eq!(skip("foo", vec![]).base_name(), "foo");
}

#[test]
fn decompose_counts_as_constant_stack() {
    assert!(Decision::Emit.is_constant_stack());
    assert!(Decision::Decompose.is_constant_stack());
    assert!(!Decision::Skip.is_constant_stack());
}

#[test]
fn function_outcome_prefers_constant_stack() {
    // A flattenable-param fn: $direct skips, $direct_mt decomposes → safe.
    let mut r = MusttailReport::new();
    r.push(skip("foo$direct", vec![ReasonCode::StructParam]));
    r.push(outcome("foo$direct_mt", Decision::Decompose));
    assert_eq!(r.function_outcome("foo"), Some(Decision::Decompose));
}

#[test]
fn function_outcome_skip_only_stays_skip() {
    let mut r = MusttailReport::new();
    r.push(skip("bar$direct", vec![ReasonCode::StructReturn]));
    assert_eq!(r.function_outcome("bar"), Some(Decision::Skip));
}

#[test]
fn function_outcome_absent_is_none() {
    let r = MusttailReport::new();
    assert_eq!(r.function_outcome("missing"), None);
}

#[test]
fn function_outcome_ignores_a_non_self_tail_edge_at_the_same_name() {
    // ADR 5.8.26a. `helper` is tail-CALLED but does not recurse, so it has
    // no self-recursive outcome — `Some(Skip)` here would claim it grows
    // its own stack. The name matching is what makes this the sharp case:
    // the guard must skip the row for being non-self even though its base
    // name is the one asked about.
    let mut r = MusttailReport::new();
    r.push(outcome("helper$direct", Decision::SkipNonSelf));
    assert_eq!(r.function_outcome("helper"), None);

    // And it must not mask a real self-recursive SKIP on the same function.
    r.push(skip("helper$direct", vec![ReasonCode::StructReturn]));
    assert_eq!(r.function_outcome("helper"), Some(Decision::Skip));
}
