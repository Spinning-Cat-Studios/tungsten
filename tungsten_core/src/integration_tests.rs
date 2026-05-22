use super::prelude::*;

/// Test: Identity function on Nat
#[test]
fn test_identity() {
    let ctx = Context::new();
    let id = Term::lambda("x", Type::Nat, Term::var("x"));
    let term = Term::app(id, Term::Zero);

    // Type check
    let ty = type_of(&ctx, &term).unwrap();
    assert_eq!(ty, Type::Nat);

    // Evaluate
    let result = eval(&term);
    assert_eq!(result, Term::Zero);
}

/// Test: Polymorphic identity function
#[test]
fn test_polymorphic_identity() {
    let ctx = Context::new();

    // Λα. λx:α. x
    let poly_id = Term::ty_abs(
        "α",
        Term::lambda("x", Type::TyVar("α".into()), Term::var("x")),
    );

    // Type check: ∀α. α → α
    let ty = type_of(&ctx, &poly_id).unwrap();
    assert_eq!(
        ty,
        Type::forall(
            "α",
            Type::arrow(Type::TyVar("α".into()), Type::TyVar("α".into()))
        )
    );

    // Instantiate at Nat and apply
    let app = Term::app(Term::ty_app(poly_id, Type::Nat), Term::Zero);
    let result = eval(&app);
    assert_eq!(result, Term::Zero);
}

/// Test: Church numeral addition using natrec
#[test]
fn test_addition() {
    let ctx = Context::new();

    // add m n = natrec [Nat] n (λ_. λacc. succ acc) m
    // This adds m to n by applying succ m times to n
    let add = Term::lambda(
        "m",
        Type::Nat,
        Term::lambda(
            "n",
            Type::Nat,
            Term::natrec(
                Type::Nat,
                Term::var("n"),
                Term::lambda(
                    "_",
                    Type::Nat,
                    Term::lambda("acc", Type::Nat, Term::succ(Term::var("acc"))),
                ),
                Term::var("m"),
            ),
        ),
    );

    // Type check: Nat → Nat → Nat
    let ty = type_of(&ctx, &add).unwrap();
    assert_eq!(
        ty,
        Type::arrow(Type::Nat, Type::arrow(Type::Nat, Type::Nat))
    );

    // 2 + 3 = 5
    let two = Term::nat(2);
    let three = Term::nat(3);
    let term = Term::app(Term::app(add, two), three);

    let result = eval(&term);
    assert_eq!(result, Term::nat(5));
}

/// Test: Pairs and projections
#[test]
fn test_pairs() {
    let ctx = Context::new();

    // swap : α × β → β × α
    // swap = Λα. Λβ. λp:α × β. (snd p, fst p)
    let alpha = Type::TyVar("α".into());
    let beta = Type::TyVar("β".into());
    let swap = Term::ty_abs(
        "α",
        Term::ty_abs(
            "β",
            Term::lambda(
                "p",
                Type::product(alpha.clone(), beta.clone()),
                Term::pair(Term::snd(Term::var("p")), Term::fst(Term::var("p"))),
            ),
        ),
    );

    // Type check
    let ty = type_of(&ctx, &swap).unwrap();
    let expected = Type::forall(
        "α",
        Type::forall(
            "β",
            Type::arrow(
                Type::product(alpha.clone(), beta.clone()),
                Type::product(beta, alpha),
            ),
        ),
    );
    assert_eq!(ty, expected);

    // swap [Nat] [Bool] (zero, true) = (true, zero)
    let app = Term::app(
        Term::ty_app(Term::ty_app(swap, Type::Nat), Type::Bool),
        Term::pair(Term::Zero, Term::True),
    );
    let result = eval(&app);
    assert_eq!(result, Term::pair(Term::True, Term::Zero));
}

/// Test: Sum types with case
#[test]
fn test_sums() {
    let ctx = Context::new();

    let sum_ty = Type::sum(Type::Nat, Type::Bool);

    // isLeft : Nat + Bool → Bool
    // isLeft = λx. case x of inl _ => true | inr _ => false
    let is_left = Term::lambda(
        "x",
        sum_ty.clone(),
        Term::case(Term::var("x"), "_", Term::True, "_", Term::False),
    );

    // Type check
    let ty = type_of(&ctx, &is_left).unwrap();
    assert_eq!(ty, Type::arrow(sum_ty.clone(), Type::Bool));

    // isLeft (inl [Nat + Bool] zero) = true
    let term1 = Term::app(is_left.clone(), Term::inl(sum_ty.clone(), Term::Zero));
    assert_eq!(eval(&term1), Term::True);

    // isLeft (inr [Nat + Bool] true) = false
    let term2 = Term::app(is_left, Term::inr(sum_ty, Term::True));
    assert_eq!(eval(&term2), Term::False);
}

/// Test: Equality proofs with refl and subst
#[test]
fn test_equality() {
    let ctx = Context::new();

    // refl [Nat] zero : Eq Nat zero zero
    let refl_zero = Term::refl(Type::Nat, Term::Zero);
    let ty = type_of(&ctx, &refl_zero).unwrap();
    assert_eq!(ty, Type::eq(Type::Nat, Term::Zero, Term::Zero));

    // Proof that zero == zero is a value
    assert!(refl_zero.is_value());
}

/// Test: Logical encoding (True = Unit, False = Void)
#[test]
fn test_logical_encoding() {
    let ctx = Context::new();

    // Unit is trivially provable
    let unit_proof = Term::Unit;
    assert_eq!(type_of(&ctx, &unit_proof).unwrap(), Type::Unit);

    // Void has no constructors
    // absurd can be typed if we have a Void value
    let ctx_with_void = ctx.with_term("contradiction", Type::Void);
    let absurd_term = Term::absurd(Type::Nat, Term::var("contradiction"));
    assert_eq!(type_of(&ctx_with_void, &absurd_term).unwrap(), Type::Nat);
}

/// Test: Multiple let bindings
#[test]
fn test_let_chain() {
    let ctx = Context::new();

    // let x = 1 in let y = 2 in x + y
    // Using natrec for addition
    let term = Term::let_in(
        "x",
        Type::Nat,
        Term::nat(1),
        Term::let_in(
            "y",
            Type::Nat,
            Term::nat(2),
            // x + y using natrec
            Term::natrec(
                Type::Nat,
                Term::var("y"),
                Term::lambda(
                    "_",
                    Type::Nat,
                    Term::lambda("acc", Type::Nat, Term::succ(Term::var("acc"))),
                ),
                Term::var("x"),
            ),
        ),
    );

    assert_eq!(type_of(&ctx, &term).unwrap(), Type::Nat);
    assert_eq!(eval(&term), Term::nat(3));
}

/// Test: Boolean operations
#[test]
fn test_boolean_ops() {
    let ctx = Context::new();

    // not : Bool → Bool
    let not = Term::lambda(
        "b",
        Type::Bool,
        Term::if_then_else(Term::var("b"), Term::False, Term::True),
    );
    assert_eq!(
        type_of(&ctx, &not).unwrap(),
        Type::arrow(Type::Bool, Type::Bool)
    );

    assert_eq!(eval(&Term::app(not.clone(), Term::True)), Term::False);
    assert_eq!(eval(&Term::app(not, Term::False)), Term::True);
}

/// Test: Sorry is stuck but type-checks (with annotation)
#[test]
fn test_sorry() {
    let ctx = Context::new();

    // (sorry : Nat) type-checks as Nat
    let annotated_sorry = Term::annot(Term::Sorry, Type::Nat);
    assert_eq!(type_of(&ctx, &annotated_sorry).unwrap(), Type::Nat);

    // (sorry : Bool → Bool) also type-checks
    let func_sorry = Term::annot(Term::Sorry, Type::arrow(Type::Bool, Type::Bool));
    assert_eq!(
        type_of(&ctx, &func_sorry).unwrap(),
        Type::arrow(Type::Bool, Type::Bool)
    );

    // But sorry doesn't reduce - it's stuck
    assert_eq!(eval(&Term::Sorry), Term::Sorry);
}
