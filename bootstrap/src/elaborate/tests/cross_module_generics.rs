//! Tests for cross-module generic type resolution (ADR 31).
//!
//! These tests verify that the E9999 "expected sum type" fix works correctly:
//! - Type::App references are normalized before pattern matching
//! - Type aliases as generic arguments work correctly
//! - Recursive and non-recursive ADTs are handled uniformly
//!
//! Note: True cross-module tests are in tests/golden/check/cross_module_*.
//! These unit tests focus on the elaboration mechanics that make those work.

use super::elab_ok;

// ─────────────────────────────────────────────────────────────────────────────
// ADR 31: Non-recursive ADT pattern matching
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_option_string_pattern_match() {
    // This was the core E9999 failure in driver/cli.tg
    // Option is non-recursive, so unfold_scrutinee_type must normalize
    // even when is_recursive=false
    let defs = elab_ok(
        r#"
        type Option<T> = None | Some(T)
        
        fn test_option(opt: Option<String>) -> Nat {
            match opt {
                Some(s) => 1,
                None() => 0
            }
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "test_option");
}

#[test]
fn test_result_generic_pattern_match() {
    // Two-parameter generic non-recursive ADT
    let defs = elab_ok(
        r#"
        type Result<T, E> = Ok(T) | Err(E)
        
        fn unwrap_result(res: Result<Nat, String>) -> Nat {
            match res {
                Ok(n) => n,
                Err(msg) => 0
            }
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "unwrap_result");
}

// ─────────────────────────────────────────────────────────────────────────────
// ADR 31: Type alias as generic argument
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_list_with_tuple_alias() {
    // This was the core E9999 failure in source_map.tg
    // List<SourceEntry> where SourceEntry = (String, String)
    let defs = elab_ok(
        r#"
        type SourceEntry = (String, String)
        type List<T> = Nil | Cons(T, List<T>)
        
        fn count_entries(entries: List<SourceEntry>) -> Nat {
            match entries {
                Cons(entry, rest) => 1,
                Nil() => 0
            }
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "count_entries");
}

#[test]
fn test_option_with_alias_argument() {
    // Non-recursive ADT with type alias argument
    let defs = elab_ok(
        r#"
        type Point = (Nat, Nat)
        type Option<T> = None | Some(T)
        
        fn get_x(opt: Option<Point>) -> Nat {
            match opt {
                Some(p) => 0,  // Would destructure p.0 in full implementation
                None() => 0
            }
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "get_x");
}

// ─────────────────────────────────────────────────────────────────────────────
// ADR 31: Nested generics
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_option_of_option_pattern_match() {
    // Option<Option<T>> — nested non-recursive ADTs
    let defs = elab_ok(
        r#"
        type Option<T> = None | Some(T)
        
        fn flatten(opt: Option<Option<Nat>>) -> Option<Nat> {
            match opt {
                Some(inner) => inner,
                None() => None
            }
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "flatten");
}

#[test]
fn test_list_of_option_pattern_match() {
    // List<Option<T>> — recursive ADT containing non-recursive ADT
    let defs = elab_ok(
        r#"
        type Option<T> = None | Some(T)
        type List<T> = Nil | Cons(T, List<T>)
        
        fn count_some(xs: List<Option<Nat>>) -> Nat {
            match xs {
                Cons(head, tail) => 1,
                Nil() => 0
            }
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "count_some");
}

#[test]
fn test_result_with_list_error() {
    // Result<T, List<E>> — non-recursive containing recursive
    let defs = elab_ok(
        r#"
        type List<T> = Nil | Cons(T, List<T>)
        type Result<T, E> = Ok(T) | Err(E)
        
        fn process(res: Result<Nat, List<String>>) -> Nat {
            match res {
                Ok(n) => n,
                Err(errors) => 0
            }
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "process");
}

// ─────────────────────────────────────────────────────────────────────────────
// ADR 31: Pattern matching preserves binding types
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_pattern_binding_has_correct_type() {
    // Verify that pattern-bound variable has the correct type
    // and can be used in nested operations
    let defs = elab_ok(
        r#"
        type Option<T> = None | Some(T)
        
        fn use_binding(opt: Option<Nat>) -> Nat {
            match opt {
                Some(n) => n + 1,  // n should have type Nat
                None() => 0
            }
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "use_binding");
}

#[test]
fn test_pattern_binding_with_alias_type() {
    // When matching List<Alias>, the binding should have type Alias
    let defs = elab_ok(
        r#"
        type Entry = (String, Nat)
        type List<T> = Nil | Cons(T, List<T>)
        
        fn get_first_entry(xs: List<Entry>) -> Entry {
            match xs {
                Cons(entry, rest) => entry,  // entry should have type Entry
                Nil() => ("default", 0)
            }
        }
    "#,
    );
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "get_first_entry");
}
