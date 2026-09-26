//! Tests for dead-arm-aware result-type unification (ADR 3.7.26a).
//!
//! An arm typed ⊥ (`Type::Void`, i.e. terminated by an early `return`) must
//! not contribute its placeholder type to an If/Case/AdtMatch result type —
//! the poisoned type propagated into lambda return-type derivation and
//! produced the T1 shrinking-cast rejection (`{ i32, [N x i8] }` → `{}`).

use super::arms::unify_arm_result_types;
use crate::codegen::backend::CodeGenError;
use crate::codegen::CodeGen;
use inkwell::context::Context;
use tungsten_core::terms::Term;
use tungsten_core::types::Type;

fn setup_codegen(context: &Context) -> CodeGen {
    CodeGen::new(context, "arms_test")
}

fn err(msg: &str) -> Result<Type, CodeGenError> {
    Err(CodeGenError::TypeError(msg.to_string()))
}

// ── unify_arm_result_types ──────────────────────────────────────────

#[test]
fn dead_arm_excluded_from_unification() {
    let ty = unify_arm_result_types([Ok(Type::Void), Ok(Type::Nat)]).unwrap();
    assert_eq!(ty, Type::Nat);
}

#[test]
fn all_dead_arms_unify_to_void() {
    let ty = unify_arm_result_types([Ok(Type::Void), Ok(Type::Void)]).unwrap();
    assert_eq!(ty, Type::Void);
}

#[test]
fn first_live_arm_wins() {
    // Documents the simplified inferencer's first-arm-wins convention.
    let ty = unify_arm_result_types([Ok(Type::Nat), Ok(Type::Bool)]).unwrap();
    assert_eq!(ty, Type::Nat);
}

#[test]
fn uninferable_arm_skipped_when_sibling_is_live() {
    let ty = unify_arm_result_types([err("no"), Ok(Type::Bool)]).unwrap();
    assert_eq!(ty, Type::Bool);
}

#[test]
fn uninferable_arm_skipped_when_sibling_is_dead() {
    let ty = unify_arm_result_types([err("no"), Ok(Type::Void)]).unwrap();
    assert_eq!(ty, Type::Void);
}

#[test]
fn no_inferable_arm_propagates_error() {
    assert!(unify_arm_result_types([err("a"), err("b")]).is_err());
}

// ── If / Case / AdtMatch inference ──────────────────────────────────

#[test]
fn if_with_dead_then_arm_types_from_else() {
    // if c { return 1 } else { 2 } : Nat (previously: Void, from then-first)
    let context = Context::create();
    let codegen = setup_codegen(&context);

    let term = Term::If(
        Box::new(Term::True),
        Box::new(Term::Return(Box::new(Term::NatLit(1)))),
        Box::new(Term::NatLit(2)),
    );
    assert_eq!(codegen.infer_term_type(&term).unwrap(), Type::Nat);
}

#[test]
fn if_with_both_arms_dead_is_void() {
    let context = Context::create();
    let codegen = setup_codegen(&context);

    let term = Term::If(
        Box::new(Term::True),
        Box::new(Term::Return(Box::new(Term::NatLit(1)))),
        Box::new(Term::Return(Box::new(Term::NatLit(2)))),
    );
    assert_eq!(codegen.infer_term_type(&term).unwrap(), Type::Void);
}

#[test]
fn case_with_dead_left_arm_types_from_right() {
    // The exact let-else shape: case scrut of inl _ => return … | inr v => v
    let context = Context::create();
    let codegen = setup_codegen(&context);

    let sum_ty = Type::Sum(Box::new(Type::Unit), Box::new(Type::Nat));
    let scrut = Term::Inl(sum_ty, Box::new(Term::Unit));
    let term = Term::Case(
        Box::new(scrut),
        "dead".to_string(),
        Box::new(Term::Return(Box::new(Term::NatLit(0)))),
        "v".to_string(),
        Box::new(Term::Var("v".to_string())),
    );
    assert_eq!(codegen.infer_term_type(&term).unwrap(), Type::Nat);
}

#[test]
fn case_with_non_sum_scrutinee_falls_back_dead_arm_aware() {
    // infer_case_type's fallback branch (scrutinee does not type as a Sum):
    // arms are inferred without payload bindings — still dead-arm aware.
    let context = Context::create();
    let codegen = setup_codegen(&context);

    let term = Term::Case(
        Box::new(Term::NatLit(1)), // Nat, not a Sum — takes the fallback path
        "dead".to_string(),
        Box::new(Term::Return(Box::new(Term::NatLit(0)))),
        "v".to_string(),
        Box::new(Term::NatLit(2)),
    );
    assert_eq!(codegen.infer_term_type(&term).unwrap(), Type::Nat);
}

#[test]
fn adt_match_with_dead_first_arm_types_from_second() {
    let context = Context::create();
    let codegen = setup_codegen(&context);

    let adt_ty = Type::Adt(
        "MaybeHit".to_string(),
        Vec::new(),
        vec![
            ("NoHit".to_string(), Type::Unit),
            ("SomeHit".to_string(), Type::Nat),
        ],
    );
    let scrut = Term::Annot(Box::new(Term::Unit), adt_ty);
    let arms = vec![
        (
            0,
            "_dead".to_string(),
            Box::new(Term::Return(Box::new(Term::NatLit(0)))),
        ),
        (1, "v".to_string(), Box::new(Term::Var("v".to_string()))),
    ];
    let term = Term::AdtMatch(Box::new(scrut), arms);
    assert_eq!(codegen.infer_term_type(&term).unwrap(), Type::Nat);
}

#[test]
fn adt_match_arm_binds_its_own_variant_payload() {
    // Arm 1's var must bind SomeHit's Nat payload, not variant 0's Unit.
    let context = Context::create();
    let codegen = setup_codegen(&context);

    let adt_ty = Type::Adt(
        "MaybeHit".to_string(),
        Vec::new(),
        vec![
            ("NoHit".to_string(), Type::Unit),
            ("SomeHit".to_string(), Type::Nat),
        ],
    );
    let scrut = Term::Annot(Box::new(Term::Unit), adt_ty);
    let arms = vec![(1, "v".to_string(), Box::new(Term::Var("v".to_string())))];
    let term = Term::AdtMatch(Box::new(scrut), arms);
    assert_eq!(codegen.infer_term_type(&term).unwrap(), Type::Nat);
}

#[test]
fn lambda_over_dead_arm_case_gets_live_return_type() {
    // The 3.7.26a trigger: the lambda's derived return type must come from
    // the live arm, not the dead arm's ⊥ placeholder ({} in LLVM).
    let context = Context::create();
    let codegen = setup_codegen(&context);

    let sum_ty = Type::Sum(Box::new(Type::Unit), Box::new(Type::Nat));
    let case = Term::Case(
        Box::new(Term::Inl(sum_ty, Box::new(Term::Unit))),
        "dead".to_string(),
        Box::new(Term::Return(Box::new(Term::NatLit(0)))),
        "v".to_string(),
        Box::new(Term::Var("v".to_string())),
    );
    let lambda = Term::Lambda("target".to_string(), Type::Nat, Box::new(case));
    assert_eq!(
        codegen.infer_term_type(&lambda).unwrap(),
        Type::arrow(Type::Nat, Type::Nat)
    );
}
