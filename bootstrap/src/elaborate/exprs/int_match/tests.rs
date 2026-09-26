//! Integer `match` (ADR 18.9.26e): the chain's shape, first-match-wins and
//! guard scoping on the core evaluator, and every diagnostic the module
//! raises — including the two (E0021 for a mixed or-pattern, E0090 for an
//! out-of-range literal) that have no golden, because the `.tg` suite runs
//! under `tungsten1` too, whose codes differ.

use tungsten_core::terms::IntBinOp;
use tungsten_core::{eval, Term};

use crate::elaborate::error::ElabError;
use crate::elaborate::tests::{elab_err, elab_ok, elab_ok_with_warnings};

/// The value of the named definition applied to `argument`, as a `u64`.
fn run(source: &str, name: &str, argument: Term) -> u64 {
    let defs = elab_ok(source);
    let def = defs
        .iter()
        .find(|d| d.name == name)
        .unwrap_or_else(|| panic!("no definition `{name}`"));
    nat_value(&eval(&Term::app(def.term.term.clone(), argument)))
}

fn nat_value(term: &Term) -> u64 {
    match term {
        Term::Zero => 0,
        Term::Succ(inner) => 1 + nat_value(inner),
        Term::NatLit(n) => *n,
        other => panic!("not a Nat value: {other}"),
    }
}

fn codes(errors: &[ElabError]) -> Vec<&'static str> {
    errors.iter().map(|e| e.kind.code()).collect()
}

const CLASSIFY: &str = "
fn classify(n: Int) -> Nat {
    match n {
        -1 => 10,
        0 | 7 => 20,
        x if x > 0 => 30,
        _ => 40,
    }
}
";

/// 18.9.26e AC 1: first match wins across a negative literal, an
/// or-of-literals, a guarded variable arm and the catch-all.
#[test]
fn int_match_first_match_wins() {
    assert_eq!(run(CLASSIFY, "classify", Term::int_lit(-1)), 10);
    assert_eq!(run(CLASSIFY, "classify", Term::int_lit(0)), 20);
    assert_eq!(run(CLASSIFY, "classify", Term::int_lit(7)), 20);
    assert_eq!(run(CLASSIFY, "classify", Term::int_lit(1)), 30);
    assert_eq!(run(CLASSIFY, "classify", Term::int_lit(-5)), 40);
}

/// A guard that fails falls through to the next arm, not to the catch-all.
#[test]
fn int_match_failed_guard_falls_through_to_the_next_arm() {
    let source = "
fn pick(n: Int) -> Nat {
    match n {
        x if x > 100 => 1,
        5 => 2,
        _ => 3,
    }
}
";
    assert_eq!(run(source, "pick", Term::int_lit(5)), 2);
    assert_eq!(run(source, "pick", Term::int_lit(500)), 1);
    assert_eq!(run(source, "pick", Term::int_lit(6)), 3);
}

/// A variable arm's binding scopes only its own guard and body: the later
/// arm's `x` is the parameter, not the scrutinee the guarded arm bound.
#[test]
fn int_match_variable_arm_does_not_shadow_later_arms() {
    let source = "
fn keep(x: Nat) -> Nat {
    match 3 {
        x if x == 0 => 1,
        _ => x,
    }
}
";
    assert_eq!(run(source, "keep", Term::nat_lit(9)), 9);
}

/// A `Nat` scrutinee with literal and or-of-literal arms and a binding catch-all.
#[test]
fn nat_match_selects_by_literal() {
    let source = "
fn small(n: Nat) -> Nat {
    match n {
        0 => 5,
        1 | 2 => 6,
        m => m + 100,
    }
}
";
    assert_eq!(run(source, "small", Term::nat_lit(0)), 5);
    assert_eq!(run(source, "small", Term::nat_lit(2)), 6);
    assert_eq!(run(source, "small", Term::nat_lit(3)), 103);
}

/// The chain's shape: a let-bound scrutinee, then a conditional over the
/// scrutinee's own equality primitive — `IntBin(Eq)` for `Int`, `NatEq`
/// for `Nat` — and no other node.
#[test]
fn int_match_lowers_to_a_let_bound_conditional_chain() {
    let int_term = int_match_body("fn f(n: Int) -> Nat { match n { 0 => 1, _ => 2 } }");
    let Term::Let(_, _, _, chain) = &int_term else {
        panic!("expected a let-bound scrutinee, got {int_term}");
    };
    let Term::If(condition, _, _) = chain.as_ref() else {
        panic!("expected a conditional, got {chain}");
    };
    assert!(matches!(
        condition.as_ref(),
        Term::IntBin(IntBinOp::Eq, _, _)
    ));

    let nat_term = int_match_body("fn f(n: Nat) -> Nat { match n { 0 => 1, _ => 2 } }");
    let Term::Let(_, _, _, chain) = &nat_term else {
        panic!("expected a let-bound scrutinee, got {nat_term}");
    };
    let Term::If(condition, _, _) = chain.as_ref() else {
        panic!("expected a conditional, got {chain}");
    };
    assert!(matches!(condition.as_ref(), Term::NatEq(_, _)));
}

/// The body of the single definition `f`, under its lambda.
fn int_match_body(source: &str) -> Term {
    let defs = elab_ok(source);
    let term = defs[0].term.term.strip_spans();
    let Term::Lambda(_, _, body) = term else {
        panic!("expected a lambda, got {term}");
    };
    *body
}

/// 18.9.26e AC 2: `Int`'s `MIN` is a legal pattern; one past `MAX` is E0090.
#[test]
fn int_match_literal_range() {
    elab_ok("fn f(n: Int) -> Nat { match n { -9223372036854775808 => 1, _ => 2 } }");
    let errors = elab_err("fn f(n: Int) -> Nat { match n { 9223372036854775808 => 1, _ => 2 } }");
    assert_eq!(codes(&errors), ["E0090"]);
}

/// 18.9.26e AC 2: an or-pattern holding a variable or a wildcard is a
/// catch-all in disguise.
#[test]
fn int_match_mixed_or_pattern_is_e0021() {
    let errors = elab_err("fn f(n: Int) -> Nat { match n { 1 | x => 1, _ => 2 } }");
    assert_eq!(codes(&errors), ["E0021"]);
    let errors = elab_err("fn f(n: Nat) -> Nat { match n { _ | 3 => 1, _ => 2 } }");
    assert_eq!(codes(&errors), ["E0021"]);
}

#[test]
fn int_match_negative_literal_on_nat_is_e0021() {
    let errors = elab_err("fn f(n: Nat) -> Nat { match n { -1 => 1, _ => 2 } }");
    assert_eq!(codes(&errors), ["E0021"]);
}

#[test]
fn int_match_non_literal_patterns_are_e0021() {
    let errors = elab_err("fn f(n: Nat) -> Nat { match n { true => 1, _ => 2 } }");
    assert_eq!(codes(&errors), ["E0021"]);
    let errors = elab_err("fn f(n: Int) -> Nat { match n { (a, b) => 1, _ => 2 } }");
    assert_eq!(codes(&errors), ["E0021"]);
}

/// Exhaustiveness is the one-unguarded-catch-all rule; a guarded catch-all
/// does not count.
#[test]
fn int_match_without_an_unguarded_catch_all_is_e0020() {
    let errors = elab_err("fn f(n: Int) -> Nat { match n { 0 => 1, 1 => 2 } }");
    assert_eq!(codes(&errors), ["E0020"]);
    let errors = elab_err("fn f(n: Int) -> Nat { match n { 0 => 1, x if x > 0 => 2 } }");
    assert_eq!(codes(&errors), ["E0020"]);
}

#[test]
fn int_match_non_boolean_guard_is_e0010() {
    let errors = elab_err("fn f(n: Int) -> Nat { match n { x if 1 => 1, _ => 2 } }");
    assert_eq!(codes(&errors), ["E0010"]);
}

/// A duplicate literal and an arm after the catch-all warn, and the file
/// still compiles.
#[test]
fn int_match_unreachable_arms_warn_w0001() {
    let (_, warnings) =
        elab_ok_with_warnings("fn f(n: Int) -> Nat { match n { 3 => 1, 1 | 3 => 2, _ => 3 } }");
    assert_eq!(codes(&warnings), ["W0001"]);
    let (_, warnings) = elab_ok_with_warnings("fn f(n: Nat) -> Nat { match n { _ => 1, 4 => 2 } }");
    assert_eq!(codes(&warnings), ["W0001"]);
}

/// With no expected type, the first body fixes the result type, and a later
/// arm of another type is E0010 against it.
#[test]
fn int_match_synthesised_result_type_comes_from_the_first_body() {
    let source = "
fn f(n: Int) -> Nat {
    let chosen = match n {
        0 => 7,
        _ => 8,
    };
    chosen
}
";
    assert_eq!(run(source, "f", Term::int_lit(0)), 7);
    assert_eq!(run(source, "f", Term::int_lit(3)), 8);
    let errors =
        elab_err("fn f(n: Int) -> Nat { let chosen = match n { 0 => 7, _ => true }; chosen }");
    assert_eq!(codes(&errors), ["E0010"]);
}

/// A `Bool` literal inside an integer or-pattern is E0021.
#[test]
fn int_match_non_integer_alternative_in_an_or_pattern_is_e0021() {
    let errors = elab_err("fn f(n: Nat) -> Nat { match n { 1 | true => 1, _ => 2 } }");
    assert_eq!(codes(&errors), ["E0021"]);
}

/// A literal repeated after a *guarded* arm is not a duplicate: the guard
/// may fail.
#[test]
fn int_match_literal_after_a_guarded_arm_is_not_a_duplicate() {
    let (_, warnings) = elab_ok_with_warnings(
        "fn f(n: Int) -> Nat { match n { 3 if false => 1, 3 => 2, _ => 3 } }",
    );
    assert!(warnings.is_empty(), "{warnings:?}");
}
