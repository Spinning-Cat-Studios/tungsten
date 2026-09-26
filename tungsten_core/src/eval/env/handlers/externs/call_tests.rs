//! Tests for the assertion dispatch — the gate, the residual guard, the
//! string arm ADR 6.8.26c added, and the effect counter ADR 14.9.26a added.
//!
//! Split out of `call.rs` alongside its `console_tests.rs`/`registry_tests.rs`
//! siblings, so the string arm's coverage can grow without pushing the dispatch
//! table past the file-size limit.

use std::collections::HashMap;

use super::*;
use crate::eval::env::EvalEnv;
use crate::eval::{nat_to_term, term_to_nat};
use crate::ffi::TgString;

/// The assertion gate must be *selective*. A predicate that answered `true`
/// for every extern would apply the residual-comparison check to unrelated
/// FFI calls, turning an ordinary stuck extern into a reported comparison
/// failure — so both polarities are asserted, not just the positive one.
#[test]
fn only_the_assertion_ffis_are_gated() {
    assert!(is_test_assertion("tg_assert_eq_bool"));
    assert!(is_test_assertion("tg_assert_eq_nat"));
    assert!(is_test_assertion("tg_assert_eq_string"));
    assert!(!is_test_assertion("tg_test_check_failure"));
    assert!(!is_test_assertion("tg_string_compare"));
    assert!(!is_test_assertion("tg_print"));
    assert!(!is_test_assertion(""));
}

/// The name is matched AFTER the `__c_` prefix is stripped, which is what
/// the dispatcher passes; gating the prefixed spelling would match nothing.
#[test]
fn the_gate_matches_the_stripped_name_not_the_c_abi_spelling() {
    assert!(!is_test_assertion("__c_tg_assert_eq_bool"));
}

#[test]
fn a_residual_naming_a_comparator_is_found_and_a_value_is_not() {
    let residual = Term::app(Term::Global("compare_Alpha".to_string()), Term::Zero);
    assert_eq!(
        residual_comparison(&[residual]),
        Some("compare_Alpha".to_string())
    );
    // Values are skipped: the check costs nothing on the passing path and
    // cannot false-positive there.
    assert_eq!(residual_comparison(&[Term::Zero, Term::True]), None);
}

#[test]
fn the_cmp_intrinsic_counts_as_an_unresolved_comparison() {
    let residual = Term::ty_app(
        Term::Global(crate::eval::COMPARE_INTRINSIC.to_string()),
        crate::types::Type::Nat,
    );
    assert_eq!(
        residual_comparison(&[residual]),
        Some(crate::eval::COMPARE_INTRINSIC.to_string())
    );
}

#[test]
fn a_residual_naming_no_comparator_is_not_a_comparison_failure() {
    // Scope guard: an unrelated unresolved global stays an ordinary stuck
    // term. Without this, a detector that fired on every residual would
    // pass the tests above for the wrong reason.
    let residual = Term::app(Term::Global("some_other_global".to_string()), Term::Zero);
    assert_eq!(residual_comparison(&[residual]), None);
}

// ---------------------------------------------------------------------------
// The `tg_assert_eq_string` arm (ADR 6.8.26c P1)
// ---------------------------------------------------------------------------

/// Allocate a live C copy of `text` and return its address — the same real
/// allocation `console::cstring_from` makes during evaluation, not a handle
/// into a side table, which is the whole reason the arm can call the native FFI.
///
/// A plain function rather than an RAII guard on purpose. A `Drop` impl here
/// would be a mutable site nothing asserts on (mutation sweeps mutate `#[cfg
/// (test)]` helpers too, and a neutered `drop` survives silently); this shape
/// has no such site, and its own mutant — returning a constant address — is
/// *caught*, because address 0 makes the FFI null-check both operands into `""`
/// and `an_unequal_string_assertion_sets_the_failure_flag` then sees no failure.
fn alloc_cstring(text: &str) -> usize {
    let borrowed = TgString {
        ptr: text.as_ptr().cast::<std::ffi::c_char>(),
        len: text.len() as u64,
    };
    crate::ffi::tg_string_to_cstring(borrowed) as usize
}

/// Dispatch `tg_assert_eq_string` over two texts, returning the step result,
/// the post-call failure flag, and the env's assertion count.
///
/// Drives the real `step_extern_call_env` rather than the helper, so the
/// counter, the residual guard and the arm are exercised in the order the
/// evaluator runs them.
fn dispatch_string_assertion(left: &str, right: &str) -> (StepResult, u64, u64) {
    crate::ffi::tg_test_begin(std::ptr::null(), 0);
    let left_address = alloc_cstring(left);
    let right_address = alloc_cstring(right);
    let args = vec![
        nat_to_term(left_address),
        nat_to_term(left.len()),
        nat_to_term(right_address),
        nat_to_term(right.len()),
    ];
    let env = EvalEnv::new(HashMap::new());
    let result = step_extern_call_env("tg_assert_eq_string", &args, &env);
    let outcome = (
        result,
        crate::ffi::tg_test_check_failure(),
        env.assertions_executed(),
    );
    crate::ffi::tg_free_string(left_address as *mut std::ffi::c_char);
    crate::ffi::tg_free_string(right_address as *mut std::ffi::c_char);
    outcome
}

/// Equal strings: the assertion runs, sets no failure, and IS counted.
///
/// The count is the half that D2 insists on — an arm that compares without
/// being counted turns a loud `ASSERTED NOTHING` into a silent pass.
#[test]
fn an_equal_string_assertion_executes_is_counted_and_does_not_fail() {
    let (result, failed, assertions) = dispatch_string_assertion("hello world", "hello world");
    assert!(matches!(result, StepResult::Stepped(Term::Unit)));
    assert_eq!(failed, 0, "equal strings must not set the failure flag");
    assert_eq!(assertions, 1, "the executed assertion must be counted");
}

/// Unequal strings set the failure flag — the polarity that proves the arm
/// actually *compares*, rather than stepping to `Unit` unconditionally.
#[test]
fn an_unequal_string_assertion_sets_the_failure_flag() {
    let (result, failed, assertions) = dispatch_string_assertion("foobarbaz", "foobarbax");
    assert!(matches!(result, StepResult::Stepped(Term::Unit)));
    assert_eq!(failed, 1, "unequal strings must set the failure flag");
    assert_eq!(assertions, 1);
}

/// Length is honoured, not just the null terminator: a prefix must not compare
/// equal to the string it is a prefix of.
#[test]
fn a_prefix_does_not_compare_equal_to_the_longer_string() {
    let (_, failed, _) = dispatch_string_assertion("abc", "abcd");
    assert_eq!(failed, 1);
}

/// A non-`Nat` operand leaves the call `Stuck` rather than being coerced —
/// reading bytes from a misinterpreted address is the mistake worth refusing.
#[test]
fn a_non_nat_operand_leaves_the_string_assertion_stuck() {
    let env = EvalEnv::new(HashMap::new());
    let args = vec![
        Term::StringLit("not an address".to_string()),
        nat_to_term(0),
        nat_to_term(0),
        nat_to_term(0),
    ];
    assert!(matches!(
        step_extern_call_env("tg_assert_eq_string", &args, &env),
        StepResult::Stuck
    ));
}

// ---------------------------------------------------------------------------
// The `tg_string_char_at_internal` arm (ADR 6.8.26b D6, reversed)
// ---------------------------------------------------------------------------

/// Dispatch `tg_string_char_at_internal` over `(text, index)` and decode the
/// stepped result, or `None` if the call stayed `Stuck`.
///
/// Drives the real `step_extern_call_env` rather than `step_string_char_at`
/// directly, so the match arm's own pattern — which is what decides whether the
/// extern is claimed at all — is part of what these tests pin.
fn dispatch_char_at(text: &str, index: &Term) -> Option<usize> {
    let env = EvalEnv::new(HashMap::new());
    let args = vec![Term::StringLit(text.to_string()), index.clone()];
    match step_extern_call_env("tg_string_char_at_internal", &args, &env) {
        StepResult::Stepped(value) => Some(term_to_nat(&value).expect("a byte is a Nat")),
        _ => None,
    }
}

/// The byte actually at the index — not merely *a* Nat.
///
/// This is the half no other test covers: `every_registry_entry_is_claimed_by_
/// dispatch` asserts only that the call is not `Stuck`, so a constant-returning
/// arm would satisfy it while making `string_eq` compare every character equal.
#[test]
fn the_byte_at_each_index_is_read() {
    assert_eq!(
        dispatch_char_at("abc", &nat_to_term(0)),
        Some(b'a' as usize)
    );
    assert_eq!(
        dispatch_char_at("abc", &nat_to_term(1)),
        Some(b'b' as usize)
    );
    assert_eq!(
        dispatch_char_at("abc", &nat_to_term(2)),
        Some(b'c' as usize)
    );
}

/// Out of range yields 0 rather than stepping to a wrong byte or staying Stuck.
///
/// D6 chose 0 to match the native implementation; the empty string is the
/// boundary that an off-by-one `<=` would let past.
#[test]
fn an_index_past_the_end_yields_zero() {
    assert_eq!(dispatch_char_at("abc", &nat_to_term(3)), Some(0));
    assert_eq!(dispatch_char_at("abc", &nat_to_term(99)), Some(0));
    assert_eq!(dispatch_char_at("", &nat_to_term(0)), Some(0));
}

/// Read `index` of `text` through the native `tg_string_char_at_internal`.
fn native_char_at(text: &str, index: usize) -> u64 {
    let borrowed = TgString {
        ptr: text.as_ptr().cast::<std::ffi::c_char>(),
        len: text.len() as u64,
    };
    crate::ffi::tg_string_char_at_internal(borrowed, index as u64)
}

/// The claim D6 rests on, asserted rather than asserted-about: the evaluator's
/// arm and the native `tg_string_char_at_internal` agree at every index a
/// `string_eq` walk visits, including the two off the end.
///
/// Without this the two paths could drift silently — evaluated and compiled
/// runs of the same `.tg` file would disagree, which is exactly the divergence
/// `diff exec` exists to catch and the reason the arm was written to match.
///
/// Scoped to **ASCII**, which is not a convenience: the two paths provably do
/// NOT agree above 0x7F, and that divergence is pinned by
/// `the_two_paths_diverge_above_ascii_which_is_a_known_defect` below rather
/// than hidden by narrowing this one silently.
#[test]
fn the_evaluator_arm_agrees_with_the_native_ffi_over_ascii() {
    for text in ["", "a", "hello", "root::Alpha", "\u{0000}z"] {
        for index in 0..=text.len() + 1 {
            assert_eq!(
                dispatch_char_at(text, &nat_to_term(index)),
                Some(native_char_at(text, index) as usize),
                "evaluator and native disagree on byte {index} of {text:?}"
            );
        }
    }
}

/// **Known defect, pinned deliberately.** For any byte ≥ 0x80 the native FFI
/// returns a sign-extended value and the evaluator returns the byte.
///
/// `tg_string_char_at_internal` reads through a `*const c_char`, and `c_char`
/// is `i8` on this platform, so `*ptr as u64` sign-extends: byte `0xC3` comes
/// back as `18446744073709551555`, not `195`. The evaluator's arm reads
/// `as_bytes()`, so it returns `195`. D6's "matched to the native
/// implementation so both paths agree" therefore holds over ASCII only.
///
/// This test exists to make the disagreement fail LOUDLY the day either side
/// changes, rather than to bless it. Equality-only consumers (`string_eq`) are
/// unaffected because the native path is self-consistent, but anything that
/// orders or does arithmetic on the result is wrong under codegen, and
/// `tungsten diff exec` will report a divergence on any non-ASCII string.
/// Fixing it means changing COMPILED behaviour, which is why it is recorded
/// here and in a filed bug report rather than repaired
/// inside ADR 6.8.26b's close-out.
#[test]
fn the_two_paths_diverge_above_ascii_which_is_a_known_defect() {
    let text = "\u{00ff}"; // UTF-8: 0xC3 0xBF — both bytes are ≥ 0x80.
    assert_eq!(
        dispatch_char_at(text, &nat_to_term(0)),
        Some(0xC3),
        "the evaluator must return the byte itself"
    );
    assert_eq!(
        native_char_at(text, 0),
        0xC3_u8 as i8 as u64,
        "the native FFI sign-extends through c_char = i8"
    );
    assert_ne!(
        dispatch_char_at(text, &nat_to_term(0)).map(|b| b as u64),
        Some(native_char_at(text, 0)),
        "if these now agree the defect is FIXED — delete this test and widen \
         `the_evaluator_arm_agrees_with_the_native_ffi_over_ascii` to all bytes"
    );
}

/// A non-`Nat` index leaves the call `Stuck` rather than being coerced — the
/// same refusal `a_non_nat_operand_leaves_the_string_assertion_stuck` makes,
/// and for the same reason: indexing by a misinterpreted term is the mistake
/// worth declining.
#[test]
fn a_non_nat_index_leaves_the_read_stuck() {
    assert_eq!(
        dispatch_char_at("abc", &Term::StringLit("not an index".to_string())),
        None
    );
}

/// ADR 6.8.26c AC 2 / 1.8.26b precedence: a residual `compare` reaching the
/// string assertion must report `NeverCompared`, **not** a zero count.
///
/// Both mean "this test asserted nothing"; only the first says why, and the
/// order that produces it is the residual guard running before the counter.
/// Asserting it here rather than assuming it is the point — extending
/// `is_test_assertion` is what puts the string arm under that guard at all.
#[test]
fn a_residual_comparison_reaching_the_string_assertion_reports_never_compared() {
    let env = EvalEnv::new(HashMap::new());
    let residual = Term::app(Term::Global("compare_Alpha".to_string()), Term::Zero);
    let args = vec![residual, nat_to_term(0), nat_to_term(0), nat_to_term(0)];

    assert!(matches!(
        step_extern_call_env("tg_assert_eq_string", &args, &env),
        StepResult::Stuck
    ));
    assert!(
        matches!(
            env.recorded_stop(),
            Some(crate::eval::EvalStopped::ComparisonNeverRan { ref symbol })
                if symbol == "compare_Alpha"
        ),
        "the residual guard must claim this call before the arm runs"
    );
    assert_eq!(
        env.assertions_executed(),
        0,
        "a guarded assertion never executed, so it must not be counted"
    );
}

// ---------------------------------------------------------------------------
// The effect counter `EvalEnv::lookup` reads (ADR 14.9.26a)
// ---------------------------------------------------------------------------

/// `is_effectful` follows the registry's kind: everything but `Pure` counts,
/// and an unregistered name — about to be `Stuck` — does not.
#[test]
fn every_registered_kind_but_pure_is_effectful() {
    assert!(is_effectful("tg_string_builder_new"), "builder");
    assert!(is_effectful("tg_type_nat"), "arena");
    assert!(is_effectful("tg_println"), "console");
    assert!(is_effectful("tg_assert_eq_nat"), "test-assertion");
    assert!(
        is_effectful("__c_tg_string_builder_new"),
        "the C-ABI spelling too"
    );
    assert!(!is_effectful("tg_string_compare"), "pure");
    assert!(!is_effectful("tg_string_char_at_internal"), "pure");
    assert!(
        !is_effectful("tg_definitely_not_a_real_extern"),
        "unregistered"
    );
}

/// A claimed effectful call moves the env's counter; a pure one and a
/// `Stuck` one do not. Both polarities matter: a counter that moved on a pure
/// call would stop `lookup` memoizing every global that compares two strings.
#[test]
fn only_a_claimed_effectful_call_moves_the_effect_counter() {
    let env = EvalEnv::new(HashMap::new());

    let pure = vec![
        Term::StringLit("a".to_string()),
        Term::StringLit("a".to_string()),
    ];
    assert!(!matches!(
        step_extern_call_env("tg_string_compare", &pure, &env),
        StepResult::Stuck
    ));
    assert_eq!(env.effects_performed(), 0, "a pure call is not an effect");

    assert!(matches!(
        step_extern_call_env(
            "tg_string_builder_len",
            &[Term::StringLit("x".into())],
            &env
        ),
        StepResult::Stuck
    ));
    assert_eq!(env.effects_performed(), 0, "a Stuck call performed nothing");

    assert!(!matches!(
        step_extern_call_env("tg_string_builder_new", &[], &env),
        StepResult::Stuck
    ));
    assert_eq!(
        env.effects_performed(),
        1,
        "a claimed effectful call counts once"
    );
}
