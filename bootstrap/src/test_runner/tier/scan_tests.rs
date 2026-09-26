//! Tests for the two text predicates.
//!
//! Every case is a literal string, so these assert the rules themselves rather
//! than the checkout they happen to run against.

use super::*;

// ---------------------------------------------------------------------------
// Guard (a)'s scan
// ---------------------------------------------------------------------------

#[test]
fn a_runtime_assertion_call_is_detected_whatever_the_wrapper() {
    assert!(calls_a_runtime_assertion("    assert_eq_string(x, \"a\")"));
    assert!(calls_a_runtime_assertion("assert_eq_nat(n, 3)"));
    assert!(calls_a_runtime_assertion("let _ = assert_ne(a, b);"));
    assert!(calls_a_runtime_assertion("assert(cond)"));
    assert!(calls_a_runtime_assertion("assert_some(opt)"));
}

/// The elaboration-time assertions a tier-3 file is *made of* must not trip
/// guard (a) — otherwise no file could be declared tier 3 at all and the guard
/// would have eaten the feature it protects.
#[test]
fn elaboration_time_assertions_do_not_count_as_runtime_ones() {
    let compare_result = "\
pub fn test_compare_result_equal_typechecks() -> Unit {
    let r: CompareResult = Equal;
    expect_type(r, \"CompareResult\")
}
";
    assert!(!calls_a_runtime_assertion(compare_result));
    assert!(!calls_a_runtime_assertion("expect_error(e, \"E0001\")"));

    let list_ops = "\
pub fn test_list_len_three() -> Unit {
    let xs: List<Nat> = Cons(1, Nil);
    let n: Nat = list_len(xs);
    let _ = n;
}
";
    assert!(!calls_a_runtime_assertion(list_ops));
}

/// Importing an assertion without calling it must not force tier 5: the scan
/// looks for an *application*, so a `use` line alone is not a call.
#[test]
fn importing_an_assertion_without_calling_it_is_not_a_call() {
    assert!(!calls_a_runtime_assertion(
        "use driver::ffi::test::{assert_eq_string, assert_eq_nat};"
    ));
}

/// An identifier that merely *ends* in `assert` is not one, or a helper named
/// `reassert_all(..)` would silently force its file to tier 5.
#[test]
fn a_name_that_only_ends_in_assert_is_not_an_assertion_call() {
    assert!(!calls_a_runtime_assertion("reassert_all(x)"));
    assert!(!calls_a_runtime_assertion("my_assert(x)"));
}

/// A bare mention with no application is not a call — the scan must look at
/// what follows the identifier, not just find the substring.
#[test]
fn a_mention_without_an_application_is_not_a_call() {
    assert!(!calls_a_runtime_assertion(
        "// this file deliberately has no assert of any kind"
    ));
    assert!(!calls_a_runtime_assertion("let assert_count = 0;"));
}

// ---------------------------------------------------------------------------
// The `must_declare` glob
// ---------------------------------------------------------------------------

#[test]
fn the_glob_matches_within_a_segment_and_not_across_one() {
    assert!(glob_matches(
        "src/compiler/test_*.tg",
        "src/compiler/test_strmap.tg"
    ));
    assert!(!glob_matches(
        "src/compiler/test_*.tg",
        "src/compiler/driver/test_x.tg"
    ));
    assert!(!glob_matches("src/compiler/test_*.tg", "tests/test_x.tg"));
    assert!(!glob_matches(
        "src/compiler/test_*.tg",
        "src/compiler/mustfail_ast_compare.tg"
    ));
}

/// The prefix and suffix both have to hold — a matcher checking only one would
/// pass the test above for the wrong reason.
#[test]
fn the_glob_requires_both_the_prefix_and_the_suffix() {
    assert!(!glob_matches("src/test_*.tg", "src/other_x.tg"));
    assert!(!glob_matches("src/test_*.tg", "src/test_x.rs"));
    // A segment shorter than prefix+suffix must not match by overlapping them.
    assert!(!glob_matches("src/ab*ba.tg", "src/aba.tg"));
}

#[test]
fn a_pattern_with_no_star_is_an_exact_match() {
    assert!(glob_matches("tests/try_block.tg", "tests/try_block.tg"));
    assert!(!glob_matches("tests/try_block.tg", "tests/type_alias.tg"));
}
