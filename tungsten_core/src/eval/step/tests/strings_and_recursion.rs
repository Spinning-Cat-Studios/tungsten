use super::*;
use crate::eval::eval;
use crate::types::Type;

// ==========================================================================
// Phase 2A Tests: Strings
// ==========================================================================

#[test]
fn test_string_literal_is_value() {
    let term = Term::string_lit("hello");
    let result = step(&term);
    assert_eq!(result, StepResult::Value);
    assert!(term.is_value());
}

#[test]
fn test_str_concat() {
    let term = Term::str_concat(Term::string_lit("hello"), Term::string_lit(" world"));
    let result = eval(&term);
    assert_eq!(result, Term::string_lit("hello world"));
}

#[test]
fn test_str_concat_empty() {
    let term = Term::str_concat(Term::string_lit(""), Term::string_lit("test"));
    let result = eval(&term);
    assert_eq!(result, Term::string_lit("test"));
}

#[test]
fn test_str_len_empty() {
    let term = Term::str_len(Term::string_lit(""));
    let result = eval(&term);
    assert_eq!(result, Term::Zero);
}

#[test]
fn test_str_len_nonempty() {
    let term = Term::str_len(Term::string_lit("hello"));
    let result = eval(&term);
    assert_eq!(result, Term::nat(5));
}

#[test]
fn test_str_eq_true() {
    let term = Term::str_eq(Term::string_lit("hello"), Term::string_lit("hello"));
    let result = eval(&term);
    assert_eq!(result, Term::True);
}

#[test]
fn test_str_eq_false() {
    let term = Term::str_eq(Term::string_lit("hello"), Term::string_lit("world"));
    let result = eval(&term);
    assert_eq!(result, Term::False);
}

#[test]
fn test_str_concat_nested() {
    let term = Term::str_concat(
        Term::str_concat(Term::string_lit("a"), Term::string_lit("b")),
        Term::string_lit("c"),
    );
    let result = eval(&term);
    assert_eq!(result, Term::string_lit("abc"));
}

// ==========================================================================
// Phase 2A Tests: Fix Combinator
// ==========================================================================

#[test]
fn test_fix_identity() {
    let fix_term = Term::fix(
        "f",
        Type::arrow(Type::Nat, Type::Nat),
        Term::lambda("n", Type::Nat, Term::var("n")),
    );
    let term = Term::app(fix_term, Term::nat(5));
    let result = eval(&term);
    assert_eq!(result, Term::nat(5));
}

#[test]
fn test_fix_double() {
    let fix_term = Term::fix(
        "f",
        Type::arrow(Type::Nat, Type::Nat),
        Term::lambda("n", Type::Nat, Term::succ(Term::var("n"))),
    );
    let term = Term::app(fix_term, Term::nat(3));
    let result = eval(&term);
    assert_eq!(result, Term::nat(4));
}

// ==========================================================================
// Phase 2A Tests: μ-types (Iso-recursive)
// ==========================================================================

#[test]
fn test_fold_is_value() {
    let mu_ty = Type::mu("α", Type::sum(Type::Unit, Type::TyVar("α".into())));
    let term = Term::fold(
        mu_ty.clone(),
        Term::inl(Type::sum(Type::Unit, mu_ty), Term::Unit),
    );
    assert!(term.is_value());
    let result = step(&term);
    assert_eq!(result, StepResult::Value);
}

#[test]
fn test_unfold_fold() {
    let mu_ty = Type::mu("α", Type::sum(Type::Unit, Type::TyVar("α".into())));
    let inner = Term::inl(Type::sum(Type::Unit, mu_ty.clone()), Term::Unit);
    let folded = Term::fold(mu_ty.clone(), inner.clone());
    let term = Term::unfold(mu_ty, folded);
    let result = eval(&term);
    assert_eq!(result, inner);
}

#[test]
fn test_mu_peano_zero() {
    let peano = Type::mu("α", Type::sum(Type::Unit, Type::TyVar("α".into())));
    let unfolded_ty = Type::sum(Type::Unit, peano.clone());
    let zero = Term::fold(peano.clone(), Term::inl(unfolded_ty.clone(), Term::Unit));

    let unfolded = Term::unfold(peano, zero);
    let result = eval(&unfolded);

    match result {
        Term::Inl(_, inner) => assert_eq!(*inner, Term::Unit),
        _ => panic!("Expected Inl"),
    }
}

#[test]
fn test_mu_peano_succ() {
    let peano = Type::mu("α", Type::sum(Type::Unit, Type::TyVar("α".into())));
    let unfolded_ty = Type::sum(Type::Unit, peano.clone());

    let zero = Term::fold(peano.clone(), Term::inl(unfolded_ty.clone(), Term::Unit));
    let one = Term::fold(peano.clone(), Term::inr(unfolded_ty.clone(), zero));

    let unfolded = Term::unfold(peano.clone(), one);
    let result = eval(&unfolded);

    let Term::Inr(_, inner) = result else {
        panic!("Expected Inr");
    };
    let Term::Fold(_, inner_inner) = inner.as_ref() else {
        panic!("Expected Fold inside Inr");
    };
    let Term::Inl(_, u) = inner_inner.as_ref() else {
        panic!("Expected Inl inside Fold");
    };
    assert_eq!(u.as_ref(), &Term::Unit);
}
