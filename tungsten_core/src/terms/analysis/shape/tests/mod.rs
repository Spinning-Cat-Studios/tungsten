//! The shape rule, per former, over injected terms (ADR 3.9.26h AC3).
//!
//! Every case is a hand-built `Term`, so nothing here needs an elaboration, a
//! self-compile or a devcontainer. Each ill-formed eliminator is paired with
//! its **well-formed counterpart**: a rule asserted only on its findings passes
//! just as happily when it reports everything, and this check's whole risk is
//! false positives over 2298 definitions.

use super::{Eliminator, Former, ShapeMismatch};
use crate::terms::Term;
use crate::types::Type;

mod derivation;

/// `let x : ty = value in body`, the shortest way to give a variable a
/// recorded type.
fn bound(ty: Type, body: Term) -> Term {
    Term::Let("x".to_string(), ty, Box::new(Term::Sorry), Box::new(body))
}

fn var() -> Box<Term> {
    Box::new(Term::Var("x".to_string()))
}

fn pair_type() -> Type {
    Type::Product(Box::new(Type::Nat), Box::new(Type::String))
}

fn sum_type() -> Type {
    Type::Sum(Box::new(Type::Nat), Box::new(Type::String))
}

fn mu_type() -> Type {
    Type::Mu(
        "a".to_string(),
        Box::new(Type::Sum(
            Box::new(Type::Unit),
            Box::new(Type::TyVar("a".to_string())),
        )),
    )
}

fn found(term: &Term) -> Vec<(Eliminator, Former)> {
    term.shape_mismatches()
        .into_iter()
        .map(|ShapeMismatch { eliminator, found }| (eliminator, found))
        .collect()
}

fn case_over(scrutinee: Term) -> Term {
    Term::Case(
        Box::new(scrutinee),
        "l".to_string(),
        Box::new(Term::Unit),
        "r".to_string(),
        Box::new(Term::Unit),
    )
}

// --- fst / snd require a product -----------------------------------------

#[test]
fn fst_over_a_scalar_is_a_finding_and_over_a_product_is_not() {
    // The shape `build_tuple_projection_terms` emitted for a tuple's last
    // element: `fst` of something no recorded type says is a pair.
    let ill = bound(Type::Nat, Term::Fst(var()));
    assert_eq!(found(&ill), vec![(Eliminator::Fst, Former::Ground("Nat"))]);

    let well = bound(pair_type(), Term::Fst(var()));
    assert_eq!(found(&well), vec![]);
}

#[test]
fn snd_over_a_scalar_is_a_finding_and_over_a_product_is_not() {
    let ill = bound(Type::String, Term::Snd(var()));
    assert_eq!(
        found(&ill),
        vec![(Eliminator::Snd, Former::Ground("String"))]
    );

    let well = bound(pair_type(), Term::Snd(var()));
    assert_eq!(found(&well), vec![]);
}

#[test]
fn a_projection_chain_reports_the_step_that_ran_out_of_product() {
    // `fst (snd x)` where `x : Nat × String`. `snd x : String`, so the outer
    // `fst` is the fault — and the inner `snd` is not.
    let term = bound(pair_type(), Term::Fst(Box::new(Term::Snd(var()))));
    assert_eq!(
        found(&term),
        vec![(Eliminator::Fst, Former::Ground("String"))]
    );
}

// --- app requires an arrow ------------------------------------------------

#[test]
fn applying_a_lambdas_result_is_a_finding_and_a_saturated_call_is_not() {
    // ADR 3.9.26e's shape: a curried constructor arrow paired with a UNARY
    // lambda, so the second argument is applied to the lambda's result.
    let unary = Term::Lambda(
        "p".to_string(),
        pair_type(),
        Box::new(Term::Fold(mu_type(), Box::new(Term::Unit))),
    );
    let ill = Term::App(
        Box::new(Term::App(Box::new(unary), Box::new(Term::NatLit(9)))),
        Box::new(Term::StringLit("N2".to_string())),
    );
    assert_eq!(found(&ill), vec![(Eliminator::App, Former::Mu)]);

    let binary = Term::Lambda(
        "a".to_string(),
        Type::Nat,
        Box::new(Term::Lambda(
            "b".to_string(),
            Type::String,
            Box::new(Term::Unit),
        )),
    );
    let well = Term::App(
        Box::new(Term::App(Box::new(binary), Box::new(Term::NatLit(9)))),
        Box::new(Term::StringLit("N2".to_string())),
    );
    assert_eq!(found(&well), vec![]);
}

#[test]
fn app_over_a_forall_is_tolerated() {
    // Instantiation is not always recorded as a `TyApp`, so judging this would
    // report every generic call site in the corpus.
    let polymorphic = Type::Forall("a".to_string(), Box::new(Type::Nat));
    let term = bound(polymorphic, Term::App(var(), Box::new(Term::Unit)));
    assert_eq!(found(&term), vec![]);
}

// --- case requires a sum --------------------------------------------------

#[test]
fn case_over_a_scalar_is_a_finding_and_over_a_sum_is_not() {
    let ill = bound(Type::Nat, case_over(Term::Var("x".to_string())));
    assert_eq!(found(&ill), vec![(Eliminator::Case, Former::Ground("Nat"))]);

    let well = bound(sum_type(), case_over(Term::Var("x".to_string())));
    assert_eq!(found(&well), vec![]);
}

#[test]
fn case_over_a_flat_adt_is_tolerated() {
    // Two constructors elaborate to a `Sum` and three or more to `Type::Adt`;
    // which one a scrutinee carries is an encoding choice, not a shape fault.
    let adt = Type::Adt(
        "Colour".to_string(),
        vec![],
        vec![("Red".to_string(), Type::Unit)],
    );
    let term = bound(adt, case_over(Term::Var("x".to_string())));
    assert_eq!(found(&term), vec![]);
}

// --- unfold requires a mu -------------------------------------------------

#[test]
fn unfold_over_a_non_mu_is_a_finding_and_over_a_mu_is_not() {
    let ill = bound(Type::Nat, Term::Unfold(mu_type(), var()));
    assert_eq!(
        found(&ill),
        vec![(Eliminator::Unfold, Former::Ground("Nat"))]
    );

    let well = bound(mu_type(), Term::Unfold(mu_type(), var()));
    assert_eq!(found(&well), vec![]);
}

// --- silence where nothing was recorded -----------------------------------

#[test]
fn an_unrecorded_operand_is_silence_rather_than_a_finding() {
    // A global's type is not on the term, and neither is a type variable's.
    // Both must report nothing: this check's budget is entirely false
    // positives, and there are 2298 definitions to be wrong about.
    let global = Term::Fst(Box::new(Term::Global("nowhere".to_string())));
    assert_eq!(found(&global), vec![]);

    let opaque = bound(Type::TyVar("T".to_string()), Term::Fst(var()));
    assert_eq!(found(&opaque), vec![]);

    let poisoned = bound(Type::Error, Term::Fst(var()));
    assert_eq!(found(&poisoned), vec![]);
}

#[test]
fn branches_that_disagree_report_no_type_rather_than_the_first_one() {
    // `fst (if c then x else 0)` where `x : Nat × String`. The branches record
    // different types, so the `if` records none and the `fst` is not judged —
    // picking the first branch would make the second a phantom finding.
    let disagreeing = Term::If(Box::new(Term::True), var(), Box::new(Term::NatLit(0)));
    let term = bound(pair_type(), Term::Fst(Box::new(disagreeing)));
    assert_eq!(found(&term), vec![]);
}

#[test]
fn branches_that_agree_carry_their_type_to_the_eliminator() {
    let agreeing = Term::If(Box::new(Term::True), var(), var());
    let term = bound(Type::Nat, Term::Fst(Box::new(agreeing)));
    assert_eq!(found(&term), vec![(Eliminator::Fst, Former::Ground("Nat"))]);
}

#[test]
fn a_binding_shadowed_by_an_inner_lambda_uses_the_inner_type() {
    // `let x : Nat × String = _ in λx:Nat. fst x` — the projection is over the
    // LAMBDA's `x`, so the outer product must not launder it.
    let inner = Term::Lambda("x".to_string(), Type::Nat, Box::new(Term::Fst(var())));
    let term = bound(pair_type(), inner);
    assert_eq!(found(&term), vec![(Eliminator::Fst, Former::Ground("Nat"))]);
}

#[test]
fn a_pattern_variable_is_bound_at_its_variants_payload() {
    // The arm's payload type comes from the scrutinee's ADT, so a projection
    // inside an arm is judged rather than skipped.
    let adt = Type::Adt(
        "Box".to_string(),
        vec![],
        vec![("Mk".to_string(), Type::Nat)],
    );
    let term = bound(
        adt,
        Term::AdtMatch(
            var(),
            vec![(
                0,
                "p".to_string(),
                Box::new(Term::Fst(Box::new(Term::Var("p".to_string())))),
            )],
        ),
    );
    assert_eq!(found(&term), vec![(Eliminator::Fst, Former::Ground("Nat"))]);
}

#[test]
fn a_finding_reads_as_the_eliminator_over_the_former() {
    let mismatch = ShapeMismatch {
        eliminator: Eliminator::App,
        found: Former::Product,
    };
    assert_eq!(mismatch.label(), "app over a product");
    assert_eq!(
        ShapeMismatch {
            eliminator: Eliminator::Fst,
            found: Former::Ground("Nat"),
        }
        .label(),
        "fst over Nat"
    );
}

#[test]
fn every_eliminator_accepts_exactly_its_own_former_and_the_unrecorded_one() {
    // The compatibility table, asserted as a table: a rule that accepted
    // everything would pass every well-formed case above.
    let formers = [
        Former::Arrow,
        Former::Product,
        Former::Sum,
        Former::Mu,
        Former::Ground("Nat"),
    ];
    for eliminator in [
        Eliminator::Fst,
        Eliminator::Snd,
        Eliminator::App,
        Eliminator::Case,
        Eliminator::Unfold,
    ] {
        assert!(eliminator.accepts(Former::Opaque), "{eliminator:?}");
        for former in formers {
            assert_eq!(
                eliminator.accepts(former),
                former == eliminator.required(),
                "{eliminator:?} over {former:?}"
            );
        }
    }
}

#[test]
fn former_of_reads_every_ground_type_by_its_own_name() {
    assert_eq!(super::former_of(&Type::Nat), Former::Ground("Nat"));
    assert_eq!(super::former_of(&Type::Bool), Former::Ground("Bool"));
    assert_eq!(super::former_of(&Type::Unit), Former::Ground("Unit"));
    assert_eq!(super::former_of(&Type::Void), Former::Ground("Void"));
    assert_eq!(super::former_of(&Type::Prop), Former::Ground("Prop"));
    assert_eq!(super::former_of(&Type::String), Former::Ground("String"));
    assert_eq!(
        super::former_of(&Type::App("Pending".to_string(), vec![])),
        Former::Opaque
    );
    assert_eq!(
        super::former_of(&Type::Ptr(Box::new(Type::Nat))),
        Former::Ptr
    );
    assert_eq!(
        super::former_of(&Type::Ref(Box::new(Type::Nat))),
        Former::Ref
    );
}
