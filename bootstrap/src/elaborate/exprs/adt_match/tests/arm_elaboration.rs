//! Tests for arm elaboration (scoped binding, validation, product destructuring).

use crate::ast::{self, Ident, Path, Pattern};
use crate::elaborate::env as elab_env;
use crate::elaborate::{ElabResult, Elaborator};
use tungsten_core::{Context, Term, Type};

/// Create an Elaborator for testing.
fn make_elaborator() -> Elaborator<'static> {
    let ctx = Box::leak(Box::new(Context::new()));
    Elaborator::new(ctx)
}

/// Create a simple path from a string.
fn simple_path(name: &str, span: crate::span::Span) -> Path {
    Path {
        segments: vec![Ident::new(name, span)],
        span,
    }
}

// ========================================================================
// Tests for with_scoped_binding helper
// ========================================================================

/// Test that with_scoped_binding properly manages depth.
#[test]
fn test_with_scoped_binding_depth_management() {
    let mut elab = make_elaborator();
    let initial_depth = elab.depth;

    let result: ElabResult<usize> = elab.with_scoped_binding("x", Type::Nat, |e| {
        // Inside the closure, depth should be incremented
        Ok(e.depth)
    });

    assert!(result.is_ok());
    let inner_depth = result.unwrap();
    assert_eq!(inner_depth, initial_depth + 1);

    // After returning, depth should be restored
    assert_eq!(elab.depth, initial_depth);
}

/// Test that with_scoped_binding handles errors correctly.
#[test]
fn test_with_scoped_binding_error_handling() {
    use crate::elaborate::error::{ElabError, ElabErrorKind};
    use crate::span::Span;

    let mut elab = make_elaborator();
    let initial_depth = elab.depth;

    let result: ElabResult<()> = elab.with_scoped_binding("x", Type::Nat, |_| {
        Err(ElabError::new(
            Span::new(0, 0),
            ElabErrorKind::Other("test error".to_string()),
        ))
    });

    assert!(result.is_err());

    // Depth should still be restored even on error
    assert_eq!(elab.depth, initial_depth);
}

// ========================================================================
// Tests for validate_ctor_arm
// ========================================================================

/// Test validate_ctor_arm passes with correct arity and no guard.
#[test]
fn test_validate_ctor_arm_success() {
    use crate::span::Span;

    let elab = make_elaborator();
    let span = Span::new(0, 0);

    let constructor = elab_env::Constructor {
        name: "Some".to_string(),
        fields: vec![Type::Nat],
        index: 0,
        span,
        visibility: None,
    };

    let arm = ast::MatchArm {
        pattern: Pattern::Constructor(
            simple_path("Some", span),
            vec![Pattern::Wildcard(span)],
            span,
        ),
        guard: None,
        body: ast::Expr::Unit(span),
        span,
    };

    let result = elab.validate_ctor_arm(&arm, &constructor, 1);
    assert!(result.is_ok());
}

/// Test validate_ctor_arm fails with guard.
#[test]
fn test_validate_ctor_arm_rejects_guard() {
    use crate::span::Span;

    let elab = make_elaborator();
    let span = Span::new(0, 0);

    let constructor = elab_env::Constructor {
        name: "Some".to_string(),
        fields: vec![Type::Nat],
        index: 0,
        span,
        visibility: None,
    };

    let arm = ast::MatchArm {
        pattern: Pattern::Constructor(
            simple_path("Some", span),
            vec![Pattern::Wildcard(span)],
            span,
        ),
        guard: Some(ast::Expr::BoolLiteral(true, span)),
        body: ast::Expr::Unit(span),
        span,
    };

    let result = elab.validate_ctor_arm(&arm, &constructor, 1);
    assert!(result.is_err());
}

/// Test validate_ctor_arm fails with wrong arity.
#[test]
fn test_validate_ctor_arm_wrong_arity() {
    use crate::span::Span;

    let elab = make_elaborator();
    let span = Span::new(0, 0);

    let constructor = elab_env::Constructor {
        name: "Some".to_string(),
        fields: vec![Type::Nat],
        index: 0,
        span,
        visibility: None,
    };

    let arm = ast::MatchArm {
        pattern: Pattern::Constructor(
            simple_path("Some", span),
            vec![], // 0 patterns but constructor expects 1
            span,
        ),
        guard: None,
        body: ast::Expr::Unit(span),
        span,
    };

    let result = elab.validate_ctor_arm(&arm, &constructor, 0);
    assert!(result.is_err());
}

// ========================================================================
// Tests for product field accessors (wrap_product_destructs)
// ========================================================================

/// Test wrap_product_destructs generates correct accessors for 2 fields.
#[test]
fn test_wrap_product_destructs_two_fields() {
    use crate::span::Span;

    let mut elab = make_elaborator();
    let span = Span::new(0, 0);

    // Simulate binding 2 patterns
    elab.depth = 2;

    let patterns = vec![
        Pattern::Var(Ident::new("a", span)),
        Pattern::Var(Ident::new("b", span)),
    ];
    let field_types = vec![Type::Nat, Type::String];
    let body = Term::var("result");

    let result = elab.wrap_product_destructs(body.clone(), &patterns, &field_types, "raw");
    assert!(result.is_ok());

    let term = result.unwrap();
    if let Term::Let(var, ty, val, inner) = &term {
        assert_eq!(var.as_str(), "a");
        assert_eq!(ty, &Type::Nat);
        if let Term::Fst(inner_val) = val.as_ref() {
            if let Term::Var(v) = inner_val.as_ref() {
                assert_eq!(v.as_str(), "raw");
            } else {
                panic!("Expected Var in fst");
            }
        } else {
            panic!("Expected Fst for first field");
        }

        if let Term::Let(var2, ty2, val2, _) = inner.as_ref() {
            assert_eq!(var2.as_str(), "b");
            assert_eq!(ty2, &Type::String);
            if let Term::Snd(inner_val2) = val2.as_ref() {
                if let Term::Var(v2) = inner_val2.as_ref() {
                    assert_eq!(v2.as_str(), "raw");
                }
            } else {
                panic!("Expected Snd for second field");
            }
        }
    } else {
        panic!("Expected Let term");
    }
}

/// Test wrap_product_destructs handles wildcards (skips them).
#[test]
fn test_wrap_product_destructs_with_wildcards() {
    use crate::span::Span;

    let mut elab = make_elaborator();
    let span = Span::new(0, 0);

    // Simulate binding 2 patterns
    elab.depth = 2;

    let patterns = vec![
        Pattern::Wildcard(span), // Should be skipped
        Pattern::Var(Ident::new("b", span)),
    ];
    let field_types = vec![Type::Nat, Type::String];
    let body = Term::var("result");

    let result = elab.wrap_product_destructs(body.clone(), &patterns, &field_types, "raw");
    assert!(result.is_ok());

    let term = result.unwrap();
    if let Term::Let(var, _, _, _) = &term {
        assert_eq!(var.as_str(), "b");
    } else {
        panic!("Expected single Let term for non-wildcard");
    }
}

/// Test wrap_product_destructs generates correct accessors for 3 fields.
#[test]
fn test_wrap_product_destructs_three_fields() {
    use crate::span::Span;

    let mut elab = make_elaborator();
    let span = Span::new(0, 0);

    // Simulate binding 3 patterns
    elab.depth = 3;

    let patterns = vec![
        Pattern::Var(Ident::new("a", span)),
        Pattern::Var(Ident::new("b", span)),
        Pattern::Var(Ident::new("c", span)),
    ];
    let field_types = vec![Type::Nat, Type::String, Type::Bool];
    let body = Term::var("result");

    let result = elab.wrap_product_destructs(body.clone(), &patterns, &field_types, "raw");
    assert!(result.is_ok());

    // Depth should be decremented by 3
    assert_eq!(elab.depth, 0);
}
