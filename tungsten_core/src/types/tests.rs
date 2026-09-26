use super::*;

#[test]
fn test_type_display() {
    assert_eq!(Type::Bool.to_string(), "Bool");
    assert_eq!(
        Type::arrow(Type::Nat, Type::Bool).to_string(),
        "(Nat → Bool)"
    );
    assert_eq!(
        Type::product(Type::Bool, Type::Nat).to_string(),
        "(Bool × Nat)"
    );
    assert_eq!(
        Type::sum(Type::Unit, Type::Void).to_string(),
        "(Unit + Void)"
    );
    assert_eq!(
        Type::forall("α", Type::TyVar("α".into())).to_string(),
        "∀α. α"
    );
}

#[test]
fn test_type_substitution() {
    let ty = Type::TyVar("α".into());
    let result = ty.substitute("α", &Type::Nat);
    assert_eq!(result, Type::Nat);

    let arrow = Type::arrow(Type::TyVar("α".into()), Type::TyVar("α".into()));
    let result = arrow.substitute("α", &Type::Bool);
    assert_eq!(result, Type::arrow(Type::Bool, Type::Bool));
}

#[test]
fn test_forall_shadowing() {
    // ∀α. α should not substitute inner α
    let ty = Type::forall("α", Type::TyVar("α".into()));
    let result = ty.substitute("α", &Type::Nat);
    assert_eq!(result, Type::forall("α", Type::TyVar("α".into())));
}

#[test]
fn test_free_type_vars() {
    let ty = Type::arrow(Type::TyVar("α".into()), Type::TyVar("β".into()));
    let free = ty.free_type_vars();
    assert!(free.contains("α"));
    assert!(free.contains("β"));
    assert_eq!(free.len(), 2);

    // Forall binds α
    let ty = Type::forall(
        "α",
        Type::arrow(Type::TyVar("α".into()), Type::TyVar("β".into())),
    );
    let free = ty.free_type_vars();
    assert!(!free.contains("α"));
    assert!(free.contains("β"));
    assert_eq!(free.len(), 1);
}

// ======================================================================
// Type::substitute — base type passthrough and binder shadowing
// ======================================================================

#[test]
fn test_type_substitute_base_types_unchanged() {
    for base in &[
        Type::Bool,
        Type::Nat,
        Type::Unit,
        Type::Void,
        Type::Prop,
        Type::String,
        Type::Error,
    ] {
        assert_eq!(base.substitute("α", &Type::Nat), base.clone());
    }
}

#[test]
fn test_type_substitute_forall_shadows() {
    // ∀α. α → α — substituting α should not penetrate
    let ty = Type::forall(
        "α",
        Type::arrow(Type::TyVar("α".into()), Type::TyVar("α".into())),
    );
    let result = ty.substitute("α", &Type::Bool);
    assert_eq!(result, ty);
}

#[test]
fn test_type_substitute_forall_no_shadow() {
    // ∀α. α → β — substituting β should work
    let ty = Type::forall(
        "α",
        Type::arrow(Type::TyVar("α".into()), Type::TyVar("β".into())),
    );
    let result = ty.substitute("β", &Type::Nat);
    assert_eq!(
        result,
        Type::forall("α", Type::arrow(Type::TyVar("α".into()), Type::Nat))
    );
}

#[test]
fn test_type_substitute_mu_shadows() {
    // μα. Unit + α — substituting α should not penetrate
    let ty = Type::Mu(
        "α".into(),
        Box::new(Type::sum(Type::Unit, Type::TyVar("α".into()))),
    );
    let result = ty.substitute("α", &Type::Bool);
    assert_eq!(result, ty);
}

#[test]
fn test_type_substitute_mu_no_shadow() {
    // μα. β + α — substituting β should work
    let ty = Type::Mu(
        "α".into(),
        Box::new(Type::sum(Type::TyVar("β".into()), Type::TyVar("α".into()))),
    );
    let result = ty.substitute("β", &Type::Nat);
    assert_eq!(
        result,
        Type::Mu(
            "α".into(),
            Box::new(Type::sum(Type::Nat, Type::TyVar("α".into())))
        )
    );
}

// ======================================================================
// Type::reconstruct_* helpers
// ======================================================================

#[test]
fn test_reconstruct_binary_arrow() {
    let template = Type::arrow(Type::Unit, Type::Unit);
    let result = Type::reconstruct_binary(&template, Type::Nat, Type::Bool);
    assert_eq!(result, Type::arrow(Type::Nat, Type::Bool));
}

#[test]
fn test_reconstruct_binary_product() {
    let template = Type::product(Type::Unit, Type::Unit);
    let result = Type::reconstruct_binary(&template, Type::Nat, Type::Bool);
    assert_eq!(result, Type::product(Type::Nat, Type::Bool));
}

#[test]
fn test_reconstruct_binary_sum() {
    let template = Type::sum(Type::Unit, Type::Unit);
    let result = Type::reconstruct_binary(&template, Type::Nat, Type::Bool);
    assert_eq!(result, Type::sum(Type::Nat, Type::Bool));
}

#[test]
fn test_reconstruct_binding_forall() {
    let template = Type::forall("x", Type::Unit);
    let result = Type::reconstruct_binding(&template, "α", Type::Nat);
    assert_eq!(result, Type::forall("α", Type::Nat));
}

#[test]
fn test_reconstruct_binding_mu() {
    let template = Type::mu("x", Type::Unit);
    let result = Type::reconstruct_binding(&template, "α", Type::Nat);
    assert_eq!(result, Type::mu("α", Type::Nat));
}

#[test]
fn test_reconstruct_wrapper_ptr() {
    let template = Type::ptr(Type::Unit);
    let result = Type::reconstruct_wrapper(&template, Type::Nat);
    assert_eq!(result, Type::ptr(Type::Nat));
}

#[test]
fn test_reconstruct_wrapper_ref() {
    let template = Type::ref_ty(Type::Unit);
    let result = Type::reconstruct_wrapper(&template, Type::Nat);
    assert_eq!(result, Type::ref_ty(Type::Nat));
}

#[test]
#[should_panic(expected = "reconstruct_binary called on non-binary type")]
fn test_reconstruct_binary_panics_on_non_binary() {
    Type::reconstruct_binary(&Type::Nat, Type::Unit, Type::Unit);
}

#[test]
#[should_panic(expected = "reconstruct_binding called on non-binding type")]
fn test_reconstruct_binding_panics_on_non_binding() {
    Type::reconstruct_binding(&Type::Nat, "x", Type::Unit);
}

#[test]
#[should_panic(expected = "reconstruct_wrapper called on non-wrapper type")]
fn test_reconstruct_wrapper_panics_on_non_wrapper() {
    Type::reconstruct_wrapper(&Type::Nat, Type::Unit);
}

// ======================================================================
// Type::children / Type::map_children — structural traversal helpers
// ======================================================================

#[test]
fn test_children_terminals_are_empty() {
    for terminal in [
        Type::Bool,
        Type::Nat,
        Type::Unit,
        Type::Void,
        Type::Prop,
        Type::String,
        Type::TyVar("α".into()),
        Type::Error,
    ] {
        assert!(
            terminal.children().is_empty(),
            "{terminal:?} should have no child types"
        );
    }
}

#[test]
fn test_children_structural_order() {
    let binary = Type::product(Type::Nat, Type::Bool);
    assert_eq!(binary.children(), vec![&Type::Nat, &Type::Bool]);

    let wrapper = Type::ptr(Type::Nat);
    assert_eq!(wrapper.children(), vec![&Type::Nat]);

    let binding = Type::mu("α", Type::Bool);
    assert_eq!(binding.children(), vec![&Type::Bool]);

    let app = Type::app("F", vec![Type::Nat, Type::Bool]);
    assert_eq!(app.children(), vec![&Type::Nat, &Type::Bool]);
}

#[test]
fn test_children_adt_yields_type_args_then_payloads() {
    let adt = Type::adt(
        "T",
        vec![Type::Nat],
        vec![("A".into(), Type::Bool), ("B".into(), Type::Unit)],
    );
    // type_args first, then variant payloads — names are not children.
    assert_eq!(adt.children(), vec![&Type::Nat, &Type::Bool, &Type::Unit]);
}

#[test]
fn test_children_eq_yields_only_the_type_arg_not_the_terms() {
    // Eq(τ, t₁, t₂): only the carried type τ is a child; the witness Terms
    // are not `Type`s and must not appear.
    let eq = Type::eq(
        Type::Nat,
        crate::terms::Term::Zero,
        crate::terms::Term::Zero,
    );
    assert_eq!(eq.children(), vec![&Type::Nat]);
}

#[test]
fn test_map_children_is_shallow_and_preserves_shape() {
    // f only ever sees the DIRECT children, so mapping Nat→String rewrites the
    // top layer's Nats but does not recurse on its own.
    let ty = Type::product(Type::Nat, Type::sum(Type::Nat, Type::Bool));
    let mapped = ty.map_children(|child| match child {
        Type::Nat => Type::String,
        other => other.clone(),
    });
    // Outer-left Nat rewritten; the inner Sum is a child (rewritten as a whole
    // only if it matched — it didn't), so its inner Nat is untouched.
    assert_eq!(
        mapped,
        Type::product(Type::String, Type::sum(Type::Nat, Type::Bool))
    );
}

#[test]
fn test_map_children_identity_roundtrips_every_variant() {
    let samples = [
        Type::arrow(Type::Nat, Type::Bool),
        Type::product(Type::Nat, Type::Bool),
        Type::sum(Type::Nat, Type::Bool),
        Type::forall("α", Type::TyVar("α".into())),
        Type::mu("α", Type::TyVar("α".into())),
        Type::ptr(Type::Nat),
        Type::ref_ty(Type::Nat),
        Type::eq(
            Type::Nat,
            crate::terms::Term::Zero,
            crate::terms::Term::Zero,
        ),
        Type::app("F", vec![Type::Nat, Type::Bool]),
        Type::adt("T", vec![Type::Nat], vec![("A".into(), Type::Bool)]),
        Type::Unit,
        Type::TyVar("x".into()),
    ];
    for ty in samples {
        assert_eq!(ty.map_children(Type::clone), ty, "identity map on {ty:?}");
    }
}

#[test]
fn test_map_children_eq_transforms_type_arg_and_preserves_terms() {
    // The Eq arm maps the carried type but must keep both witness Terms as-is.
    let eq = Type::eq(
        Type::Nat,
        crate::terms::Term::Zero,
        crate::terms::Term::True,
    );
    let mapped = eq.map_children(|_| Type::String);
    assert_eq!(
        mapped,
        Type::eq(
            Type::String,
            crate::terms::Term::Zero,
            crate::terms::Term::True
        )
    );
}

#[test]
fn test_map_children_recursive_closure_rewrites_whole_tree() {
    // A caller that recurses through map_children rewrites every Nat, at any
    // depth — the transformer-default pattern the resolvers use.
    fn rewrite(ty: &Type) -> Type {
        match ty {
            Type::Nat => Type::String,
            other => other.map_children(rewrite),
        }
    }
    let ty = Type::product(Type::Nat, Type::sum(Type::Nat, Type::Bool));
    assert_eq!(
        rewrite(&ty),
        Type::product(Type::String, Type::sum(Type::String, Type::Bool))
    );
}

// ======================================================================
// Type::node_count — count nodes in type tree
// ======================================================================

#[test]
fn test_node_count_leaf_types() {
    assert_eq!(Type::Nat.node_count(), 1);
    assert_eq!(Type::Bool.node_count(), 1);
    assert_eq!(Type::Unit.node_count(), 1);
    assert_eq!(Type::TyVar("α".into()).node_count(), 1);
    assert_eq!(Type::Error.node_count(), 1);
}

#[test]
fn test_node_count_binary() {
    // Arrow(Nat, Bool) = 3 nodes
    assert_eq!(Type::arrow(Type::Nat, Type::Bool).node_count(), 3);
    // Product(Sum(Nat, Bool), Unit) = 5 nodes
    assert_eq!(
        Type::product(Type::sum(Type::Nat, Type::Bool), Type::Unit).node_count(),
        5
    );
}

#[test]
fn test_node_count_mu() {
    // Mu(α, Sum(Unit, TyVar(α))) = 1 + 1 + 1 + 1 = 4
    let mu = Type::Mu(
        "α".into(),
        Box::new(Type::sum(Type::Unit, Type::TyVar("α".into()))),
    );
    assert_eq!(mu.node_count(), 4);
}

#[test]
fn test_node_count_nested_mu() {
    // Mu(α_A, Mu(α_B, Sum(TyVar(α_A), TyVar(α_B)))) = 5
    let ty = Type::Mu(
        "α_A".into(),
        Box::new(Type::Mu(
            "α_B".into(),
            Box::new(Type::sum(
                Type::TyVar("α_A".into()),
                Type::TyVar("α_B".into()),
            )),
        )),
    );
    assert_eq!(ty.node_count(), 5);
}

// ======================================================================
// Type::depth — max depth of type tree
// ======================================================================

#[test]
fn test_depth_leaf_types() {
    assert_eq!(Type::Nat.depth(), 1);
    assert_eq!(Type::Bool.depth(), 1);
    assert_eq!(Type::TyVar("α".into()).depth(), 1);
}

#[test]
fn test_depth_binary() {
    // Arrow(Nat, Bool) → depth 2
    assert_eq!(Type::arrow(Type::Nat, Type::Bool).depth(), 2);
}

#[test]
fn test_depth_nested() {
    // Arrow(Nat, Arrow(Bool, Unit)) → depth 3
    assert_eq!(
        Type::arrow(Type::Nat, Type::arrow(Type::Bool, Type::Unit)).depth(),
        3
    );
}

#[test]
fn test_depth_mu() {
    // Mu(α, Sum(Unit, TyVar(α))) → depth 3
    let mu = Type::Mu(
        "α".into(),
        Box::new(Type::sum(Type::Unit, Type::TyVar("α".into()))),
    );
    assert_eq!(mu.depth(), 3);
}

#[test]
fn test_depth_asymmetric_tree() {
    // Product(Nat, Arrow(Bool, Arrow(Unit, Void)))
    // Left: depth 1, Right: depth 3 → total 4
    let ty = Type::product(
        Type::Nat,
        Type::arrow(Type::Bool, Type::arrow(Type::Unit, Type::Void)),
    );
    assert_eq!(ty.depth(), 4);
}

// ── display_detailed_to_depth (ADR 21.7.26f) ────────────────────────────────
//
// `display_detailed` is unbounded, which is fine for debugging but not for a
// diagnostic: the stored encodings it is most useful on are the large ones.

#[test]
fn detailed_depth_zero_elides_everything() {
    assert_eq!(Type::Nat.display_detailed_to_depth(0), "…");
}

#[test]
fn detailed_depth_renders_terminals_in_full() {
    assert_eq!(Type::Nat.display_detailed_to_depth(1), "Nat");
    assert_eq!(Type::Bool.display_detailed_to_depth(4), "Bool");
}

#[test]
fn detailed_depth_elides_below_the_cut_off() {
    // Product(Nat, Bool) at depth 1: the constructor is visible, children are not.
    let ty = Type::product(Type::Nat, Type::Bool);
    assert_eq!(ty.display_detailed_to_depth(1), "Product(…, …)");
    assert_eq!(ty.display_detailed_to_depth(2), "Product(Nat, Bool)");
}

#[test]
fn detailed_depth_elides_inside_app_and_adt_children() {
    let inner = Type::product(Type::Nat, Type::Bool);
    let app = Type::App("Box".to_string(), vec![inner.clone()]);
    assert_eq!(
        app.display_detailed_to_depth(2),
        "App(Box, [Product(…, …)])"
    );

    let adt = Type::Adt("Wrap".to_string(), vec![], vec![("W".to_string(), inner)]);
    assert_eq!(
        adt.display_detailed_to_depth(2),
        "Adt(Wrap, [], [(W, Product(…, …))])"
    );
}

#[test]
fn detailed_depth_bounds_output_for_a_deep_type() {
    // A chain far deeper than the cut-off must not render in full.
    let deep = (0..50).fold(Type::Nat, |acc, _| Type::Ptr(Box::new(acc)));
    let bounded = deep.display_detailed_to_depth(3);
    assert_eq!(bounded, "Ptr(Ptr(Ptr(…)))");
    assert!(deep.display_detailed().len() > bounded.len() * 10);
}

#[test]
fn display_detailed_is_the_unbounded_case_of_display_detailed_to_depth() {
    // The delegation must be behaviour-preserving — every existing caller of
    // `display_detailed` relies on it.
    let ty = Type::Adt(
        "Result".to_string(),
        vec![Type::Nat],
        vec![
            ("Ok".to_string(), Type::product(Type::Nat, Type::Bool)),
            ("Err".to_string(), Type::String),
        ],
    );
    assert_eq!(
        ty.display_detailed(),
        ty.display_detailed_to_depth(usize::MAX)
    );
}
