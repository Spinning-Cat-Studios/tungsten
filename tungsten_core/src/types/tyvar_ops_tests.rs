//! Tests for `Type::any_tyvar()` and `Type::strip_tyvar_at_prefix()`.
//!
//! Extracted from `types/tests.rs` (ADR 10.5.26d) to keep file sizes under
//! the 400-line limit.

use super::*;

// ── any_tyvar tests ─────────────────────────────────────────────────

#[test]
fn test_any_tyvar_bare() {
    let ty = Type::TyVar("T".into());
    assert!(ty.any_tyvar(&|name| name == "T"));
    assert!(!ty.any_tyvar(&|name| name == "U"));
}

#[test]
fn test_any_tyvar_at_prefix_skipped() {
    let ty = Type::TyVar("@Token".into());
    // Predicate that skips @-prefixed TyVars (like type_has_free_tyvar)
    assert!(!ty.any_tyvar(&|name| !name.starts_with('@') && !name.starts_with("α_")));
}

#[test]
fn test_any_tyvar_alpha_prefix_skipped() {
    let ty = Type::TyVar("α_List".into());
    assert!(!ty.any_tyvar(&|name| !name.starts_with('@') && !name.starts_with("α_")));
}

#[test]
fn test_any_tyvar_nested_in_arrow() {
    let ty = Type::arrow(Type::Nat, Type::TyVar("T".into()));
    assert!(ty.any_tyvar(&|name| name == "T"));
    assert!(!ty.any_tyvar(&|name| name == "U"));
}

#[test]
fn test_any_tyvar_nested_in_product() {
    let ty = Type::product(Type::TyVar("A".into()), Type::TyVar("B".into()));
    assert!(ty.any_tyvar(&|name| name == "A"));
    assert!(ty.any_tyvar(&|name| name == "B"));
}

#[test]
fn test_any_tyvar_nested_in_adt() {
    let ty = Type::Adt(
        "Option".into(),
        vec![Type::TyVar("T".into())],
        vec![
            ("None".into(), Type::Unit),
            ("Some".into(), Type::TyVar("T".into())),
        ],
    );
    assert!(ty.any_tyvar(&|name| name == "T"));
    assert!(!ty.any_tyvar(&|name| name == "U"));
}

#[test]
fn test_any_tyvar_in_adt_type_args() {
    // TyVar only in type_args, not in variants — should still be found
    let ty = Type::Adt(
        "Wrapper".into(),
        vec![Type::TyVar("X".into())],
        vec![("Val".into(), Type::Nat)],
    );
    assert!(ty.any_tyvar(&|name| name == "X"));
    assert!(!ty.any_tyvar(&|name| name == "Y"));
}

#[test]
fn test_any_tyvar_terminal_types() {
    assert!(!Type::Nat.any_tyvar(&|_| true));
    assert!(!Type::Bool.any_tyvar(&|_| true));
    assert!(!Type::String.any_tyvar(&|_| true));
    assert!(!Type::Unit.any_tyvar(&|_| true));
    assert!(!Type::Void.any_tyvar(&|_| true));
    assert!(!Type::Error.any_tyvar(&|_| true));
}

#[test]
fn test_any_tyvar_mu_binding() {
    // Mu(α_List, Sum(Unit, TyVar(α_List))) — α_List should be skippable
    let ty = Type::Mu(
        "α_List".into(),
        Box::new(Type::sum(Type::Unit, Type::TyVar("α_List".into()))),
    );
    assert!(!ty.any_tyvar(&|name| !name.starts_with('@') && !name.starts_with("α_")));
    // But a predicate that matches α_ should find it
    assert!(ty.any_tyvar(&|name| name.starts_with("α_")));
}

#[test]
fn test_any_tyvar_in_eq() {
    // Eq(TyVar("X"), ...) — any_tyvar recurses into the type position of Eq
    let ty = Type::Eq(
        Box::new(Type::TyVar("X".into())),
        Box::new(crate::terms::Term::Unit),
        Box::new(crate::terms::Term::Unit),
    );
    assert!(ty.any_tyvar(&|name| name == "X"));
    assert!(!ty.any_tyvar(&|name| name == "Y"));
}

// ── strip_tyvar_at_prefix tests ─────────────────────────────────────

#[test]
fn test_strip_bare_tyvar_unchanged() {
    let ty = Type::TyVar("T".into());
    assert_eq!(ty.strip_tyvar_at_prefix(), Type::TyVar("T".into()));
}

#[test]
fn test_strip_at_prefix_tyvar() {
    let ty = Type::TyVar("@Token".into());
    assert_eq!(ty.strip_tyvar_at_prefix(), Type::TyVar("Token".into()));
}

#[test]
fn test_strip_alpha_prefix_unchanged() {
    let ty = Type::TyVar("α_List".into());
    assert_eq!(ty.strip_tyvar_at_prefix(), Type::TyVar("α_List".into()));
}

#[test]
fn test_strip_nested_compound() {
    let ty = Type::arrow(
        Type::TyVar("@Token".into()),
        Type::product(Type::TyVar("@Span".into()), Type::Nat),
    );
    let expected = Type::arrow(
        Type::TyVar("Token".into()),
        Type::product(Type::TyVar("Span".into()), Type::Nat),
    );
    assert_eq!(ty.strip_tyvar_at_prefix(), expected);
}

#[test]
fn test_strip_in_adt() {
    let ty = Type::Adt(
        "List".into(),
        vec![Type::TyVar("@Token".into())],
        vec![
            ("Nil".into(), Type::Unit),
            ("Cons".into(), Type::TyVar("@Token".into())),
        ],
    );
    let expected = Type::Adt(
        "List".into(),
        vec![Type::TyVar("Token".into())],
        vec![
            ("Nil".into(), Type::Unit),
            ("Cons".into(), Type::TyVar("Token".into())),
        ],
    );
    assert_eq!(ty.strip_tyvar_at_prefix(), expected);
}

#[test]
fn test_strip_terminals_unchanged() {
    assert_eq!(Type::Nat.strip_tyvar_at_prefix(), Type::Nat);
    assert_eq!(Type::Bool.strip_tyvar_at_prefix(), Type::Bool);
    assert_eq!(Type::Unit.strip_tyvar_at_prefix(), Type::Unit);
}

#[test]
fn test_strip_in_app() {
    let ty = Type::App("List".into(), vec![Type::TyVar("@Token".into())]);
    let expected = Type::App("List".into(), vec![Type::TyVar("Token".into())]);
    assert_eq!(ty.strip_tyvar_at_prefix(), expected);
}

#[test]
fn test_strip_in_forall_and_mu() {
    let ty = Type::Forall(
        "T".into(),
        Box::new(Type::Mu(
            "α_List".into(),
            Box::new(Type::TyVar("@Foo".into())),
        )),
    );
    let expected = Type::Forall(
        "T".into(),
        Box::new(Type::Mu(
            "α_List".into(),
            Box::new(Type::TyVar("Foo".into())),
        )),
    );
    assert_eq!(ty.strip_tyvar_at_prefix(), expected);
}

#[test]
fn test_strip_in_ptr_and_ref() {
    let ty_ptr = Type::Ptr(Box::new(Type::TyVar("@Foo".into())));
    assert_eq!(
        ty_ptr.strip_tyvar_at_prefix(),
        Type::Ptr(Box::new(Type::TyVar("Foo".into())))
    );

    let ty_ref = Type::Ref(Box::new(Type::TyVar("@Bar".into())));
    assert_eq!(
        ty_ref.strip_tyvar_at_prefix(),
        Type::Ref(Box::new(Type::TyVar("Bar".into())))
    );
}
