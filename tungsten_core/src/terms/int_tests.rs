//! The `Int` term shapes (ADR 14.9.26c) through every walker that gained an
//! arm: kernel typing, the legacy stepper, substitution, free-variable and
//! use-count analysis, display, shape typing, the arena-size walker, and the
//! FFI constructors. One file, so the five variants are exercised in one
//! place rather than an arm at a time in eight.

use crate::context::Context;
use crate::eval::eval;
use crate::ffi::terms::primitives::{
    tg_term_int_bin, tg_term_int_lit, tg_term_int_neg, tg_term_int_to_nat, tg_term_nat_to_int,
};
use crate::ffi::types::constructors::tg_type_int;
use crate::ffi::types::predicates::tg_type_tag;
use crate::ffi::INVALID_HANDLE;
use crate::terms::{IntBinOp, Term};
use crate::typecheck::{type_of, TypeError};
use crate::types::Type;

const ALL_OPS: [IntBinOp; 10] = [
    IntBinOp::Add,
    IntBinOp::Sub,
    IntBinOp::Mul,
    IntBinOp::Div,
    IntBinOp::Mod,
    IntBinOp::Eq,
    IntBinOp::Lt,
    IntBinOp::Le,
    IntBinOp::Gt,
    IntBinOp::Ge,
];

fn int(v: i64) -> Term {
    Term::int_lit(v)
}

// ── the operator enum ───────────────────────────────────────────────────────

#[test]
fn op_codes_round_trip_and_symbols_are_distinct() {
    let mut symbols = Vec::new();
    for op in ALL_OPS {
        assert_eq!(IntBinOp::from_code(op.code()), Some(op));
        symbols.push(op.symbol());
    }
    symbols.sort_unstable();
    symbols.dedup();
    assert_eq!(symbols.len(), 10, "every operator has its own spelling");
    assert_eq!(IntBinOp::from_code(10), None);
    assert_eq!(IntBinOp::from_code(u64::MAX), None);
    assert_eq!(IntBinOp::Div.symbol(), "/");
    assert_eq!(IntBinOp::Le.symbol(), "<=");
    assert!(IntBinOp::Eq.is_comparison());
    assert!(!IntBinOp::Mod.is_comparison());
}

// ── kernel typing (14.9.26c AC 1, the Core side) ────────────────────────────

#[test]
fn int_bin_types_arithmetic_as_int_and_comparison_as_bool() {
    let ctx = Context::new();
    for op in ALL_OPS {
        let expected = if op.is_comparison() {
            Type::Bool
        } else {
            Type::Int
        };
        assert_eq!(
            type_of(&ctx, &Term::int_bin(op, int(1), int(2))),
            Ok(expected),
            "{op:?}"
        );
    }
    assert_eq!(type_of(&ctx, &Term::int_neg(int(1))), Ok(Type::Int));
    assert_eq!(type_of(&ctx, &Term::nat_to_int(Term::Zero)), Ok(Type::Int));
    assert_eq!(type_of(&ctx, &Term::int_to_nat(int(1))), Ok(Type::Nat));
    assert_eq!(type_of(&ctx, &int(-1)), Ok(Type::Int));
}

#[test]
fn int_operands_must_be_int_and_bridges_check_their_side() {
    let ctx = Context::new();
    let mismatch = |expected: Type, got: Type| Err(TypeError::TypeMismatch { expected, got });
    assert_eq!(
        type_of(&ctx, &Term::int_bin(IntBinOp::Add, Term::Zero, int(1))),
        mismatch(Type::Int, Type::Nat)
    );
    assert_eq!(
        type_of(&ctx, &Term::int_bin(IntBinOp::Lt, int(1), Term::True)),
        mismatch(Type::Int, Type::Bool)
    );
    assert_eq!(
        type_of(&ctx, &Term::int_neg(Term::Zero)),
        mismatch(Type::Int, Type::Nat)
    );
    assert_eq!(
        type_of(&ctx, &Term::nat_to_int(int(1))),
        mismatch(Type::Nat, Type::Int)
    );
    assert_eq!(
        type_of(&ctx, &Term::int_to_nat(Term::Zero)),
        mismatch(Type::Int, Type::Nat)
    );
}

// ── the legacy (env-free) stepper ───────────────────────────────────────────

#[test]
fn legacy_stepper_evaluates_every_int_shape_including_nested_operands() {
    let nested = Term::int_bin(
        IntBinOp::Mul,
        Term::int_bin(IntBinOp::Sub, int(3), int(5)),
        Term::int_neg(Term::int_bin(IntBinOp::Add, int(1), int(1))),
    );
    assert_eq!(eval(&nested), int(4));
    assert_eq!(
        eval(&Term::int_bin(IntBinOp::Div, int(-7), int(2))),
        int(-3)
    );
    assert_eq!(
        eval(&Term::int_bin(IntBinOp::Mod, int(-7), int(2))),
        int(-1)
    );
    assert_eq!(
        eval(&Term::int_bin(IntBinOp::Ge, int(2), int(2))),
        Term::True
    );
    assert_eq!(
        eval(&Term::nat_to_int(Term::nat_add(Term::nat(2), Term::nat(3)))),
        int(5)
    );
    assert_eq!(
        crate::eval::term_to_nat(&eval(&Term::int_to_nat(Term::int_neg(int(9))))),
        Some(0)
    );
    assert_eq!(
        crate::eval::term_to_nat(&eval(&Term::int_to_nat(int(9)))),
        Some(9)
    );
}

#[test]
fn legacy_stepper_is_stuck_on_a_trap_rather_than_wrapping() {
    let overflow = Term::int_bin(IntBinOp::Add, int(i64::MAX), int(1));
    assert_eq!(eval(&overflow), overflow, "a trap has no env to record on");
    let neg_min = Term::int_neg(int(i64::MIN));
    assert_eq!(eval(&neg_min), neg_min);
    let too_big = Term::nat_to_int(Term::NatLit(u64::MAX));
    assert_eq!(eval(&too_big), too_big);
    // A non-literal operand that cannot step is stuck too, not a panic.
    let open = Term::int_bin(IntBinOp::Add, Term::var("x"), int(1));
    assert_eq!(eval(&open), open);
}

// ── substitution, analysis, display ─────────────────────────────────────────

fn every_shape_over(x: Term) -> Vec<Term> {
    vec![
        Term::int_bin(IntBinOp::Add, x.clone(), int(1)),
        Term::int_neg(x.clone()),
        Term::nat_to_int(x.clone()),
        Term::int_to_nat(x),
    ]
}

/// The helper's contract: one term per operand-carrying `Int` shape, so a
/// loop over it that asserts nothing is a failing test, not a vacuous one.
#[test]
fn every_shape_over_yields_the_four_operand_carrying_shapes() {
    let names: Vec<&str> = every_shape_over(Term::var("x"))
        .iter()
        .map(crate::diagnostics::term_shape::variant_name)
        .collect();
    assert_eq!(names, ["IntBin", "IntNeg", "NatToInt", "IntToNat"]);
}

#[test]
fn term_substitution_reaches_every_int_operand() {
    for term in every_shape_over(Term::var("x")) {
        let after = term.substitute("x", &int(7));
        assert!(!after.free_vars().contains("x"), "{term}");
        assert_eq!(after.var_use_count("x"), 0);
        assert_eq!(term.var_use_count("x"), 1);
        assert!(term.free_vars().contains("x"));
        // Type substitution is a no-op on these shapes but must rebuild them.
        assert_eq!(term.substitute_type("a", &Type::Nat), term);
        assert!(term.free_type_vars().is_empty());
        assert!(!term.contains_sorry());
        assert_eq!(term.strip_spans(), term);
    }
    assert_eq!(int(3).substitute("x", &int(7)), int(3));
    assert!(int(3).is_value());
    assert!(!Term::int_neg(int(3)).is_value());
}

#[test]
fn int_terms_render_distinguishably_from_nat_ones() {
    assert_eq!(int(-2).to_string(), "int:-2");
    assert_eq!(
        Term::int_bin(IntBinOp::Sub, int(3), int(5)).to_string(),
        "(int:3 int- int:5)"
    );
    assert_eq!(Term::int_neg(int(1)).to_string(), "(intneg int:1)");
    assert_eq!(Term::nat_to_int(Term::Zero).to_string(), "(to_int zero)");
    assert_eq!(Term::int_to_nat(int(1)).to_string(), "(from_int int:1)");
    assert_eq!(Type::Int.to_string(), "Int");
    for (term, name) in [
        (int(1), "IntLit"),
        (Term::int_bin(IntBinOp::Eq, int(1), int(1)), "IntBin"),
        (Term::int_neg(int(1)), "IntNeg"),
        (Term::nat_to_int(Term::Zero), "NatToInt"),
        (Term::int_to_nat(int(1)), "IntToNat"),
    ] {
        assert_eq!(crate::diagnostics::term_shape::variant_name(&term), name);
    }
}

#[test]
fn walkers_visit_every_int_child_and_size_it() {
    for term in every_shape_over(Term::var("x")) {
        let mut seen = 0;
        term.for_each_subterm(|_| seen += 1);
        let expected = if matches!(term, Term::IntBin(..)) {
            2
        } else {
            1
        };
        assert_eq!(seen, expected, "{term}");
        assert!(crate::ffi::arena_stats::deep_term_bytes(&term) > 0);
    }
    assert_eq!(crate::ffi::arena_stats::deep_term_bytes(&int(1)), 0);
}

/// The shape-typing walk records `Int` for the new nodes: an eliminator
/// applied to one reports `Int` as the found former.
#[test]
fn shape_typing_records_int_for_the_new_nodes() {
    for term in [
        int(1),
        Term::int_neg(int(1)),
        Term::nat_to_int(Term::Zero),
        Term::int_bin(IntBinOp::Add, int(1), int(1)),
    ] {
        let mismatches = Term::fst(term.clone()).shape_mismatches();
        assert_eq!(mismatches.len(), 1, "{term}");
        assert_eq!(mismatches[0].label(), "fst over Int");
    }
    let comparison = Term::int_bin(IntBinOp::Lt, int(1), int(1));
    assert_eq!(
        Term::fst(comparison).shape_mismatches()[0].label(),
        "fst over Bool"
    );
    let bridged = Term::int_to_nat(int(1));
    assert_eq!(
        Term::fst(bridged).shape_mismatches()[0].label(),
        "fst over Nat"
    );
}

// ── the FFI constructors ────────────────────────────────────────────────────

#[test]
fn ffi_constructors_build_the_five_nodes_and_refuse_bad_inputs() {
    let a = tg_term_int_lit(-3);
    let b = tg_term_int_lit(4);
    assert_ne!(a, INVALID_HANDLE);
    for op in ALL_OPS {
        assert_ne!(tg_term_int_bin(op.code(), a, b), INVALID_HANDLE, "{op:?}");
    }
    assert_eq!(tg_term_int_bin(99, a, b), INVALID_HANDLE, "unknown op code");
    assert_eq!(tg_term_int_bin(0, a, INVALID_HANDLE), INVALID_HANDLE);
    assert_ne!(tg_term_int_neg(a), INVALID_HANDLE);
    assert_eq!(tg_term_int_neg(INVALID_HANDLE), INVALID_HANDLE);
    assert_ne!(tg_term_nat_to_int(a), INVALID_HANDLE);
    assert_eq!(tg_term_nat_to_int(INVALID_HANDLE), INVALID_HANDLE);
    assert_ne!(tg_term_int_to_nat(b), INVALID_HANDLE);
    assert_eq!(tg_term_int_to_nat(INVALID_HANDLE), INVALID_HANDLE);
    // The type node, and its tag — 17, appended after Adt (§2.1).
    assert_eq!(tg_type_tag(tg_type_int()), 17);
}
