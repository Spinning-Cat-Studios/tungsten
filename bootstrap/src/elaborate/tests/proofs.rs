//! Tests for proof constructs: theorems, axioms, have/show.

use super::elab_ok;
use tungsten_core::{Term, Type};

// ─────────────────────────────────────────────────────────────────────────────
// Theorems
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_elaborate_theorem_sorry() {
    let defs = elab_ok(
        r#"
        theorem trivial(): Bool {
            sorry
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "trivial");
    assert_eq!(defs[0].ty, Type::Bool);
    // Sorry is a valid proof term
    assert_eq!(defs[0].term, Term::Sorry);
}

#[test]
fn test_elaborate_theorem_with_hypothesis() {
    let defs = elab_ok(
        r#"
        theorem id_bool(h: Bool): Bool {
            h
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].ty, Type::arrow(Type::Bool, Type::Bool));
}

// ─────────────────────────────────────────────────────────────────────────────
// Axioms
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_elaborate_axiom() {
    let defs = elab_ok(
        r#"
        axiom excluded_middle(p: Prop): Bool
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "excluded_middle");
    // Axiom body is sorry
    assert_eq!(
        defs[0].term,
        Term::Lambda("_".to_string(), Type::Prop, Box::new(Term::Sorry))
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Proof constructs
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_elaborate_have() {
    let defs = elab_ok(
        r#"
        fn test() -> Bool {
            have h: Bool = true;
            h
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].ty, Type::Bool);
}

#[test]
fn test_elaborate_show() {
    let defs = elab_ok(
        r#"
        fn test() -> Bool {
            show Bool { true }
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].ty, Type::Bool);
}
