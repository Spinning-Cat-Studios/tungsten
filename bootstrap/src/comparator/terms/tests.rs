//! Shape tests for synthesized comparator `Term`s. Each expected value is the
//! Core IR grounded via `tungsten info def` (ADR 29.6.26f, design doc §T11) —
//! these lock the synthesized shape to what the elaborator emits for the
//! equivalent hand-written `.tg`.

use super::*;
use tungsten_core::{Term, Type};

fn sum_unit_diff() -> Type {
    Type::Sum(
        Box::new(Type::Unit),
        Box::new(Type::TyVar("CompareDiff".to_string())),
    )
}

#[test]
fn equal_is_inl_unit() {
    // Grounded: `Equal` → (inl [(Unit + CompareDiff)] ())
    assert_eq!(
        equal_term(),
        Term::Inl(sum_unit_diff(), Box::new(Term::Unit))
    );
}

#[test]
fn not_equal_is_inr_payload() {
    // Grounded: `NotEqual(d)` → (inr [(Unit + CompareDiff)] d)
    let payload = Term::Var("d".to_string());
    assert_eq!(
        not_equal_term(payload.clone()),
        Term::Inr(sum_unit_diff(), Box::new(payload))
    );
}

#[test]
fn empty_path_is_fold_inl_unit() {
    // Grounded: `Nil : List<PathSeg>` →
    //   (fold [μα_List. (Unit + (PathSeg × α_List))]
    //         (inl [(Unit + (PathSeg × μα_List…))] ()))
    let mu = Type::Mu(
        "α_List".to_string(),
        Box::new(Type::Sum(
            Box::new(Type::Unit),
            Box::new(Type::Product(
                Box::new(Type::TyVar("PathSeg".to_string())),
                Box::new(Type::TyVar("α_List".to_string())),
            )),
        )),
    );
    let inner_sum = Type::Sum(
        Box::new(Type::Unit),
        Box::new(Type::Product(
            Box::new(Type::TyVar("PathSeg".to_string())),
            Box::new(mu.clone()),
        )),
    );
    let expected = Term::Fold(mu, Box::new(Term::Inl(inner_sum, Box::new(Term::Unit))));
    assert_eq!(empty_path_term(), expected);
}

#[test]
fn empty_diff_is_pair_path_displays() {
    // Grounded: (empty_path, ("", ""))
    let expected = Term::Pair(
        Box::new(empty_path_term()),
        Box::new(Term::Pair(
            Box::new(Term::StringLit(String::new())),
            Box::new(Term::StringLit(String::new())),
        )),
    );
    assert_eq!(empty_diff_term(), expected);
}

#[test]
fn leaf_body_is_if_eq_equal_notequal() {
    let eq = Term::NatEq(
        Box::new(Term::Var("a".to_string())),
        Box::new(Term::Var("b".to_string())),
    );
    let expected = Term::If(
        Box::new(eq.clone()),
        Box::new(equal_term()),
        Box::new(not_equal_term(empty_diff_term())),
    );
    assert_eq!(leaf_compare_body(eq), expected);
}

#[test]
fn nat_comparator_is_curried_lambda() {
    // Grounded compare_Nat shape: λa:Nat. λb:Nat. if (a == b) then Equal else NotEqual(diff)
    let term = leaf_comparator_term(Type::Nat, nat_eq);
    let Term::Lambda(a, ty_a, inner) = &term else {
        panic!("outer not a lambda: {term:?}");
    };
    assert_eq!(a, "a");
    assert_eq!(*ty_a, Type::Nat);
    let Term::Lambda(b, ty_b, body) = inner.as_ref() else {
        panic!("inner not a lambda");
    };
    assert_eq!(b, "b");
    assert_eq!(*ty_b, Type::Nat);
    // Body is an `if` whose condition is NatEq(a, b).
    let Term::If(cond, _, _) = body.as_ref() else {
        panic!("body not an if");
    };
    assert_eq!(
        cond.as_ref(),
        &Term::NatEq(
            Box::new(Term::Var("a".to_string())),
            Box::new(Term::Var("b".to_string()))
        )
    );
}

#[test]
fn bool_eq_uses_if_not() {
    let a = Term::Var("a".to_string());
    let b = Term::Var("b".to_string());
    assert_eq!(
        bool_eq(a.clone(), b.clone()),
        Term::If(
            Box::new(a),
            Box::new(b.clone()),
            Box::new(Term::BoolNot(Box::new(b)))
        )
    );
}

#[test]
fn seg_pos_is_adt_construct_index_1() {
    // PathSeg::Pos is constructor index 1 with a Nat payload.
    assert_eq!(
        seg_pos(2),
        Term::AdtConstruct(
            Type::TyVar("PathSeg".to_string()),
            1,
            Box::new(Term::NatLit(2))
        )
    );
}

#[test]
fn seg_tag_is_adt_construct_index_3() {
    // PathSeg::Tag is nullary constructor index 3.
    assert_eq!(
        seg_tag(),
        Term::AdtConstruct(Type::TyVar("PathSeg".to_string()), 3, Box::new(Term::Unit))
    );
}

#[test]
fn prepend_seg_is_case_keeping_equal_and_consing_on_notequal() {
    let result = Term::Var("x".to_string());
    let Term::Case(scrut, eq_v, eq_body, ne_v, ne_body) = prepend_seg(result, seg_tag()) else {
        panic!("prepend_seg is not a Case");
    };
    assert_eq!(*scrut, Term::Var("x".to_string()));
    assert_eq!(eq_v, "_e");
    // Equal arm stays Equal (Inl unit).
    assert!(matches!(eq_body.as_ref(), Term::Inl(_, _)));
    // NotEqual arm rebuilds Inr(...) from the bound diff `d`.
    assert_eq!(ne_v, "d");
    assert!(matches!(ne_body.as_ref(), Term::Inr(_, _)));
}

#[test]
fn str_eq_is_streq_primitive() {
    let a = Term::Var("a".to_string());
    let b = Term::Var("b".to_string());
    assert_eq!(
        str_eq(a.clone(), b.clone()),
        Term::StrEq(Box::new(a), Box::new(b))
    );
}
