use super::*;

#[test]
fn test_pair() {
    let ctx = Context::new();
    let pair = Term::pair(Term::Zero, Term::True);
    assert_eq!(
        type_of(&ctx, &pair),
        Ok(Type::product(Type::Nat, Type::Bool))
    );
}

#[test]
fn test_fst() {
    let ctx = Context::new();
    let pair = Term::pair(Term::Zero, Term::True);
    let fst = Term::fst(pair);
    assert_eq!(type_of(&ctx, &fst), Ok(Type::Nat));
}

#[test]
fn test_snd() {
    let ctx = Context::new();
    let pair = Term::pair(Term::Zero, Term::True);
    let snd = Term::snd(pair);
    assert_eq!(type_of(&ctx, &snd), Ok(Type::Bool));
}

#[test]
fn test_fst_not_a_product() {
    let ctx = Context::new();
    let term = Term::fst(Term::Zero);
    assert!(matches!(
        type_of(&ctx, &term),
        Err(TypeError::NotAProduct { .. })
    ));
}

#[test]
fn test_snd_not_a_product() {
    let ctx = Context::new();
    let term = Term::snd(Term::True);
    assert!(matches!(
        type_of(&ctx, &term),
        Err(TypeError::NotAProduct { .. })
    ));
}

#[test]
fn test_sum_inl() {
    let ctx = Context::new();
    let sum_ty = Type::sum(Type::Nat, Type::Bool);
    let inl = Term::inl(sum_ty.clone(), Term::Zero);
    assert_eq!(type_of(&ctx, &inl), Ok(sum_ty));
}

#[test]
fn test_sum_inr() {
    let ctx = Context::new();
    let sum_ty = Type::sum(Type::Nat, Type::Bool);
    let inr = Term::inr(sum_ty.clone(), Term::True);
    assert_eq!(type_of(&ctx, &inr), Ok(sum_ty));
}

#[test]
fn test_case() {
    let ctx = Context::new();
    let sum_ty = Type::sum(Type::Nat, Type::Bool);
    let scrut = Term::inl(sum_ty, Term::Zero);
    let case = Term::case(
        scrut,
        "n",
        Term::var("n"), // : Nat
        "b",
        Term::if_then_else(Term::var("b"), Term::Zero, Term::succ(Term::Zero)), // : Nat
    );
    assert_eq!(type_of(&ctx, &case), Ok(Type::Nat));
}

#[test]
fn test_if() {
    let ctx = Context::new();
    let term = Term::if_then_else(Term::True, Term::Zero, Term::succ(Term::Zero));
    assert_eq!(type_of(&ctx, &term), Ok(Type::Nat));
}

#[test]
fn test_if_branch_mismatch() {
    let ctx = Context::new();
    let term = Term::if_then_else(Term::True, Term::Zero, Term::True);
    assert!(matches!(
        type_of(&ctx, &term),
        Err(TypeError::BranchTypeMismatch { .. })
    ));
}
