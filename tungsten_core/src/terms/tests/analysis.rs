use super::*;
use crate::types::Type;

#[test]
fn sorry_leaf_is_detected() {
    assert!(Term::Sorry.contains_sorry());
}

#[test]
fn non_sorry_leaves_are_clean() {
    assert!(!Term::Zero.contains_sorry());
    assert!(!Term::True.contains_sorry());
    assert!(!Term::Unit.contains_sorry());
    assert!(!Term::StringLit("hello".to_string()).contains_sorry());
    assert!(!Term::NatLit(42).contains_sorry());
}

#[test]
fn sorry_nested_in_succ() {
    let term = Term::Succ(Box::new(Term::Sorry));
    assert!(term.contains_sorry());
}

#[test]
fn sorry_nested_in_app() {
    let clean = Term::App(Box::new(Term::Zero), Box::new(Term::Unit));
    assert!(!clean.contains_sorry());

    let dirty_fn = Term::App(Box::new(Term::Sorry), Box::new(Term::Unit));
    assert!(dirty_fn.contains_sorry());

    let dirty_arg = Term::App(Box::new(Term::Zero), Box::new(Term::Sorry));
    assert!(dirty_arg.contains_sorry());
}

#[test]
fn sorry_in_let_body() {
    let term = Term::Let(
        "x".to_string(),
        Type::Nat,
        Box::new(Term::Zero),
        Box::new(Term::Sorry),
    );
    assert!(term.contains_sorry());
}

#[test]
fn sorry_in_if_branch() {
    let term = Term::If(
        Box::new(Term::True),
        Box::new(Term::Zero),
        Box::new(Term::Sorry),
    );
    assert!(term.contains_sorry());
}

#[test]
fn sorry_in_extern_call_args() {
    let clean = Term::ExternCall("foo".to_string(), vec![Term::Zero, Term::Unit]);
    assert!(!clean.contains_sorry());

    let dirty = Term::ExternCall("foo".to_string(), vec![Term::Zero, Term::Sorry]);
    assert!(dirty.contains_sorry());
}

#[test]
fn sorry_in_adt_match_arm() {
    let term = Term::AdtMatch(
        Box::new(Term::Zero),
        vec![(0, "x".to_string(), Box::new(Term::Sorry))],
    );
    assert!(term.contains_sorry());
}

#[test]
fn deeply_nested_sorry() {
    // Lambda wrapping Fix wrapping Pair with Sorry buried inside
    let inner = Term::Pair(Box::new(Term::Zero), Box::new(Term::Sorry));
    let fix = Term::Fix("f".to_string(), Type::Nat, Box::new(inner));
    let lam = Term::Lambda("x".to_string(), Type::Nat, Box::new(fix));
    assert!(lam.contains_sorry());
}

#[test]
fn clean_complex_term() {
    let inner = Term::Pair(Box::new(Term::Zero), Box::new(Term::True));
    let lam = Term::Lambda("x".to_string(), Type::Nat, Box::new(inner));
    assert!(!lam.contains_sorry());
}

// ── var_use_count tests ─────────────────────────────────────────────

fn tvar(name: &str) -> Term {
    Term::Var(name.to_string())
}

fn str_concat(l: Term, r: Term) -> Term {
    Term::StrConcat(Box::new(l), Box::new(r))
}

fn let_str(name: &str, def: Term, body: Term) -> Term {
    Term::Let(
        name.to_string(),
        Type::String,
        Box::new(def),
        Box::new(body),
    )
}

#[test]
fn var_use_count_single_use() {
    let term = tvar("x");
    assert_eq!(term.var_use_count("x"), 1);
    assert_eq!(term.var_use_count("y"), 0);
}

#[test]
fn var_use_count_str_concat_two_uses() {
    let term = str_concat(tvar("x"), tvar("x"));
    assert_eq!(term.var_use_count("x"), 2);
}

#[test]
fn var_use_count_let_shadows() {
    // let x = "hi" in x ++ x → 0 uses of outer x in body
    let term = let_str(
        "x",
        Term::StringLit("hi".to_string()),
        str_concat(tvar("x"), tvar("x")),
    );
    assert_eq!(term.var_use_count("x"), 0);
}

#[test]
fn var_use_count_let_no_shadow() {
    // let y = x in y ++ x → x free 2 times (1 in def, 1 in body)
    let body = str_concat(tvar("y"), tvar("x"));
    let term = let_str("y", tvar("x"), body.clone());
    assert_eq!(term.var_use_count("x"), 2);
    // y is bound by the let — not a free variable of the whole expression
    assert_eq!(term.var_use_count("y"), 0);
    // but y appears once in the body sub-term (the optimization use case)
    assert_eq!(body.var_use_count("y"), 1);
}

#[test]
fn var_use_count_lambda_shadows() {
    let term = Term::Lambda(
        "x".to_string(),
        Type::String,
        Box::new(str_concat(tvar("x"), tvar("x"))),
    );
    assert_eq!(term.var_use_count("x"), 0);
}

#[test]
fn var_use_count_nested_let_single_use() {
    // let s = "a" ++ "b" in s → s is bound, so 0 free uses in whole expr
    // but body.var_use_count("s") == 1 (the optimization check)
    let body = tvar("s");
    let term = let_str(
        "s",
        str_concat(
            Term::StringLit("a".to_string()),
            Term::StringLit("b".to_string()),
        ),
        body.clone(),
    );
    assert_eq!(term.var_use_count("s"), 0);
    assert_eq!(body.var_use_count("s"), 1);
}

#[test]
fn var_use_count_fix_shadows() {
    // fix f:Nat. f → f is bound by fix, 0 free uses of outer f
    let term = Term::Fix("f".to_string(), Type::Nat, Box::new(tvar("f")));
    assert_eq!(term.var_use_count("f"), 0);
}

#[test]
fn var_use_count_adt_match_arm_shadows() {
    // match scrut { (0, x, x ++ x) } with outer x
    // arm binds x → shadow, so outer x not counted in arm body
    let arm_body = str_concat(tvar("x"), tvar("x"));
    let term = Term::AdtMatch(
        Box::new(tvar("x")),
        vec![(0, "x".to_string(), Box::new(arm_body))],
    );
    // scrutinee has 1 use of x, arm body is shadowed → total 1
    assert_eq!(term.var_use_count("x"), 1);
}

#[test]
fn var_use_count_deeply_nested_lets() {
    // let a = x in let b = a ++ a in let c = b ++ x in c
    // x appears: 1 in a's def + 1 in c's def = 2
    let inner = let_str("c", str_concat(tvar("b"), tvar("x")), tvar("c"));
    let mid = let_str("b", str_concat(tvar("a"), tvar("a")), inner);
    let outer = let_str("a", tvar("x"), mid);
    assert_eq!(outer.var_use_count("x"), 2);
}
