//! Elaboration of signed `Int` arithmetic (ADR 14.9.26c §2.2): the
//! operand-type dispatch in `operators/mod.rs` and the `to_int`/`from_int`
//! bridges. The ADR's `.tg` suites cover the same ground under `tungsten
//! test`, which `cargo test` never runs — so the mutation sweep saw every
//! branch here as untested. Each test names the mutant it kills.

use crate::elaborate::tests::{elab_err, elab_ok};
use tungsten_core::Type;

fn only_message(source: &str) -> String {
    let errors = elab_err(source);
    assert_eq!(errors.len(), 1, "{errors:?}");
    errors[0].message.clone()
}

/// `1 + x` for `x : Int`, INFERRED (a `let` with no annotation — a body
/// checked against `-> Int` takes `check_numeric`'s arm instead): the bare
/// literal on the left defers to the non-literal right, so the `Int` path
/// is taken (`bare_literal -> None`, `left && !right` with the `!` deleted).
#[test]
fn a_bare_literal_on_the_left_defers_to_an_int_operand() {
    let defs = elab_ok("fn f(x: Int) -> Int { let z = 1 + x; z }");
    assert_eq!(defs[0].ty, Type::arrow(Type::Int, Type::Int));
    let defs = elab_ok("fn f(x: Int) -> Int { let z = (1) + x; z }");
    assert_eq!(defs[0].ty, Type::arrow(Type::Int, Type::Int));
    let defs = elab_ok("fn f(x: Int) -> Bool { 1 < x }");
    assert_eq!(defs[0].ty, Type::arrow(Type::Int, Type::Bool));
}

/// Two `Int` operands: arithmetic keeps the type, ordering yields `Bool`.
#[test]
fn int_operands_dispatch_to_the_int_path() {
    let defs = elab_ok("fn f(x: Int, y: Int) -> Int { x * y }");
    assert_eq!(
        defs[0].ty,
        Type::arrow(Type::Int, Type::arrow(Type::Int, Type::Int))
    );
    let defs = elab_ok("fn f(x: Int, y: Int) -> Bool { x < y }");
    assert_eq!(
        defs[0].ty,
        Type::arrow(Type::Int, Type::arrow(Type::Int, Type::Bool))
    );
}

/// The LEFT operand decides when neither is a literal: with `x : Int` on
/// the left, `y : Nat` is checked against `Int` and the error says so. The
/// `&&`→`||` mutant makes the right operand the decider and reports
/// "expected `Nat`, found `Int`" on `x` instead. Inferred contexts, as above.
#[test]
fn the_left_operand_decides_the_operand_type() {
    assert_eq!(
        only_message("fn f(x: Int, y: Nat) -> Bool { x < y }"),
        "expected `Int`, found `Nat`"
    );
    assert_eq!(
        only_message("fn f(x: Int, y: Nat) -> Int { let z = x + y; z }"),
        "expected `Int`, found `Nat`"
    );
}

/// `check_numeric`'s arithmetic arm against `Int`: both literals are checked
/// as `Int`, where inference alone would have made them `Nat` (the
/// `is_arithmetic -> false` and Int-guard `false` mutants).
#[test]
fn an_arithmetic_expression_checks_its_literals_against_the_expected_int() {
    let defs = elab_ok("fn f() -> Int { 1 + 2 }");
    assert_eq!(defs[0].ty, Type::Int);
    assert!(
        defs[0].term.term.to_string().contains("int:1"),
        "{}",
        defs[0].term.term
    );
}

/// The arithmetic arm is for `+ - * / %` ONLY: an ordering yields `Bool`, so
/// against `Int` or `Nat` it is a mismatch, not a numeric term with the
/// wrong type (the `is_arithmetic -> true` and guard-`true` mutants).
#[test]
fn an_ordering_is_not_checked_as_arithmetic() {
    assert_eq!(
        only_message("fn f() -> Int { 1 < 2 }"),
        "expected `Int`, found `Bool`"
    );
    assert_eq!(
        only_message("fn f() -> Nat { 1 < 2 }"),
        "expected `Nat`, found `Bool`"
    );
}

/// The `Nat` arithmetic arm checks each operand, so a mismatch lands on the
/// operand rather than on the whole expression (the Nat-guard `false`
/// mutant infers the binary and reports its full span).
#[test]
fn a_nat_arithmetic_mismatch_is_reported_on_the_operand() {
    let source = "fn f(x: Int, y: Int) -> Nat { x + y }";
    let errors = elab_err(source);
    assert_eq!(errors.len(), 1, "{errors:?}");
    let x_at = source.find("x + y").expect("operand") as u32;
    assert_eq!((errors[0].span.start, errors[0].span.end), (x_at, x_at + 1));
    assert_eq!(errors[0].message, "expected `Nat`, found `Int`");
}

/// `-5` against `Int` folds to the literal `-5` — not `-0`, `-1` or a
/// negation of `5` (the `bare_literal -> Some(0)/Some(1)/None` mutants).
#[test]
fn a_negated_literal_folds_with_its_sign() {
    let defs = elab_ok("fn f() -> Int { -5 }");
    assert_eq!(defs[0].term.term.to_string(), "int:-5");
    let defs = elab_ok("fn f() -> Int { -(5) }");
    assert_eq!(defs[0].term.term.to_string(), "int:-5");
    let defs = elab_ok("fn f(x: Int) -> Int { -x }");
    assert!(
        defs[0].term.term.to_string().contains("(intneg x)"),
        "{}",
        defs[0].term.term
    );
}

/// The bridges take exactly one argument (the `!=`→`==` arity mutants
/// refuse the one-argument call and accept the two-argument one).
#[test]
fn the_bridges_take_exactly_one_argument() {
    let defs = elab_ok("fn f(n: Nat) -> Int { to_int(n) }");
    assert_eq!(defs[0].ty, Type::arrow(Type::Nat, Type::Int));
    let defs = elab_ok("fn f(i: Int) -> Nat { from_int(i) }");
    assert_eq!(defs[0].ty, Type::arrow(Type::Int, Type::Nat));

    let errors = elab_err("fn f(n: Nat) -> Int { to_int(n, n) }");
    assert!(
        errors[0].message.contains("expected 1 arguments, found 2"),
        "{errors:?}"
    );
    let errors = elab_err("fn f(i: Int) -> Nat { from_int(i, i) }");
    assert!(
        errors[0].message.contains("expected 1 arguments, found 2"),
        "{errors:?}"
    );
}

/// `x |> f` is `f(x)`, and the argument must match the parameter (the
/// deleted `!` accepts the mismatch and refuses the match).
#[test]
fn pipe_applies_when_the_argument_matches_and_refuses_when_it_does_not() {
    let defs = elab_ok("fn f(x: Nat, g: Nat -> Int) -> Int { x |> g }");
    assert_eq!(
        defs[0].ty,
        Type::arrow(
            Type::Nat,
            Type::arrow(Type::arrow(Type::Nat, Type::Int), Type::Int)
        )
    );
    assert_eq!(
        only_message("fn f(s: String, g: Nat -> Int) -> Int { s |> g }"),
        "expected `Nat`, found `String`"
    );
}
