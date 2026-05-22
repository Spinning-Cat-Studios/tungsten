use super::*;

// ======================================================================
// Display: grouped fmt helpers
// ======================================================================

#[test]
fn test_display_infix_ops() {
    let add = Term::NatAdd(Box::new(Term::var("x")), Box::new(Term::var("y")));
    assert_eq!(add.to_string(), "(x + y)");
    let eq = Term::NatEq(Box::new(Term::NatLit(1)), Box::new(Term::NatLit(2)));
    assert_eq!(eq.to_string(), "(1 == 2)");
    let and = Term::BoolAnd(Box::new(Term::True), Box::new(Term::False));
    assert_eq!(and.to_string(), "(true && false)");
}

#[test]
fn test_display_keyword_unary() {
    assert_eq!(Term::succ(Term::Zero).to_string(), "(succ zero)");
    assert_eq!(Term::fst(Term::var("p")).to_string(), "(fst p)");
    assert_eq!(Term::snd(Term::var("p")).to_string(), "(snd p)");
    assert_eq!(Term::str_len(Term::var("s")).to_string(), "(strlen s)");
}

#[test]
fn test_display_typed_unary() {
    assert_eq!(
        Term::inl(Type::Nat, Term::Zero).to_string(),
        "(inl [Nat] zero)"
    );
    assert_eq!(
        Term::refl(Type::Bool, Term::True).to_string(),
        "(refl [Bool] true)"
    );
}

#[test]
fn test_display_keyword_binary() {
    let cat = Term::StrConcat(Box::new(Term::var("a")), Box::new(Term::var("b")));
    assert_eq!(cat.to_string(), "(strconcat a b)");
    let set = Term::RefSet(Box::new(Term::var("r")), Box::new(Term::var("v")));
    assert_eq!(set.to_string(), "(ref_set r v)");
}

#[test]
fn test_display_leaf_constants() {
    assert_eq!(Term::True.to_string(), "true");
    assert_eq!(Term::False.to_string(), "false");
    assert_eq!(Term::Unit.to_string(), "()");
    assert_eq!(Term::Zero.to_string(), "zero");
    assert_eq!(Term::Sorry.to_string(), "sorry");
}

// ======================================================================
// Display: typed-ternary helper
// ======================================================================

#[test]
fn test_display_typed_ternary() {
    let natrec = Term::natrec(Type::Nat, Term::Zero, Term::var("s"), Term::var("n"));
    assert_eq!(natrec.to_string(), "(natrec [Nat] zero s n)");

    let natind = Term::natind(Type::Bool, Term::var("z"), Term::var("s"), Term::var("n"));
    assert_eq!(natind.to_string(), "(natind [Bool] z s n)");
}

#[test]
fn test_display_extern_call() {
    let call = Term::ExternCall("foo".into(), vec![Term::var("x"), Term::var("y")]);
    assert_eq!(call.to_string(), "(extern_call foo x y)");
}

#[test]
fn test_display_adt_match() {
    let m = Term::adt_match(
        Term::var("scrut"),
        vec![
            (0, "a".into(), Box::new(Term::var("a"))),
            (1, "b".into(), Box::new(Term::var("b"))),
        ],
    );
    assert_eq!(m.to_string(), "(adt_match scrut [0 a => a | 1 b => b])");
}

#[test]
fn test_display_binding_forms() {
    let lam = Term::lambda("x", Type::Nat, Term::var("x"));
    assert_eq!(lam.to_string(), "(λx:Nat. x)");

    let fix = Term::Fix(
        "f".into(),
        Type::arrow(Type::Nat, Type::Nat),
        Box::new(Term::var("f")),
    );
    assert_eq!(fix.to_string(), "(fix f:(Nat → Nat). f)");
}
