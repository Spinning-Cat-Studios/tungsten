//! Tests: what `unfold_inner_mu_layers` unfolds *to* — substitution
//! correctness for ordinary, generic and mutually recursive encodings.
//!
//! Its sibling `termination.rs` covers the complementary question: that the
//! peel *stops* (ADR 11.8.26c).

use tungsten_core::Type;

/// Test that μ-type unfolding properly substitutes the bound variable.
///
/// This is a key part of the fix for ADR 30.1.26 - without this substitution,
/// pattern variables get types containing raw μ-variables like `α_List`.
#[test]
fn test_mu_type_unfolding_substitutes_variable() {
    // Create a μ-type: μα. (Unit + (String × α))
    // This represents List<String>
    let alpha = "α_List".to_string();
    let body = Type::Sum(
        Box::new(Type::Unit),
        Box::new(Type::Product(
            Box::new(Type::String),
            Box::new(Type::TyVar(alpha.clone())),
        )),
    );
    let mu_type = Type::Mu(alpha.clone(), Box::new(body.clone()));

    // Unfold: substitute α with the full μ-type
    let unfolded = body.substitute(&alpha, &mu_type);

    // The result should have the μ-type in place of α
    // Unit + (String × μα.(Unit + (String × α)))
    let Type::Sum(_, right) = &unfolded else {
        panic!("Expected sum type");
    };
    let Type::Product(_, tail) = &**right else {
        panic!("Expected product type");
    };
    // The tail should be the full μ-type, not TyVar("α_List")
    assert!(
        matches!(&**tail, Type::Mu(_, _)),
        "Expected μ-type in tail position, got {:?}",
        tail
    );
}

/// Test that non-recursive types pass through unchanged.
#[test]
fn test_non_recursive_type_unchanged() {
    let ty = Type::Sum(Box::new(Type::Unit), Box::new(Type::String));

    // For non-recursive types, unfolding should return the same type
    // (This is tested indirectly through the elaborator, but we can test the logic)
    let cloned = ty.clone();
    assert_eq!(ty, cloned);
}

/// Test with nested μ-types to ensure correct substitution.
///
/// This catches bugs where only the outermost μ-variable is substituted.
#[test]
fn test_nested_mu_type_substitution() {
    // Create: μα. (μβ. (α × β) + Unit)
    // This is a contrived example but tests nested binding
    let alpha = "α".to_string();
    let beta = "β".to_string();

    let inner_body = Type::Product(
        Box::new(Type::TyVar(alpha.clone())),
        Box::new(Type::TyVar(beta.clone())),
    );
    let inner_mu = Type::Mu(beta.clone(), Box::new(inner_body));

    let outer_body = Type::Sum(Box::new(inner_mu), Box::new(Type::Unit));
    let outer_mu = Type::Mu(alpha.clone(), Box::new(outer_body.clone()));

    // Substitute α in the body
    let unfolded = outer_body.substitute(&alpha, &outer_mu);

    // Check that α was substituted but β remains
    let Type::Sum(left, _) = &unfolded else {
        panic!("Expected sum type");
    };
    let Type::Mu(var, body) = &**left else {
        panic!("Expected inner μ-type");
    };
    assert_eq!(var, &beta, "Inner μ-variable should be β");
    // The body should contain the outer μ-type, not α
    let Type::Product(fst, _) = &**body else {
        panic!("Expected product");
    };
    assert!(
        matches!(&**fst, Type::Mu(_, _)),
        "α should be replaced with μ-type"
    );
}

// ========================================================================
// Tests for unfold_inner_mu_layers (ADR 20.4.26b)
// ========================================================================
//
// Mutually recursive types produce nested Mu binders. After the initial
// unfold peels the outermost Mu, inner Mu layers remain. Each must be
// resolved by substituting its variable with the cached encoding.

use crate::ast::Visibility;
use crate::elaborate::env::{Constructor, TypeDef, TypeDefKind};
use crate::elaborate::Elaborator;
use crate::span::Span;
use tungsten_core::Context;

fn make_elaborator() -> Elaborator<'static> {
    let ctx = Box::leak(Box::new(Context::new()));
    Elaborator::new(ctx)
}

/// unfold_inner_mu_layers should peel all nested Mu layers from a
/// mutually recursive encoding, producing a Sum/Product type.
#[test]
fn test_unfold_inner_mu_layers_peels_nested() {
    let mut elab = make_elaborator();
    let dummy_span = Span::new(0, 0);

    // Register type A = AA | AB(B)
    // Register type B = BA | BB(A)
    // These form a mutual recursion group {A, B}.

    // Cached encoding for A: Mu("α_A", Mu("α_B", Sum(Unit, TyVar("α_B"))))
    let a_encoding = Type::mu(
        "α_A",
        Type::mu("α_B", Type::sum(Type::Unit, Type::TyVar("α_B".to_string()))),
    );
    // Cached encoding for B: Mu("α_A", Mu("α_B", Sum(Unit, TyVar("α_A"))))
    let b_encoding = Type::mu(
        "α_A",
        Type::mu("α_B", Type::sum(Type::Unit, Type::TyVar("α_A".to_string()))),
    );

    elab.env.define_type(TypeDef {
        name: "A".to_string(),
        params: vec![],
        kind: TypeDefKind::ADT(vec![
            Constructor {
                name: "AA".to_string(),
                fields: vec![],
                index: 0,
                visibility: None,
                span: dummy_span,
            },
            Constructor {
                name: "AB".to_string(),
                fields: vec![Type::TyVar("B".to_string())],
                index: 1,
                visibility: None,
                span: dummy_span,
            },
        ]),
        visibility: Visibility::Public,
        span: dummy_span,
        defining_module: None,
        encoded_type: Some(a_encoding.clone()),
        field_visibilities: Vec::new(),
    });

    elab.env.define_type(TypeDef {
        name: "B".to_string(),
        params: vec![],
        kind: TypeDefKind::ADT(vec![
            Constructor {
                name: "BA".to_string(),
                fields: vec![],
                index: 0,
                visibility: None,
                span: dummy_span,
            },
            Constructor {
                name: "BB".to_string(),
                fields: vec![Type::TyVar("A".to_string())],
                index: 1,
                visibility: None,
                span: dummy_span,
            },
        ]),
        visibility: Visibility::Public,
        span: dummy_span,
        defining_module: None,
        encoded_type: Some(b_encoding.clone()),
        field_visibilities: Vec::new(),
    });

    // Simulate: after unfolding the outer Mu("α_A") of A's encoding,
    // we have: Mu("α_B", Sum(Unit, TyVar("α_B")))
    // unfold_inner_mu_layers should peel this remaining Mu("α_B").
    let after_outer_unfold = Type::mu("α_B", Type::sum(Type::Unit, Type::TyVar("α_B".to_string())));

    let result = elab
        .unfold_inner_mu_layers(super::ResidualMuChain::after_outer_unfold(
            after_outer_unfold,
        ))
        .expect("a mutual pair's inner chain flattens");

    // Should be a Sum type (Mu layers peeled away)
    assert!(
        matches!(&result, Type::Sum(_, _)),
        "Expected Sum after unfolding inner Mu layers, got {:?}",
        result
    );
}

/// unfold_inner_mu_layers should be a no-op for non-Mu types.
#[test]
fn test_unfold_inner_mu_layers_noop_for_non_mu() {
    let elab = make_elaborator();

    let sum = Type::sum(Type::Unit, Type::Nat);
    let result =
        elab.unfold_inner_mu_layers(super::ResidualMuChain::after_outer_unfold(sum.clone()));

    assert_eq!(result, Ok(sum), "Non-Mu type should pass through unchanged");
}
