//! Execute test-assertion FFI calls during evaluation (ADR 29.6.26f / T13).
//!
//! `tungsten run`/`test` evaluate test bodies, and the test-assertion FFIs live
//! in this same crate, so the evaluator can invoke them directly. The
//! console-output externs join them via [`super::console`] (ADR
//! 28.7.26a §2.2); every other `ExternCall` stays stuck (pure evaluation).

use crate::eval::env::{eval_with_env, EvalEnv};
use crate::eval::StepResult;
use crate::eval::{nat_to_term, term_to_nat, EvalStopped};
use crate::terms::Term;

/// Execute a known test-assertion FFI, or stay stuck for any other extern call.
/// Arguments are evaluated to values first.
pub(in crate::eval::env) fn step_extern_call_env(
    name: &str,
    args: &[Term],
    env: &EvalEnv,
) -> StepResult {
    // Strip the `__c_` C-ABI prefix the elaborator prepends to extern names.
    let fname = name.strip_prefix("__c_").unwrap_or(name);
    // A black hole below an argument leaves the call stuck; the recorded
    // cycle rides the env and is reported at the entry point (ADR 22.7.26a).
    let mut values: Vec<Term> = Vec::with_capacity(args.len());
    for arg in args {
        match eval_with_env(arg, env) {
            Ok(value) => values.push(value),
            Err(_) => return StepResult::Stuck,
        }
    }

    let result = dispatch_extern(fname, values.as_slice(), env);
    // A claimed call to anything but a `Pure` extern is an effect, and the
    // global being forced above it must not be memoized (ADR 14.9.26a; the
    // check is in `EvalEnv::lookup`). Decided from the registry's kind rather
    // than per arm, so a new dispatch module cannot forget to say so.
    if !matches!(result, StepResult::Stuck) && is_effectful(fname) {
        env.record_effect_performed();
    }
    result
}

/// Whether the registry files `fname` under any kind but `Pure`.
///
/// An unregistered name is not effectful — it is not executable at all, and
/// its call is about to be `Stuck`.
fn is_effectful(fname: &str) -> bool {
    super::registry::entry_for(fname)
        .is_some_and(|entry| entry.kind != super::registry::ExternKind::Pure)
}

/// Try each dispatch module in turn; `Stuck` if none claims the call.
fn dispatch_extern(fname: &str, values: &[Term], env: &EvalEnv) -> StepResult {
    if let Some(result) = super::console::step_console_extern(fname, values) {
        return result;
    }

    // Above the assertion-counting block below, deliberately: an arena extern
    // is not a test assertion, and counting one would inflate the census the
    // `ASSERTED NOTHING` detector reads (ADR 7.8.26c §3 task 7).
    if let Some(result) = super::arena::step_arena_extern(fname, values) {
        return result;
    }

    // Same reasoning: a builder push is not an assertion (ADR 14.9.26a).
    if let Some(result) = super::builder::step_builder_extern(fname, values) {
        return result;
    }

    // The invariant, asserted at the boundary rather than by enumerating the
    // ways synthesis can fail: no residual `compare` may reach an assertion.
    if is_test_assertion(fname) {
        if let Some(symbol) = residual_comparison(values) {
            env.record_stop(EvalStopped::ComparisonNeverRan { symbol });
            return StepResult::Stuck;
        }
        // Past the residual guard, this assertion IS about to execute, so
        // count it (ADR 6.8.26b). Order matters and is asserted by a test: a
        // residual comparison must report as `NeverCompared`, not as a zero
        // count. Both mean "asserted nothing"; only the first says why.
        env.record_assertion_executed();
    }

    match (fname, values) {
        ("tg_assert_eq_bool", [l, r]) => call_assert_nat(l, r, crate::ffi::tg_assert_eq_bool),
        ("tg_assert_eq_nat", [l, r]) => call_assert_nat(l, r, crate::ffi::tg_assert_eq_nat),
        ("tg_assert_eq_int", [Term::IntLit(l), Term::IntLit(r)]) => {
            crate::ffi::tg_assert_eq_int(*l, *r);
            StepResult::Stepped(Term::Unit)
        }
        // String equality, in the (address, len) pair form the `.tg` wrapper
        // actually hands over (ADR 6.8.26c D1).
        ("tg_assert_eq_string", [laddr, llen, raddr, rlen]) => {
            call_assert_eq_string(laddr, llen, raddr, rlen)
        }
        ("tg_test_check_failure", []) => {
            let failed = crate::ffi::tg_test_check_failure();
            StepResult::Stepped(nat_to_term(failed as usize))
        }
        // Three-way string comparison for the StrMap AVL index (ADR 21.7.26c).
        // Pure, so safe to execute during evaluation; shares its byte-wise
        // comparison with the native tg_string_compare FFI.
        ("tg_string_compare", [Term::StringLit(a), Term::StringLit(b)]) => StepResult::Stepped(
            nat_to_term(crate::ffi::string_compare_bytes(a.as_bytes(), b.as_bytes()) as usize),
        ),
        // Byte read at an index — pure, and the base case `string_eq` bottoms
        // out in (ADR 6.8.26b D6, reversed on evidence). Leaving it stuck made
        // `string_eq` stuck, which made every assertion depending on a string
        // comparison never run: 113 of the corpus's tests were reporting `ok`
        // while asserting nothing. Out-of-range yields 0, matching the native
        // `tg_string_char_at_internal`, so the two paths agree.
        ("tg_string_char_at_internal", [Term::StringLit(s), index]) => {
            step_string_char_at(s, index)
        }
        _ => StepResult::Stuck,
    }
}

/// Byte at `index` in `s`, or 0 out of range — matching the native
/// `tg_string_char_at_internal` so the evaluator and codegen paths agree.
///
/// Extracted rather than inlined in the dispatch arm to keep
/// `step_extern_call_env` within the match-nesting budget.
fn step_string_char_at(s: &str, index: &Term) -> StepResult {
    let Some(i) = term_to_nat(index) else {
        return StepResult::Stuck;
    };
    StepResult::Stepped(nat_to_term(
        s.as_bytes().get(i).copied().unwrap_or(0) as usize
    ))
}

/// Whether `fname` is one of the FFIs a `.tg` assertion bottoms out in.
///
/// `assert_eq`/`assert_ne`/`assert`/`fail` all reduce to `assert_eq_bool` or
/// `assert_eq_nat` (`src/compiler/driver/ffi/compare/mod.tg`), so gating those
/// two covers the structural-comparison surface without the gate having to know
/// how many wrappers sit above it. `assert_eq_string` is a separate bottom —
/// its `.tg` wrapper calls `tg_assert_eq_string` directly rather than reducing
/// to either — so it is named here too, and it must be: an arm that executes
/// the comparison without being counted turns a loud `ASSERTED NOTHING` into a
/// silent pass the detector cannot vouch for (ADR 6.8.26c D2).
fn is_test_assertion(fname: &str) -> bool {
    matches!(
        fname,
        "tg_assert_eq_bool" | "tg_assert_eq_nat" | "tg_assert_eq_int" | "tg_assert_eq_string"
    )
}

/// The first unresolved comparator symbol inside an assertion's arguments, if
/// any (ADR 1.8.26b).
///
/// Only non-value arguments are inspected, so a healthy assertion — whose
/// operands have reduced to `Nat`/`Bool` values — can never match: the check
/// costs nothing on the passing path and cannot false-positive on it. A
/// `compare_*` global or a bare `__cmp` still standing in a *residual* operand
/// means the comparison did not run, and the assertion is about to assert
/// nothing.
fn residual_comparison(values: &[Term]) -> Option<String> {
    values
        .iter()
        .filter(|v| !v.is_value())
        .find_map(unresolved_comparator)
}

/// Depth-first search for a `Global("compare_*")` or the `__cmp` intrinsic.
fn unresolved_comparator(term: &Term) -> Option<String> {
    if let Term::Global(name) = term {
        if name.starts_with("compare_") || name == crate::eval::COMPARE_INTRINSIC {
            return Some(name.clone());
        }
    }
    let mut found = None;
    term.for_each_subterm(|child| {
        if found.is_none() {
            found = unresolved_comparator(child);
        }
    });
    found
}

/// Marshal two `Nat` operands to `u64`, invoke `f`, and step to `Unit`.
fn call_assert_nat(l: &Term, r: &Term, f: extern "C" fn(u64, u64)) -> StepResult {
    match (term_to_nat(l), term_to_nat(r)) {
        (Some(l), Some(r)) => {
            f(l as u64, r as u64);
            StepResult::Stepped(Term::Unit)
        }
        _ => StepResult::Stuck,
    }
}

/// Marshal two `(address, length)` C-string pairs and compare them natively.
///
/// The operands really are heap addresses by the time they arrive: the `.tg`
/// wrapper (`src/compiler/driver/ffi/test/mod.tg:35-43`) puts both `String`s
/// through `tg_string_to_cstring`, which the evaluator executes for real
/// (`super::console::cstring_from`) precisely so evaluated and compiled output
/// stay byte-identical for `diff exec`. So this hands the same pointers to the
/// same `tungsten_core::ffi` entry point compiled code calls — the shape
/// `write_console` already uses — rather than inventing an evaluator-local
/// string comparison that could disagree with the native one.
///
/// A non-`Nat` operand stays `Stuck` rather than being coerced: reading four
/// bytes from a misinterpreted address is exactly the mistake worth refusing.
fn call_assert_eq_string(
    left: &Term,
    left_len: &Term,
    right: &Term,
    right_len: &Term,
) -> StepResult {
    let (Some(left), Some(left_len), Some(right), Some(right_len)) = (
        term_to_nat(left),
        term_to_nat(left_len),
        term_to_nat(right),
        term_to_nat(right_len),
    ) else {
        return StepResult::Stuck;
    };
    crate::ffi::tg_assert_eq_string(
        left as *const std::ffi::c_char,
        left_len as u64,
        right as *const std::ffi::c_char,
        right_len as u64,
    );
    StepResult::Stepped(Term::Unit)
}

// Tests: call_tests.rs — kept beside this module alongside its
// `console_tests.rs` / `registry_tests.rs` siblings, so the string arm's
// coverage can grow without crowding the dispatch table.
#[cfg(test)]
#[path = "call_tests.rs"]
mod tests;
