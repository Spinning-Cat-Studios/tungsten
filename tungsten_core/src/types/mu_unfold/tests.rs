//! Tests for the shallow μ-unfold (ADR 7.7.26k), including equivalence
//! against the pre-ADR accumulated-substitution loop as a semantic oracle.

use super::unfold_mu_type;
use crate::types::Type;

fn tv(name: &str) -> Type {
    Type::TyVar(name.to_string())
}

fn mu(var: &str, body: Type) -> Type {
    Type::Mu(var.to_string(), Box::new(body))
}

fn sum(left: Type, right: Type) -> Type {
    Type::Sum(Box::new(left), Box::new(right))
}

fn product(left: Type, right: Type) -> Type {
    Type::Product(Box::new(left), Box::new(right))
}

/// The pre-ADR-7.7.26k unfold: substitute the accumulated `current` into
/// each successive binder body. Exponential on nested chains; kept here
/// only as the semantic oracle for layout-equivalence tests.
fn accumulated_unfold(ty: &Type) -> Type {
    fn subst(ty: &Type, var: &str, replacement: &Type) -> Type {
        match ty {
            Type::TyVar(v) if v == var => replacement.clone(),
            Type::TyVar(_)
            | Type::Unit
            | Type::Bool
            | Type::Nat
            | Type::Int
            | Type::String
            | Type::Void
            | Type::Prop
            | Type::Error => ty.clone(),
            Type::Arrow(a, b) => Type::Arrow(
                Box::new(subst(a, var, replacement)),
                Box::new(subst(b, var, replacement)),
            ),
            Type::Product(a, b) => Type::Product(
                Box::new(subst(a, var, replacement)),
                Box::new(subst(b, var, replacement)),
            ),
            Type::Sum(a, b) => Type::Sum(
                Box::new(subst(a, var, replacement)),
                Box::new(subst(b, var, replacement)),
            ),
            Type::Forall(v, _) | Type::Mu(v, _) if v == var => ty.clone(),
            Type::Forall(v, body) => {
                Type::Forall(v.clone(), Box::new(subst(body, var, replacement)))
            }
            Type::Mu(v, body) => Type::Mu(v.clone(), Box::new(subst(body, var, replacement))),
            Type::Eq(ty_inner, t1, t2) => Type::Eq(
                Box::new(subst(ty_inner, var, replacement)),
                t1.clone(),
                t2.clone(),
            ),
            Type::Ptr(inner) => Type::Ptr(Box::new(subst(inner, var, replacement))),
            Type::Ref(inner) => Type::Ref(Box::new(subst(inner, var, replacement))),
            Type::App(name, args) => Type::App(
                name.clone(),
                args.iter().map(|a| subst(a, var, replacement)).collect(),
            ),
            Type::Adt(name, type_args, variants) => Type::Adt(
                name.clone(),
                type_args
                    .iter()
                    .map(|a| subst(a, var, replacement))
                    .collect(),
                variants
                    .iter()
                    .map(|(vname, vty)| (vname.clone(), subst(vty, var, replacement)))
                    .collect(),
            ),
        }
    }
    let mut current = ty.clone();
    while let Type::Mu(ref var, ref body) = current {
        current = subst(body, var, &current);
    }
    current
}

/// Collapse every μ-subtree to an opaque marker, mirroring `lower_type`'s
/// `Mu` → `ptr` rule. Two unfolds with equal skeletons produce identical
/// LLVM layouts.
fn layout_skeleton(ty: &Type) -> Type {
    match ty {
        Type::Mu(_, _) => Type::Ptr(Box::new(Type::Unit)),
        Type::Arrow(a, b) => {
            Type::Arrow(Box::new(layout_skeleton(a)), Box::new(layout_skeleton(b)))
        }
        Type::Product(a, b) => {
            Type::Product(Box::new(layout_skeleton(a)), Box::new(layout_skeleton(b)))
        }
        Type::Sum(a, b) => Type::Sum(Box::new(layout_skeleton(a)), Box::new(layout_skeleton(b))),
        Type::Forall(v, body) => Type::Forall(v.clone(), Box::new(layout_skeleton(body))),
        Type::Ptr(inner) => Type::Ptr(Box::new(layout_skeleton(inner))),
        Type::Ref(inner) => Type::Ref(Box::new(layout_skeleton(inner))),
        Type::App(name, args) => {
            Type::App(name.clone(), args.iter().map(layout_skeleton).collect())
        }
        Type::Adt(name, type_args, variants) => Type::Adt(
            name.clone(),
            type_args.iter().map(layout_skeleton).collect(),
            variants
                .iter()
                .map(|(vname, vty)| (vname.clone(), layout_skeleton(vty)))
                .collect(),
        ),
        _ => ty.clone(),
    }
}

/// A two-member SCC fixture in the ADR 18.4.26i group encoding:
/// μA. μB. (A + (B × Nat)).
fn nested_scc_fixture() -> Type {
    mu("A", mu("B", sum(tv("A"), product(tv("B"), Type::Nat))))
}

#[test]
fn non_mu_types_pass_through_unchanged() {
    assert_eq!(unfold_mu_type(&Type::Nat), Type::Nat);
    let sum_ty = sum(Type::Unit, Type::Nat);
    assert_eq!(unfold_mu_type(&sum_ty), sum_ty);
    // Free type variables are untouched (no chain to substitute).
    assert_eq!(unfold_mu_type(&tv("T")), tv("T"));
}

#[test]
fn single_binder_matches_accumulated_unfold_exactly() {
    // For a single binder the two algorithms substitute the same
    // replacement, so the trees must be identical, not just layout-equal.
    let list_like = mu("X", sum(Type::Unit, product(Type::Nat, tv("X"))));
    let unfolded = unfold_mu_type(&list_like);
    assert_eq!(unfolded, accumulated_unfold(&list_like));
    assert_eq!(
        unfolded,
        sum(Type::Unit, product(Type::Nat, list_like.clone()))
    );
}

#[test]
fn oracle_agrees_on_free_vars_and_shadowed_binders() {
    // Pins the oracle's own substitution guards: a free unrelated variable
    // must NOT be replaced, and a shadowed inner binder must stop
    // substitution — in both the oracle and the shallow unfold.
    let fixture = mu("X", sum(tv("Free"), product(tv("X"), mu("X", tv("X")))));
    let unfolded = unfold_mu_type(&fixture);
    assert_eq!(unfolded, accumulated_unfold(&fixture));
    assert_eq!(
        unfolded,
        sum(tv("Free"), product(fixture.clone(), mu("X", tv("X"))))
    );
}

#[test]
fn nested_chain_replaces_every_chain_var_with_the_original() {
    let fixture = nested_scc_fixture();
    let unfolded = unfold_mu_type(&fixture);
    assert_eq!(
        unfolded,
        sum(fixture.clone(), product(fixture.clone(), Type::Nat))
    );
}

#[test]
fn nested_chain_is_layout_equivalent_to_accumulated_unfold() {
    let fixture = nested_scc_fixture();
    let shallow = unfold_mu_type(&fixture);
    let accumulated = accumulated_unfold(&fixture);
    assert_eq!(layout_skeleton(&shallow), layout_skeleton(&accumulated));

    // Re-unfolding the payload μ-types must also agree on layout: extract
    // the B-position payload from each result and unfold once more.
    let payload_of = |unfolded: &Type| match unfolded {
        Type::Sum(_, right) => match right.as_ref() {
            Type::Product(b_slot, _) => b_slot.as_ref().clone(),
            other => panic!("expected product in right summand, got {other:?}"),
        },
        other => panic!("expected sum after unfold, got {other:?}"),
    };
    let shallow_payload = unfold_mu_type(&payload_of(&shallow));
    let accumulated_payload = accumulated_unfold(&payload_of(&accumulated));
    assert_eq!(
        layout_skeleton(&shallow_payload),
        layout_skeleton(&accumulated_payload)
    );
}

#[test]
fn nested_chain_output_size_is_linear() {
    // |result| = |F| - occurrences + occurrences × |input|:
    // F = A + (B × Nat) has 5 nodes and 2 chain-var occurrences; the
    // fixture has 7 nodes. 5 - 2 + 2×7 = 17.
    let fixture = nested_scc_fixture();
    assert_eq!(unfold_mu_type(&fixture).node_count(), 17);
}

#[test]
fn shallow_unfold_is_a_fixed_point_across_reunfolds() {
    // Payload holes receive the original μ-type, so re-unfolding a payload
    // reproduces the same tree — sizes must not compound.
    let fixture = nested_scc_fixture();
    let first = unfold_mu_type(&fixture);
    let Type::Sum(a_slot, _) = &first else {
        panic!("expected sum after unfold");
    };
    assert_eq!(unfold_mu_type(a_slot), first);
}

#[test]
fn inner_mu_binder_shadows_chain_var() {
    // μX. (X + μX. X): occurrences under the inner binder stay bound to it.
    let inner = mu("X", tv("X"));
    let outer = mu("X", sum(tv("X"), inner.clone()));
    assert_eq!(unfold_mu_type(&outer), sum(outer.clone(), inner));
}

#[test]
fn inner_binder_shadows_only_its_own_var_in_a_chain() {
    // μA. μB. (A + μA. (A + B)): the embedded μA shadows A but B stays
    // substitutable inside it.
    let fixture = mu("A", mu("B", sum(tv("A"), mu("A", sum(tv("A"), tv("B"))))));
    let expected_inner = mu("A", sum(tv("A"), fixture.clone()));
    assert_eq!(
        unfold_mu_type(&fixture),
        sum(fixture.clone(), expected_inner)
    );
}

#[test]
fn forall_binder_shadows_chain_var() {
    let fixture = mu(
        "X",
        sum(tv("X"), Type::Forall("X".to_string(), Box::new(tv("X")))),
    );
    assert_eq!(
        unfold_mu_type(&fixture),
        sum(
            fixture.clone(),
            Type::Forall("X".to_string(), Box::new(tv("X")))
        )
    );
}

#[test]
fn vacuous_mu_terminates_and_returns_input() {
    // μX. X made the accumulated loop spin forever; the shallow unfold
    // returns the input itself in one pass.
    let vacuous = mu("X", tv("X"));
    assert_eq!(unfold_mu_type(&vacuous), vacuous);
}

#[test]
fn unrelated_inner_binder_does_not_block_substitution() {
    // μX. (X + μY. (Y × X)): an embedded μ over a DIFFERENT variable (the
    // common shape — e.g. an inline List μ inside an SCC body) must not
    // stop chain-var substitution inside it.
    let fixture = mu("X", sum(tv("X"), mu("Y", product(tv("Y"), tv("X")))));
    assert_eq!(
        unfold_mu_type(&fixture),
        sum(fixture.clone(), mu("Y", product(tv("Y"), fixture.clone())))
    );
}

#[test]
fn eq_type_substitutes_only_the_type_component() {
    use crate::terms::Term;
    let term = |name: &str| Box::new(Term::Var(name.to_string()));
    let fixture = mu("X", Type::Eq(Box::new(tv("X")), term("lhs"), term("rhs")));
    assert_eq!(
        unfold_mu_type(&fixture),
        Type::Eq(Box::new(fixture.clone()), term("lhs"), term("rhs"))
    );
}

#[test]
fn adt_type_args_and_variant_payloads_are_substituted() {
    let fixture = mu(
        "X",
        Type::Adt(
            "List".to_string(),
            vec![tv("X")],
            vec![
                ("Nil".to_string(), Type::Unit),
                ("Cons".to_string(), product(Type::Nat, tv("X"))),
            ],
        ),
    );
    assert_eq!(
        unfold_mu_type(&fixture),
        Type::Adt(
            "List".to_string(),
            vec![fixture.clone()],
            vec![
                ("Nil".to_string(), Type::Unit),
                ("Cons".to_string(), product(Type::Nat, fixture.clone())),
            ],
        )
    );
}

#[test]
fn arrow_ptr_ref_and_app_positions_are_substituted() {
    let fixture = mu(
        "X",
        Type::Arrow(
            Box::new(Type::Ptr(Box::new(tv("X")))),
            Box::new(Type::App(
                "Wrap".to_string(),
                vec![Type::Ref(Box::new(tv("X")))],
            )),
        ),
    );
    assert_eq!(
        unfold_mu_type(&fixture),
        Type::Arrow(
            Box::new(Type::Ptr(Box::new(fixture.clone()))),
            Box::new(Type::App(
                "Wrap".to_string(),
                vec![Type::Ref(Box::new(fixture.clone()))],
            )),
        )
    );
}
