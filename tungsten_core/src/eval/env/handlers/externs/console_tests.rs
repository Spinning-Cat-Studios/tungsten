//! Tests for the evaluator's console-output externs (ADR 28.7.26a §2.2).
//!
//! These drive the full `.tg` chain the wrapper emits —
//! `tg_string_to_cstring` → `tg_println` → `tg_free_string` — because the
//! chain is the thing that was silently broken: each link is individually
//! plausible, and only running them in sequence shows bytes arriving.
//!
//! The capture sink is process-global, so tests that install one serialize on
//! the ONE lock declared beside it —
//! [`console_capture::test_exclusive::exclusive_sink`] — shared with
//! `ffi/driver/console_capture_tests.rs`, which drives the same sink from this
//! same test binary. See that helper's docs for the race a per-file lock caused.

use crate::eval::{nat_to_term, term_to_nat, StepResult};
use crate::ffi::console_capture;
use crate::ffi::console_capture::test_exclusive::exclusive_sink;
use crate::terms::Term;

use super::step_console_extern;

/// Run one console extern, asserting it was claimed and stepped.
fn step(name: &str, values: &[Term]) -> Term {
    match step_console_extern(name, values) {
        Some(StepResult::Stepped(term)) => term,
        other => panic!("{name} should step, got {other:?}"),
    }
}

/// Drive the full `.tg` `println` chain for `text`, the way an evaluated
/// program reaches the console.
fn println_chain(text: &str, print_extern: &str) {
    let address = step("tg_string_to_cstring", &[Term::StringLit(text.to_string())]);
    let len = step(
        "tg_string_len_internal",
        &[Term::StringLit(text.to_string())],
    );
    assert_eq!(
        step(print_extern, &[address.clone(), len]),
        Term::Unit,
        "{print_extern} should step to Unit"
    );
    assert_eq!(step("tg_free_string", &[address]), Term::Unit);
}

/// AC 5(a): evaluating a `println` chain puts the printed bytes in the
/// captured stdout buffer, and leaves stderr empty.
#[test]
fn evaluated_println_lands_in_captured_stdout() {
    let _guard = exclusive_sink();

    console_capture::install().expect("no sink should be installed");
    println_chain("from the evaluator", "tg_println");
    let captured = console_capture::take().expect("a sink was installed");

    assert_eq!(captured.stdout, b"from the evaluator\n");
    assert!(
        captured.stderr.is_empty(),
        "stderr should be untouched, got {:?}",
        captured.stderr
    );
}

/// AC 5(b): an evaluated `eprintln` chain lands in stderr, not stdout.
#[test]
fn evaluated_eprintln_lands_in_captured_stderr() {
    let _guard = exclusive_sink();

    console_capture::install().expect("no sink should be installed");
    println_chain("evaluator warning", "tg_eprintln");
    let captured = console_capture::take().expect("a sink was installed");

    assert_eq!(captured.stderr, b"evaluator warning\n");
    assert!(captured.stdout.is_empty());
}

/// `tg_print` writes without a newline, unlike `tg_println`.
#[test]
fn evaluated_print_omits_the_newline() {
    let _guard = exclusive_sink();

    console_capture::install().expect("no sink should be installed");
    println_chain("no newline", "tg_print");
    let captured = console_capture::take().expect("a sink was installed");

    assert_eq!(captured.stdout, b"no newline");
}

/// The regression this module exists for: before it, every console extern was
/// unclaimed and went silently `Stuck`, so an evaluated program printed
/// nothing at all. Each name must now be recognized.
#[test]
fn every_console_extern_is_claimed() {
    for name in [
        "tg_string_to_cstring",
        "tg_string_len_internal",
        "tg_free_string",
        "tg_print",
        "tg_println",
        "tg_eprintln",
    ] {
        let values = [Term::StringLit("x".to_string()), nat_to_term(1)];
        let claimed = step_console_extern(name, &values[..1]).is_some()
            || step_console_extern(name, &values).is_some();
        assert!(claimed, "{name} should be claimed by the console dispatch");
    }
}

/// An extern this module does not own returns `None` so the caller's other
/// dispatch arms still get a chance — a `Stuck` here would swallow them.
#[test]
fn unrelated_externs_are_not_claimed() {
    assert!(step_console_extern("tg_assert_eq_nat", &[nat_to_term(1), nat_to_term(1)]).is_none());
    assert!(step_console_extern("tg_string_compare", &[]).is_none());
}

/// `tg_string_len_internal` reports the byte length, matching the native FFI
/// (which reads `TgString::len`) rather than a character count.
#[test]
fn string_len_reports_bytes_not_characters() {
    // "é" is two UTF-8 bytes; a char count would say 1.
    let len = step(
        "tg_string_len_internal",
        &[Term::StringLit("é".to_string())],
    );
    assert_eq!(term_to_nat(&len), Some(2));
}

/// `tg_string_to_cstring` yields a usable, non-zero address — a zero would be
/// the allocation-failure sentinel, and freeing it must still step.
#[test]
fn cstring_address_is_non_null_and_freeable() {
    let address = step(
        "tg_string_to_cstring",
        &[Term::StringLit("abc".to_string())],
    );
    assert_ne!(term_to_nat(&address), Some(0), "address should be non-null");
    assert_eq!(step("tg_free_string", &[address]), Term::Unit);
}

/// A non-`Nat` address stays `Stuck` rather than being coerced — the evaluator
/// must not guess at an address it would then free or dereference.
#[test]
fn non_nat_operands_stay_stuck() {
    assert!(matches!(
        step_console_extern("tg_free_string", &[Term::Unit]),
        Some(StepResult::Stuck)
    ));
    assert!(matches!(
        step_console_extern("tg_println", &[Term::Unit, nat_to_term(1)]),
        Some(StepResult::Stuck)
    ));
    assert!(matches!(
        step_console_extern("tg_print", &[nat_to_term(1), Term::Unit]),
        Some(StepResult::Stuck)
    ));
}

/// A non-literal operand to the string externs is not claimed, so a future
/// arm could handle it rather than this one silently answering wrongly.
#[test]
fn non_literal_string_operands_are_not_claimed() {
    assert!(step_console_extern("tg_string_to_cstring", &[Term::Unit]).is_none());
    assert!(step_console_extern("tg_string_len_internal", &[Term::Unit]).is_none());
}

/// Successive prints accumulate in order within one capture — the property a
/// multi-line program's output depends on.
#[test]
fn successive_prints_accumulate_in_order() {
    let _guard = exclusive_sink();

    console_capture::install().expect("no sink should be installed");
    println_chain("one", "tg_println");
    println_chain("two", "tg_println");
    let captured = console_capture::take().expect("a sink was installed");

    assert_eq!(captured.stdout, b"one\ntwo\n");
}

/// An empty string still prints its newline — the byte a `println("")` owes.
#[test]
fn empty_string_still_prints_its_newline() {
    let _guard = exclusive_sink();

    console_capture::install().expect("no sink should be installed");
    println_chain("", "tg_println");
    let captured = console_capture::take().expect("a sink was installed");

    assert_eq!(captured.stdout, b"\n");
}
