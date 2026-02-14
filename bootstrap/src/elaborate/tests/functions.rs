//! Tests for basic function elaboration and polymorphic/generic functions.

use super::{elab_err, elab_ok};
use tungsten_core::{Term, Type};

// ─────────────────────────────────────────────────────────────────────────────
// Basic function elaboration
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_elaborate_identity() {
    let defs = elab_ok("fn id(x: Nat) -> Nat { x }");
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "id");
    assert_eq!(defs[0].ty, Type::arrow(Type::Nat, Type::Nat));

    // Should produce: λ (x: Nat). x
    assert!(matches!(defs[0].term, Term::Lambda(_, Type::Nat, _)));
}

#[test]
fn test_elaborate_constant() {
    let defs = elab_ok("fn zero() -> Nat { 0 }");
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "zero");
    assert_eq!(defs[0].ty, Type::Nat);

    // Body should be Zero
    assert_eq!(defs[0].term, Term::Zero);
}

#[test]
fn test_elaborate_bool_literal() {
    let defs = elab_ok("fn yes() -> Bool { true }");
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].ty, Type::Bool);
    assert_eq!(defs[0].term, Term::True);
}

#[test]
fn test_elaborate_two_params() {
    let defs = elab_ok("fn first(x: Nat, y: Bool) -> Nat { x }");
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "first");
    // Type: Nat → Bool → Nat
    assert_eq!(
        defs[0].ty,
        Type::arrow(Type::Nat, Type::arrow(Type::Bool, Type::Nat))
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Polymorphic functions
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_elaborate_polymorphic_id() {
    let defs = elab_ok("fn id<T>(x: T) -> T { x }");
    assert_eq!(defs.len(), 1);
    // Type: ∀T. T → T
    assert_eq!(
        defs[0].ty,
        Type::forall(
            "T",
            Type::arrow(Type::TyVar("T".into()), Type::TyVar("T".into()))
        )
    );

    // Term: Λ T. λ (x: T). x
    assert!(matches!(defs[0].term, Term::TyAbs(_, _)));
}

#[test]
fn test_elaborate_generic_function_instantiation() {
    // When calling id(42) without explicit type args, T should be inferred as Nat
    let defs = elab_ok(
        r#"
        fn id<T>(x: T) -> T { x }
        fn test() -> Nat { id(42) }
    "#,
    );
    assert_eq!(defs.len(), 2);
    assert_eq!(defs[1].ty, Type::Nat);
    // The body should contain a type application: id[Nat](42)
}

#[test]
fn test_elaborate_generic_function_instantiation_string() {
    // id("hello") should infer T = String
    let defs = elab_ok(
        r#"
        fn id<T>(x: T) -> T { x }
        fn test() -> String { id("hello") }
    "#,
    );
    assert_eq!(defs.len(), 2);
    assert_eq!(defs[1].ty, Type::String);
}

#[test]
fn test_elaborate_generic_function_instantiation_multiple_type_params() {
    // const<A, B>(x: A, y: B) -> A should infer A and B separately
    let defs = elab_ok(
        r#"
        fn const_fn<A, B>(x: A, y: B) -> A { x }
        fn test() -> Nat { const_fn(42, "hello") }
    "#,
    );
    assert_eq!(defs.len(), 2);
    assert_eq!(defs[1].ty, Type::Nat);
}

#[test]
fn test_elaborate_generic_function_with_adt_param_concrete_arg() {
    // Gap #2 Test A: is_some(Some(42)) - concrete argument provides T=Nat
    // Some(42) infers to Option<Nat>, so T should be bound to Nat
    let defs = elab_ok(
        r#"
        type Option<T> = None | Some(T)
        
        fn is_some<T>(opt: Option<T>) -> Bool {
            match opt {
                None() => false,
                Some(_) => true,
            }
        }
        
        fn test() -> Bool { is_some(Some(42)) }
    "#,
    );
    assert_eq!(defs.len(), 2);
    assert_eq!(defs[1].ty, Type::Bool);
}

#[test]
fn test_elaborate_generic_function_with_adt_param_nullary_arg() {
    // Gap #2 Test B: is_some(None) - nullary constructor needs bidirectional propagation
    // None alone can't infer T, but the expected param type Option<T> should guide it
    let defs = elab_ok(
        r#"
        type Option<T> = None | Some(T)
        
        fn is_some<T>(opt: Option<T>) -> Bool {
            match opt {
                None() => false,
                Some(_) => true,
            }
        }
        
        fn test() -> Bool { is_some(None) }
    "#,
    );
    assert_eq!(defs.len(), 2);
    assert_eq!(defs[1].ty, Type::Bool);
}

#[test]
fn test_elaborate_generic_function_definition_with_adt() {
    // Gap #2 Test C: Just defining fn is_some<T>(opt: Option<T>) -> Bool
    // The function itself should elaborate correctly even with abstract T
    let defs = elab_ok(
        r#"
        type Option<T> = None | Some(T)
        
        fn is_some<T>(opt: Option<T>) -> Bool {
            match opt {
                None() => false,
                Some(_) => true,
            }
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "is_some");
}

#[test]
fn test_elaborate_generic_function_return_type_only_variable() {
    // Gap #2 Test D: Type variable appears ONLY in return type - this should fail
    // fn get_default<T>() -> T has no way to infer T from arguments
    let errors = elab_err(
        r#"
        fn get_default<T>() -> T {
            // Can't implement without knowing T
            get_default()  // recursive call also can't help
        }
        
        fn test() -> Nat { get_default() }
    "#,
    );
    // Should fail because T cannot be inferred from call site
    assert!(!errors.is_empty());
}

// ─────────────────────────────────────────────────────────────────────────────
// Multiple definitions
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_elaborate_multiple_functions() {
    let defs = elab_ok(
        r#"
        fn zero() -> Nat { 0 }
        fn one() -> Nat { 1 }
        fn two() -> Nat { 2 }
    "#,
    );
    assert_eq!(defs.len(), 3);
    assert_eq!(defs[0].name, "zero");
    assert_eq!(defs[1].name, "one");
    assert_eq!(defs[2].name, "two");
}

#[test]
fn test_elaborate_forward_reference() {
    // Functions can reference each other (forward references)
    let defs = elab_ok(
        r#"
        fn a() -> Nat { b() }
        fn b() -> Nat { 0 }
    "#,
    );
    assert_eq!(defs.len(), 2);
}
