//! Tests that keep [`EXECUTABLE_EXTERNS`] honest.
//!
//! The point of these is the *coupling*: they drive every registry entry
//! through the real dispatchers, so an entry describing an extern the evaluator
//! cannot actually execute fails the build rather than producing a diagnostic
//! that quietly lies.

use crate::eval::env::handlers::externs::console::step_console_extern;
use crate::eval::{nat_to_term, StepResult};
use crate::ffi::console_capture::test_exclusive::exclusive_sink;
use crate::ffi::types::constructors;
use crate::terms::Term;

use super::{entry_for, is_executable, ExternKind, EXECUTABLE_EXTERNS};

/// Plausible, well-typed arguments for `name`, so a `Stuck` result means "not
/// claimed" rather than "wrong arity".
fn sample_arguments(name: &str) -> Vec<Term> {
    match name {
        "tg_assert_eq_nat" | "tg_assert_eq_bool" => vec![nat_to_term(1), nat_to_term(1)],
        "tg_assert_eq_int" => vec![Term::IntLit(-1), Term::IntLit(-1)],
        // Two null (address, len) pairs: the FFI null-checks both, so this
        // compares "" against "" and cannot set the failure flag mid-test.
        "tg_assert_eq_string" => vec![
            nat_to_term(0),
            nat_to_term(0),
            nat_to_term(0),
            nat_to_term(0),
        ],
        "tg_test_check_failure" => vec![],
        "tg_string_compare" => vec![
            Term::StringLit("a".to_string()),
            Term::StringLit("a".to_string()),
        ],
        "tg_string_to_cstring" | "tg_string_len_internal" => {
            vec![Term::StringLit("x".to_string())]
        }
        "tg_string_char_at_internal" => vec![Term::StringLit("x".to_string()), nat_to_term(0)],
        // An address operand: 0 is safe because the console FFI null-checks.
        "tg_free_string" => vec![nat_to_term(0)],
        "tg_print" | "tg_println" | "tg_eprintln" => vec![nat_to_term(0), nat_to_term(0)],
        _ => arena_sample_arguments(name),
    }
}

/// Samples for the type-arena entries (ADR 7.8.26c §5).
///
/// Every accessor sample **builds a live node first**, and that is the whole
/// point of splitting this out. `dispatch_claims` asserts only "not `Stuck`",
/// so an accessor handed `nat_to_term(0)` against an arena that happens to be
/// empty returns `INVALID_HANDLE` and *still counts as claimed*: the coverage
/// test would prove the arm exists, not that it works. Allocating the node the
/// accessor is meant to decompose is what makes the claim non-vacuous.
fn arena_sample_arguments(name: &str) -> Vec<Term> {
    match name {
        "tg_type_nat" | "tg_type_bool" | "tg_type_unit" => vec![],
        "tg_type_arrow" | "tg_type_sum" => vec![live_nat_type(), live_bool_type()],
        "tg_type_mu" => vec![live_cstr_address("List"), live_nat_type()],
        "tg_type_tag" => vec![live_nat_type()],
        "tg_type_get_arrow_domain" | "tg_type_get_arrow_codomain" => vec![live_arrow_type()],
        "tg_type_get_sum_left" | "tg_type_get_sum_right" => vec![live_sum_type()],
        "tg_type_get_product_left" | "tg_type_get_product_right" => vec![live_product_type()],
        "tg_type_get_mu_body" | "tg_type_get_mu_var" => vec![live_mu_type()],
        "tg_type_get_forall_body" | "tg_type_get_forall_var" => vec![live_forall_type()],
        "tg_type_get_tyvar_name" => vec![live_tyvar_type()],
        "tg_string_to_cstr" => vec![Term::StringLit("List".to_string())],
        "tg_cstring_to_string" => vec![live_cstr_address("List")],
        _ => builder_sample_arguments(name),
    }
}

/// Samples for the `StringBuilder` entries (ADR 14.9.26a §2.2).
///
/// Every handle operand is a **live** builder from a real `new`: the runtime
/// aborts the process on a null or consumed handle, so `nat_to_term(0)` here
/// would not merely make the claim vacuous — it would kill the test binary.
fn builder_sample_arguments(name: &str) -> Vec<Term> {
    match name {
        "tg_string_builder_new" => vec![],
        "tg_string_builder_with_capacity" => vec![nat_to_term(16)],
        "tg_string_builder_push_str" => vec![live_builder(), Term::StringLit("x".to_string())],
        "tg_string_builder_push_char" => vec![live_builder(), nat_to_term(0x41)],
        "tg_string_builder_len" | "tg_string_builder_to_string" => vec![live_builder()],
        other => panic!("no sample arguments for registry entry {other} — add them here"),
    }
}

/// A handle to a freshly allocated, live `StringBuilder`.
fn live_builder() -> Term {
    nat_to_term(crate::ffi::tg_string_builder_new() as usize)
}

/// A handle to a freshly allocated `Nat` type node.
fn live_nat_type() -> Term {
    nat_to_term(constructors::tg_type_nat() as usize)
}

/// A handle to a freshly allocated `Bool` type node.
fn live_bool_type() -> Term {
    nat_to_term(constructors::tg_type_bool() as usize)
}

/// A handle to a live `Nat -> Bool`.
fn live_arrow_type() -> Term {
    nat_to_term(constructors::tg_type_arrow(
        constructors::tg_type_nat(),
        constructors::tg_type_bool(),
    ) as usize)
}

/// A handle to a live `Nat + Bool`.
fn live_sum_type() -> Term {
    nat_to_term(
        constructors::tg_type_sum(constructors::tg_type_nat(), constructors::tg_type_bool())
            as usize,
    )
}

/// A handle to a live `Nat × Bool`.
fn live_product_type() -> Term {
    nat_to_term(constructors::tg_type_product(
        constructors::tg_type_nat(),
        constructors::tg_type_bool(),
    ) as usize)
}

/// A handle to a live `μList. Nat`.
fn live_mu_type() -> Term {
    nat_to_term(crate::ffi::mu_type_from_cstr(
        crate::ffi::cstr_address_of("List"),
        constructors::tg_type_nat(),
    ) as usize)
}

/// A handle to a live `∀T. Bool`.
fn live_forall_type() -> Term {
    nat_to_term(crate::ffi::test_support::forall_type_from_cstr(
        crate::ffi::cstr_address_of("T"),
        constructors::tg_type_bool(),
    ) as usize)
}

/// A handle to a live type variable `T`.
fn live_tyvar_type() -> Term {
    nat_to_term(
        crate::ffi::test_support::tyvar_type_from_cstr(crate::ffi::cstr_address_of("T")) as usize,
    )
}

/// The address of a leaked C string holding `text`.
fn live_cstr_address(text: &str) -> Term {
    nat_to_term(crate::ffi::cstr_address_of(text))
}

/// Whether either dispatcher claims `name` with `values`.
///
/// The console dispatcher is checked directly; the general one is reached
/// through its own module. A claimed call steps, an unclaimed one is `Stuck`.
fn dispatch_claims(name: &str, values: &[Term]) -> bool {
    if step_console_extern(name, values).is_some() {
        return true;
    }
    // The assertion/pure arms live in extern_call, which evaluates its
    // arguments against an env first; values here are already values, so an
    // empty env suffices.
    let env = crate::eval::env::EvalEnv::new(std::collections::HashMap::new());
    let args: Vec<Term> = values.to_vec();
    !matches!(
        crate::eval::env::handlers::externs::call::step_extern_call_env(name, &args, &env),
        StepResult::Stuck
    )
}

/// The load-bearing test: every registry entry must actually be executable.
///
/// A console entry needs a capture sink installed, or `tg_println` would write
/// to the real stdout mid-test.
#[test]
fn every_registry_entry_is_claimed_by_dispatch() {
    let _guard = exclusive_sink();
    crate::ffi::console_capture::install().expect("no sink should be installed");

    for entry in EXECUTABLE_EXTERNS {
        let values = sample_arguments(entry.name);
        assert!(
            dispatch_claims(entry.name, &values),
            "registry lists `{}` but no dispatcher claims it — the diagnostics \
             would report an extern the evaluator leaves silently Stuck",
            entry.name
        );
    }

    let _ = crate::ffi::console_capture::take();
}

/// A name with no arm is reported as not executable — the case the whole
/// registry exists to make visible.
#[test]
fn an_unlisted_extern_is_not_executable() {
    assert!(!is_executable("tg_definitely_not_a_real_extern"));
    assert!(entry_for("tg_definitely_not_a_real_extern").is_none());
}

/// The elaborator prepends `__c_` to extern names, so lookups must accept both
/// spellings or the doctor check would report every elaborated call as missing.
#[test]
fn the_c_abi_prefix_is_stripped_on_lookup() {
    assert!(is_executable("tg_println"));
    assert!(is_executable("__c_tg_println"));
    assert_eq!(
        entry_for("__c_tg_println").map(|e| e.name),
        Some("tg_println"),
        "the stripped name is what should be reported back"
    );
}

/// Entries are unique — a duplicate would double-report in `info eval externs`
/// and silently shadow in `entry_for`.
#[test]
fn registry_entries_are_unique() {
    let mut names: Vec<&str> = EXECUTABLE_EXTERNS.iter().map(|e| e.name).collect();
    names.sort_unstable();
    let count = names.len();
    names.dedup();
    assert_eq!(count, names.len(), "duplicate entry in EXECUTABLE_EXTERNS");
}

/// Every entry carries a non-empty summary, since the summary IS the output of
/// `info eval externs`.
#[test]
fn every_entry_documents_itself() {
    for entry in EXECUTABLE_EXTERNS {
        assert!(
            !entry.summary.is_empty(),
            "{} has no summary; it would print as a blank row",
            entry.name
        );
        assert!(
            entry.name.starts_with("tg_"),
            "{} does not look like a tg_* extern",
            entry.name
        );
    }
}

/// The console chain that ADR 28.7.26a found broken is present in full.
///
/// `println` in `.tg` is three externs, not one; a registry missing any link
/// would let the doctor check pass on a program that still prints nothing.
#[test]
fn the_whole_console_chain_is_registered() {
    for link in [
        "tg_string_to_cstring",
        "tg_string_len_internal",
        "tg_println",
        "tg_free_string",
    ] {
        assert!(is_executable(link), "console chain is missing {link}");
    }
}

/// The whole `type_handle_to_codegen_type` path is registered (ADR 7.8.26c).
///
/// The names come from `tungsten info eval reachable-externs`, which walks the
/// static call graph — so the five accessors no test triggers are here too,
/// and `tg_cstring_to_string`, the extra hop inside `cstring_to_string` that
/// the ADR's own extern count missed. A registry short one link would let the
/// doctor check pass on six tests that still assert nothing.
#[test]
fn the_whole_type_arena_path_is_registered() {
    for link in [
        "tg_type_nat",
        "tg_type_bool",
        "tg_type_unit",
        "tg_type_arrow",
        "tg_type_sum",
        "tg_type_mu",
        "tg_type_tag",
        "tg_type_get_arrow_domain",
        "tg_type_get_arrow_codomain",
        "tg_type_get_sum_left",
        "tg_type_get_sum_right",
        "tg_type_get_product_left",
        "tg_type_get_product_right",
        "tg_type_get_mu_body",
        "tg_type_get_mu_var",
        "tg_type_get_forall_body",
        "tg_type_get_forall_var",
        "tg_type_get_tyvar_name",
        "tg_string_to_cstr",
        "tg_cstring_to_string",
    ] {
        assert!(is_executable(link), "the type-arena path is missing {link}");
    }
}

/// `tg_string_to_cstr` and `tg_string_to_cstring` are different symbols, one
/// letter apart, in different modules and different kinds (ADR 7.8.26c §5).
///
/// Pinned because the confusable pair is the mistake a reader ticking the
/// registry off against the ADR is most likely to make: seeing one and
/// counting the other as done.
#[test]
fn the_two_confusable_string_externs_are_both_present_and_distinct() {
    let cstr = entry_for("tg_string_to_cstr").expect("tg_string_to_cstr is registered");
    let cstring = entry_for("tg_string_to_cstring").expect("tg_string_to_cstring is registered");

    assert_ne!(cstr.name, cstring.name);
    assert_eq!(cstr.kind, ExternKind::Arena);
    assert_eq!(cstring.kind, ExternKind::Console);
}

/// An arena extern must NOT be counted as a test assertion.
///
/// The census the `ASSERTED NOTHING` detector reads counts executed
/// assertions; a `tg_type_tag` that incremented it would turn "this test
/// asserted nothing" into a plausible non-zero number, which is the exact
/// failure ADR 6.8.26b built the counter to surface.
#[test]
fn an_arena_extern_does_not_count_as_an_assertion() {
    let env = crate::eval::env::EvalEnv::new(std::collections::HashMap::new());
    let handle = live_nat_type();
    let stepped = crate::eval::env::handlers::externs::call::step_extern_call_env(
        "tg_type_tag",
        std::slice::from_ref(&handle),
        &env,
    );

    assert_ne!(stepped, StepResult::Stuck, "tg_type_tag must be claimed");
    assert_eq!(
        env.assertions_executed(),
        0,
        "an arena read is not an assertion"
    );
}

/// The whole `StringBuilder` surface is registered (ADR 14.9.26a AC 1), and
/// under its own kind — a `Pure` label would tell `doctor check
/// extern-coverage` the evaluator runs six more pure functions than it does.
#[test]
fn the_whole_string_builder_path_is_registered_as_builder() {
    for link in [
        "tg_string_builder_new",
        "tg_string_builder_with_capacity",
        "tg_string_builder_push_str",
        "tg_string_builder_push_char",
        "tg_string_builder_len",
        "tg_string_builder_to_string",
    ] {
        let entry = entry_for(link).unwrap_or_else(|| panic!("the builder path is missing {link}"));
        assert_eq!(
            entry.kind,
            ExternKind::Builder,
            "{link} is not kind Builder"
        );
    }
}

/// Kind labels are distinct and non-empty — they are a report column.
#[test]
fn kind_labels_are_distinct() {
    let labels = [
        ExternKind::TestAssertion.label(),
        ExternKind::Console.label(),
        ExternKind::Pure.label(),
        ExternKind::Arena.label(),
        ExternKind::Builder.label(),
    ];
    assert!(labels.iter().all(|l| !l.is_empty()));
    let mut sorted = labels;
    sorted.sort_unstable();
    let unique = {
        let mut v = sorted.to_vec();
        v.dedup();
        v.len()
    };
    assert_eq!(unique, labels.len(), "kind labels must be distinguishable");
}

/// `dispatch_claims` must actually discriminate.
///
/// It is a non-`#[test]` helper, so the mutation engine mutates it too — and an
/// always-`true` version would make
/// `every_registry_entry_is_claimed_by_dispatch` pass vacuously over a registry
/// full of externs no dispatcher handles. Pinning both polarities is what makes
/// that test mean something.
#[test]
fn dispatch_claims_discriminates() {
    assert!(
        dispatch_claims("tg_println", &[nat_to_term(0), nat_to_term(0)]),
        "a registered extern must be claimed"
    );
    assert!(
        !dispatch_claims("tg_definitely_not_a_real_extern", &[]),
        "an unregistered extern must NOT be claimed — otherwise the coverage \
         test above proves nothing"
    );
}
