//! Basic error case tests: undefined variables, types, constructors, type mismatches, etc.

use crate::elaborate::error::ElabErrorKind;
use crate::elaborate::tests::{elab_err, elab_ok, elab_ok_with_warnings, elab_with_mode};
use crate::elaborate::ElabMode;

// ─────────────────────────────────────────────────────────────────────────────
// Error cases
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_error_undefined_variable() {
    let errors = elab_err(
        r#"
        fn test() -> Nat {
            undefined_var
        }
    "#,
    );
    assert!(!errors.is_empty());
    assert!(matches!(
        errors[0].kind,
        ElabErrorKind::UndefinedVariable(_)
    ));
}

#[test]
fn test_error_undefined_variable_with_suggestion() {
    // When there's a similar variable name, suggest it
    let errors = elab_err(
        r#"
        fn test(value: Nat) -> Nat {
            valu
        }
    "#,
    );
    assert!(!errors.is_empty());
    assert!(matches!(
        errors[0].kind,
        ElabErrorKind::UndefinedVariable(_)
    ));
    // Check that help contains a suggestion
    assert!(errors[0]
        .help
        .as_ref()
        .map_or(false, |h| h.contains("value")));
}

#[test]
fn test_error_undefined_type() {
    let errors = elab_err(
        r#"
        fn test(x: UndefinedType) -> Nat {
            0
        }
    "#,
    );
    assert!(!errors.is_empty());
    assert!(matches!(errors[0].kind, ElabErrorKind::UndefinedType(_)));
}

#[test]
fn test_error_undefined_type_with_suggestion() {
    // When there's a similar type name, suggest it
    let errors = elab_err(
        r#"
        fn test() -> Boo {
            true
        }
    "#,
    );
    assert!(!errors.is_empty());
    assert!(matches!(errors[0].kind, ElabErrorKind::UndefinedType(_)));
    // Check that help contains a suggestion
    assert!(errors[0]
        .help
        .as_ref()
        .map_or(false, |h| h.contains("Bool")));
}

#[test]
fn test_error_undefined_constructor_with_suggestion() {
    // When there's a similar constructor name in a pattern, suggest it
    // Use Gren() with parens to ensure it's parsed as a constructor pattern
    let errors = elab_err(
        r#"
        enum Color { Red, Green, Blue }
        fn test(c: Color) -> Nat {
            match c {
                Gren() => 1,
                _ => 0
            }
        }
    "#,
    );
    assert!(!errors.is_empty(), "should have errors");
    assert!(
        matches!(errors[0].kind, ElabErrorKind::UndefinedConstructor(_)),
        "expected UndefinedConstructor, got: {:?}",
        errors[0].kind
    );
    // Check that help contains a suggestion
    let help = errors[0]
        .help
        .as_ref()
        .expect("should have help suggestion");
    assert!(
        help.contains("Green"),
        "should suggest Green for Gren, got: {}",
        help
    );
}

#[test]
fn test_error_type_mismatch() {
    let errors = elab_err(
        r#"
        fn test() -> Nat {
            true
        }
    "#,
    );
    assert!(!errors.is_empty());
    assert!(matches!(errors[0].kind, ElabErrorKind::TypeMismatch { .. }));
}

#[test]
fn test_error_type_mismatch_with_context() {
    // Type mismatch should include context about why the type was expected
    let errors = elab_err(
        r#"
        fn test() -> Nat {
            true
        }
    "#,
    );
    assert!(!errors.is_empty());
    assert!(matches!(errors[0].kind, ElabErrorKind::TypeMismatch { .. }));
    // Check that context is set (appears as a note with span)
    assert!(errors[0].notes.iter().any(|n| n.span.is_some()));
}

#[test]
fn test_return_elaborates_to_return_term() {
    // `return 42` in a `-> Nat` function should now succeed (ADR 13.5.26d)
    let result = elab_ok(
        r#"
        fn test() -> Nat {
            return 42
        }
    "#,
    );
    assert!(result.iter().any(|d| d.name == "test"));
}

#[test]
fn test_return_in_if_branch() {
    // `return` in one branch of an if (⊥ unifies with Nat) — ADR 13.5.26d
    let result = elab_ok(
        r#"
        fn test(x: Bool) -> Nat {
            if x { return 1 } else { 2 }
        }
    "#,
    );
    assert!(result.iter().any(|d| d.name == "test"));
}

#[test]
fn test_return_type_mismatch() {
    // `return "hello"` in a `-> Nat` function should be a type error
    let errors = elab_err(
        r#"
        fn test() -> Nat {
            return "hello"
        }
    "#,
    );
    assert!(!errors.is_empty());
}

#[test]
fn test_bare_return_in_unit_fn() {
    // bare `return` (no argument) in a `-> Unit` function should succeed — ADR 13.5.26d
    let result = elab_ok(
        r#"
        fn test() -> Unit {
            return
        }
    "#,
    );
    assert!(result.iter().any(|d| d.name == "test"));
}

#[test]
fn test_bare_return_in_non_unit_fn() {
    // bare `return` in a `-> Nat` function should be a type error — ADR 13.5.26d
    let errors = elab_err(
        r#"
        fn test() -> Nat {
            return
        }
    "#,
    );
    assert!(!errors.is_empty());
    assert!(matches!(errors[0].kind, ElabErrorKind::TypeMismatch { .. }));
}

#[test]
fn test_dead_code_after_return_warning() {
    // Code after `return` should produce a W0002 warning — ADR 13.5.26d §2.7
    let (_defs, warnings) = elab_ok_with_warnings(
        r#"
        fn test() -> Nat {
            return 42;
            100
        }
    "#,
    );
    assert!(
        warnings
            .iter()
            .any(|w| matches!(w.kind, ElabErrorKind::DeadCodeAfterReturn)),
        "expected DeadCodeAfterReturn warning, got: {:?}",
        warnings
    );
}

#[test]
fn test_return_in_closure_checks_closure_type() {
    // `return` inside a closure checks against the closure's return type (§2.3.1)
    let result = elab_ok(
        r#"
        fn apply(f: Nat -> Bool) -> Bool { f(42) }
        fn outer() -> Nat {
            apply(fn(x: Nat) => return true);
            42
        }
    "#,
    );
    assert!(result.iter().any(|d| d.name == "outer"));
}

#[test]
fn test_return_in_let_binding_with_if() {
    // ⊥ from return in else branch unifies with Nat — ADR 13.5.26d §5
    let result = elab_ok(
        r#"
        fn test() -> Nat {
            let x: Nat = if true { 1 } else { return 0 };
            x
        }
    "#,
    );
    assert!(result.iter().any(|d| d.name == "test"));
}

#[test]
fn test_return_in_nested_let_chain() {
    // Dead code after return in a let chain — ADR 13.5.26d §2.7
    let (_defs, warnings) = elab_ok_with_warnings(
        r#"
        fn test() -> Nat {
            let a = 1;
            return a;
            let b = 2;
            b
        }
    "#,
    );
    assert!(
        warnings
            .iter()
            .any(|w| matches!(w.kind, ElabErrorKind::DeadCodeAfterReturn)),
        "expected DeadCodeAfterReturn warning in let chain, got: {:?}",
        warnings
    );
}

#[test]
fn test_return_type_is_void() {
    // expect_type(return 42, "Void") — acceptance criterion: return has ⊥ type
    let result = elab_with_mode(
        r#"
        fn test() -> Nat {
            expect_type(return 42, "Void");
            0
        }
    "#,
        ElabMode::Test,
    );
    assert!(
        result.is_ok(),
        "expect_type(return 42, \"Void\") should pass: {:?}",
        result.err()
    );
}

#[test]
fn test_return_in_inferred_closure_rejects() {
    // return inside an inferred closure (no expected type) should error
    let errors = elab_err(
        r#"
        fn test() -> Nat {
            let f = |x: Nat| return x;
            42
        }
    "#,
    );
    assert!(
        !errors.is_empty(),
        "return in inferred closure should error"
    );
}

#[test]
fn test_error_duplicate_definition() {
    let errors = elab_err(
        r#"
        fn foo() -> Nat { 0 }
        fn foo() -> Nat { 1 }
    "#,
    );
    assert!(!errors.is_empty());
    assert!(matches!(
        errors[0].kind,
        ElabErrorKind::DuplicateDefinition(_)
    ));
}
