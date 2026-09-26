//! The construction boundaries consume stub poison (ADR 15.8.26d).
//!
//! Every fixture seeds ONE type-body fault (`NoSuchType`) and then builds,
//! takes apart or projects from a value of the failed type inside the same
//! file — the in-module shape, where the type is a poisoned `Stub` with no
//! field list. The invariant under test: the seeded `E0002` is the only
//! diagnostic. What each transit arm must NOT swallow — an argument's own
//! fault, and a genuinely wrong site against a HEALTHY type (D3) — is the
//! sibling `construction_site_must_fail.rs`, which shares these helpers.

use tungsten_core::Context;

/// Every error the run reports, in order.
pub(super) fn errors(source: &str) -> Vec<crate::elaborate::ElabError> {
    let (ast, parse_errors) = crate::parse(source);
    assert!(
        parse_errors.is_empty(),
        "fixture must parse: {parse_errors:?}"
    );
    let mut ctx = Context::new();
    crate::elaborate::elaborate(&ast, &mut ctx)
        .err()
        .unwrap_or_default()
}

/// Every error code the run reports, in order.
pub(super) fn error_codes(source: &str) -> Vec<String> {
    errors(source)
        .iter()
        .map(|e| e.kind.code().to_string())
        .collect()
}

/// The seeded fault, and nothing else.
fn assert_only_the_seeded_fault(source: &str) {
    let codes = error_codes(source);
    assert!(
        !codes.is_empty() && codes.iter().all(|c| c == "E0002"),
        "the seeded E0002 must be the only diagnostic; got {codes:?}"
    );
}

/// The helper's own contract: a second diagnostic beside the seeded one must
/// fail it — a helper that passes everything proves nothing.
#[test]
#[should_panic(expected = "the seeded E0002 must be the only diagnostic")]
fn the_seeded_fault_helper_refuses_a_second_diagnostic() {
    assert_only_the_seeded_fault(&format!(
        "{POISONED_ADT}fn tick() -> Event {{ Tick(undefined_var) }}"
    ));
}

pub(super) const POISONED_RECORD: &str = "type Box = { width: Nat, height: NoSuchType }\n";
pub(super) const POISONED_ADT: &str = "type Event = | Tick(NoSuchType) | Tock(Nat)\n";

// ── D1: record literals ─────────────────────────────────────────────────

/// AC 1: the named-record construction site that used to add `E0050`.
#[test]
fn a_named_record_literal_against_a_poisoned_record_reports_nothing() {
    assert_only_the_seeded_fault(&format!(
        "{POISONED_RECORD}fn make() -> Box {{ Box {{ width: 1, height: 2 }} }}"
    ));
}

#[test]
fn an_anonymous_record_literal_against_a_poisoned_record_reports_nothing() {
    assert_only_the_seeded_fault(&format!(
        "{POISONED_RECORD}fn make() -> Box {{ {{ width: 1, height: 2 }} }}"
    ));
}

#[test]
fn a_spread_record_literal_against_a_poisoned_record_reports_nothing() {
    assert_only_the_seeded_fault(&format!(
        "{POISONED_RECORD}fn grow(b: Box) -> Box {{ Box {{ ...b, width: 3 }} }}"
    ));
}

/// Projection is the same boundary read the other way: a poisoned base has
/// no field to name, and the fault is already reported.
#[test]
fn a_field_access_on_a_poisoned_record_reports_nothing() {
    assert_only_the_seeded_fault(&format!(
        "{POISONED_RECORD}fn wide(b: Box) -> Nat {{ b.width + b.height }}"
    ));
}

// ── D2: constructor calls ───────────────────────────────────────────────

/// AC 2: the argumentful constructor call that used to add `E0010` — with
/// arguments, because a nullary probe never enters the field check.
#[test]
fn a_constructor_call_on_a_poisoned_adt_reports_nothing() {
    assert_only_the_seeded_fault(&format!(
        "{POISONED_ADT}fn tick() -> Event {{ Tick(1) }}\nfn tock() -> Event {{ Tock(2) }}"
    ));
}

/// Both bidirectional entry points: checked against the return type, and
/// inferred inside a `let`.
#[test]
fn an_inferred_constructor_call_on_a_poisoned_adt_reports_nothing() {
    assert_only_the_seeded_fault(&format!(
        "{POISONED_ADT}fn tick() -> Nat {{ let e = Tick(1); 0 }}"
    ));
}

#[test]
fn a_nullary_constructor_of_a_poisoned_adt_reports_nothing() {
    assert_only_the_seeded_fault(
        "type Flag = | Raised(NoSuchType) | Lowered\n\
         fn low() -> Flag { Lowered }\n\
         fn low2() -> Nat { let f = Lowered; 0 }",
    );
}

/// A HEALTHY constructor checked against a poisoned field: the injection
/// has no sum to build into (`build_constructor_injection`'s poison arm).
/// `Som` is index 1, the index that has to peel a `Sum` — index 0 never
/// enters that loop and would false-pass.
#[test]
fn a_healthy_constructor_checked_against_a_poisoned_field_reports_nothing() {
    assert_only_the_seeded_fault(&format!(
        "{POISONED_ADT}type Opt = | Non | Som(Nat)\nfn tick() -> Event {{ Tick(Som(1)) }}"
    ));
}

/// A healthy GENERIC constructor checked against a poisoned field: poison
/// carries no type arguments, so the check falls back to inference rather
/// than unifying `T` against nothing and mismatching its own argument —
/// the one residue V5 kept after D2 (`MkPath(Cons(id, Nil()), span)`).
#[test]
fn a_generic_constructor_checked_against_a_poisoned_field_reports_nothing() {
    assert_only_the_seeded_fault(&format!(
        "{POISONED_ADT}type Lst<T> = | LNil | LCons(T, Lst<T>)\n\
         fn tick() -> Event {{ Tick(LCons(1, LNil)) }}"
    ));
}

/// A lambda checked against a poisoned field reads it as `<error> ->
/// <error>`: the parameter binds and the body still elaborates.
#[test]
fn a_lambda_against_a_poisoned_field_reports_nothing() {
    assert_only_the_seeded_fault(&format!(
        "{POISONED_ADT}fn tick() -> Event {{ Tick(fn(x) => x) }}"
    ));
}

// ── Matches and patterns over a poisoned type ───────────────────────────

#[test]
fn a_match_on_a_poisoned_adt_reports_nothing() {
    assert_only_the_seeded_fault(&format!(
        "{POISONED_ADT}fn is_tick(e: Event) -> Bool {{ match e {{ Tick(_) => true, Tock(_) => false }} }}"
    ));
}

/// Pattern variables bind at `Type::Error`, so a body that USES them stays
/// quiet too — and without an expected type the match takes its first arm's
/// type, as the healthy path does.
#[test]
fn a_match_on_a_poisoned_adt_binds_its_variables() {
    assert_only_the_seeded_fault(&format!(
        "{POISONED_ADT}fn val(e: Event) -> Nat {{ let n = match e {{ Tick(t) => t, Tock(m) => m }}; n + 1 }}"
    ));
}

/// A nested pattern reaching INTO a poisoned payload from a healthy outer
/// constructor: the inner match transits instead of raising `E0021`.
#[test]
fn a_nested_pattern_into_a_poisoned_payload_reports_nothing() {
    assert_only_the_seeded_fault(&format!(
        "{POISONED_ADT}type Maybe = | Just(Event) | Nothing\n\
         fn m(o: Maybe) -> Nat {{ match o {{ Just(Tick(n)) => n, Just(Tock(q)) => q, Nothing => 0 }} }}"
    ));
}

/// The scrutinee-poison guard in `elab_adt_match`: the inner patterns name a
/// HEALTHY ADT, but the payload they match over is poison.
#[test]
fn a_nested_match_over_a_poisoned_payload_with_healthy_patterns_reports_nothing() {
    assert_only_the_seeded_fault(&format!(
        "{POISONED_ADT}type Maybe = | Just(Event) | Nothing\ntype Other = | A(Nat) | B(Nat)\n\
         fn m(o: Maybe) -> Nat {{ match o {{ Just(A(n)) => n, Just(B(q)) => q, Nothing => 0 }} }}"
    ));
}

/// The same nested pattern with ONE arm per outer constructor takes the
/// single-field route (`elab_nested_ctor_pattern`), not the nested-match
/// builder: `resolve_pattern_ctor`'s poison verdict binds `n` at poison and
/// the arm is a hole.
#[test]
fn a_single_arm_nested_pattern_into_a_poisoned_payload_reports_nothing() {
    assert_only_the_seeded_fault(&format!(
        "{POISONED_ADT}type Maybe = | Just(Event) | Nothing\n\
         fn m(o: Maybe) -> Nat {{ match o {{ Just(Tick(n)) => n, Nothing => 0 }} }}"
    ));
}

/// A poisoned payload inside a MULTI-field constructor takes the product
/// route (`collect_pattern_bindings` then `wrap_single_subpattern`): the
/// binding walk binds at poison and the wrap leaves a hole. One arm per
/// outer constructor — two `Both` arms with nested patterns are E0021 on a
/// healthy type as well.
#[test]
fn a_nested_pattern_in_a_multi_field_constructor_reports_nothing() {
    assert_only_the_seeded_fault(&format!(
        "{POISONED_ADT}type Pair = | Both(Event, Nat) | Neither\n\
         fn m(p: Pair) -> Nat {{ match p {{ Both(Tick(n), k) => k, Neither => 0 }} }}"
    ));
}

/// A scrutinee whose TYPE is poison — the value of a poisoned construction
/// — transits in `elab_match` before any dispatch on its shape.
#[test]
fn a_match_on_a_poisoned_value_reports_nothing() {
    assert_only_the_seeded_fault(&format!(
        "{POISONED_ADT}fn f() -> Nat {{ match Tick(1) {{ Tick(n) => n, Tock(m) => m }} }}"
    ));
}

/// A guard on a poisoned match still elaborates against `Bool`.
#[test]
fn a_guard_on_a_poisoned_match_is_still_checked() {
    assert_only_the_seeded_fault(&format!(
        "{POISONED_ADT}fn f(e: Event) -> Nat {{ match e {{ Tick(n) if true => n, Tick(n) => n, Tock(m) => m }} }}"
    ));
    let codes = error_codes(&format!(
        "{POISONED_ADT}fn f(e: Event) -> Nat {{ match e {{ Tick(n) if 1 => n, Tick(n) => n, Tock(m) => m }} }}"
    ));
    assert!(
        codes.contains(&"E0010".to_string()),
        "a non-Bool guard must still be diagnosed: {codes:?}"
    );
}

/// A nullary healthy constructor checked against a poisoned field reaches
/// `build_constructor_injection` with a poisoned ADT type — the argumentful
/// form falls back to inference before it gets there.
#[test]
fn a_nullary_healthy_constructor_checked_against_a_poisoned_field_reports_nothing() {
    assert_only_the_seeded_fault(&format!(
        "{POISONED_ADT}type Opt = | Non | Som(Nat)\nfn tick() -> Event {{ Tick(Non) }}"
    ));
}

/// An anonymous record literal checked against a poisoned field:
/// `resolve_record_type` meets `Type::Error` itself, not a poisoned name.
#[test]
fn an_anonymous_record_literal_against_a_poisoned_field_reports_nothing() {
    assert_only_the_seeded_fault(&format!(
        "{POISONED_ADT}fn tick() -> Event {{ Tick({{ width: 1 }}) }}"
    ));
}

/// The type-body producer fires for a failed ALIAS too, and registers no
/// constructors for it — an alias has none.
#[test]
fn a_failed_alias_is_poisoned_and_registers_no_constructors() {
    use crate::elaborate::Elaborator;
    let (ast, _) = crate::parse("type Alias = NoSuchType\nfn f() -> Alias { 0 }");
    let mut ctx = Context::new();
    let mut elab = Elaborator::new(&mut ctx);
    assert!(elab.run_collection_pass(&ast).is_ok());
    assert!(elab.env.types["Alias"].is_poison());
    assert!(elab.env.constructors.is_empty());
}
