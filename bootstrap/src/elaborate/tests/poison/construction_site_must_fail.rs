//! The must-fail half of ADR 15.8.26d's construction-boundary suite: what
//! the transit arms must NOT swallow. The Non-Goal — a built value's own
//! faults still surface — and D3 — a genuinely wrong site against a HEALTHY
//! type, or with a wrong arity, still diagnoses. The transit fixtures
//! themselves are in `construction_site_poison.rs`, which owns the helpers.

use super::construction_site_poison::{error_codes, errors, POISONED_ADT, POISONED_RECORD};
use tungsten_core::Context;

// ── Non-Goal: the built value's own faults still surface ────────────────

#[test]
fn an_argument_fault_inside_a_poisoned_construction_still_surfaces() {
    for source in [
        format!("{POISONED_ADT}fn tick() -> Event {{ Tick(undefined_var) }}"),
        format!("{POISONED_RECORD}fn make() -> Box {{ Box {{ width: undefined_var, height: 2 }} }}"),
        format!("{POISONED_RECORD}fn make() -> Box {{ {{ width: undefined_var, height: 2 }} }}"),
        format!("{POISONED_ADT}fn f(e: Event) -> Nat {{ match e {{ Tick(_) => undefined_var, Tock(_) => 0 }} }}"),
    ] {
        let codes = error_codes(&source);
        assert!(
            codes.contains(&"E0001".to_string()),
            "the argument's own E0001 must survive the transit: {codes:?}\n{source}"
        );
    }
}

/// The arms of a poisoned match still unify with the expected type, and
/// with each other when there is none — only the scrutinee is a hole.
#[test]
fn a_poisoned_match_still_checks_its_arms() {
    let against_expected = format!(
        "{POISONED_ADT}fn g(e: Event) -> Nat {{ match e {{ Tick(n) => n, Tock(_) => \"s\" }} }}"
    );
    assert!(error_codes(&against_expected).contains(&"E0010".to_string()));

    let against_first_arm = format!(
        "{POISONED_ADT}fn g(e: Event) -> Nat {{ let x = match e {{ Tick(_) => 1, Tock(_) => 2 }}; x + true }}"
    );
    assert!(error_codes(&against_first_arm).contains(&"E0010".to_string()));
}

/// WHICH expectation a later arm's mismatch is blamed on follows the healthy
/// path's convention: with no expected type, the first arm's body; with one,
/// the expectation itself — never the first arm.
#[test]
fn a_poisoned_matchs_later_arm_is_blamed_on_the_first_arm_only_without_an_expected_type() {
    use crate::elaborate::error::ExpectedReason;
    let mismatch = |source: &str| {
        let errors = errors(source);
        errors
            .into_iter()
            .find(|e| e.kind.code() == "E0010")
            .expect("the arm mismatch must be reported")
    };

    let inferred = mismatch(&format!(
        "{POISONED_ADT}fn g(e: Event) -> Nat {{ let x = match e {{ Tick(_) => 1, Tock(_) => \"s\" }}; 0 }}"
    ));
    let context = inferred
        .context
        .expect("an inferred match blames its second arm on the first");
    assert!(
        matches!(context.reason, ExpectedReason::BranchUnification),
        "got {:?}",
        context.reason
    );

    let checked = mismatch(&format!(
        "{POISONED_ADT}fn g(e: Event) -> Nat {{ match e {{ Tick(_) => 1, Tock(_) => \"s\" }} }}"
    ));
    assert!(
        !matches!(
            checked.context.as_ref().map(|c| &c.reason),
            Some(ExpectedReason::BranchUnification)
        ),
        "a checked match blames the expectation, not the first arm: {:?}",
        checked.context
    );
}

// ── D3: a genuinely wrong site against a HEALTHY type still fails ────────

#[test]
fn a_record_literal_with_a_wrong_field_against_a_healthy_type_still_fails() {
    let codes = error_codes("type Good = { w: Nat }\nfn make() -> Good { Good { w: 1, zzz: 2 } }");
    assert_eq!(
        codes,
        vec!["E0052"],
        "the extra field must still be diagnosed"
    );
    let codes = error_codes("type Good = { w: Nat }\nfn make() -> Good { { w: true } }");
    assert_eq!(
        codes,
        vec!["E0010"],
        "the mistyped field must still be diagnosed"
    );
}

/// Arity is validated BEFORE the parent is consulted, so it holds on the
/// poisoned type too.
#[test]
fn a_constructor_call_with_wrong_arity_still_fails() {
    let codes = error_codes("type Ev = | Tick(Nat) | Tock(Nat)\nfn t() -> Ev { Tick(1, 2) }");
    assert_eq!(codes, vec!["E0012"]);
    let codes = error_codes(&format!("{POISONED_ADT}fn t() -> Event {{ Tick(1, 2) }}"));
    assert!(
        codes.contains(&"E0012".to_string()),
        "arity is checked before the poison verdict: {codes:?}"
    );
}

/// The lambda arm's refusal is intact: against a healthy NON-function type
/// it still says "expected function type".
#[test]
fn a_lambda_against_a_non_function_type_still_fails() {
    let codes = error_codes("fn f() -> Nat { fn(x) => x }");
    assert_eq!(codes.len(), 1, "exactly one refusal: {codes:?}");
    assert_ne!(codes[0], "E0002");
}

#[test]
fn a_wrong_argument_against_a_healthy_constructor_still_fails() {
    let codes = error_codes("type Ev = | Tick(Nat) | Tock(Nat)\nfn t() -> Ev { Tick(true) }");
    assert_eq!(codes, vec!["E0010"]);
}

/// The transit is keyed on the PRODUCER's residue, not on the stub kind: an
/// unpoisoned stub is a compiler invariant broken, and stays loud.
#[test]
fn an_unpoisoned_stub_is_not_transit() {
    use crate::elaborate::env::{TypeDef, TypeDefKind};
    use crate::elaborate::Elaborator;
    let (ast, _) = crate::parse("fn make() -> Nat { let b = Pending { w: 1 }; 0 }");
    let mut ctx = Context::new();
    let mut elab = Elaborator::new(&mut ctx);
    elab.env
        .define_type(TypeDef::test_stub("Pending", TypeDefKind::Stub));
    let errors = elab
        .elaborate_file(&ast)
        .err()
        .expect("a bare stub must still refuse");
    assert!(
        errors.iter().any(|e| e.kind.code() == "E0050"),
        "the stub kind's E0050 must survive when nothing poisoned it: {errors:?}"
    );
}
