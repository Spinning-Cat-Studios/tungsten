//! Synthesis for a generic ADT instantiation (ADR 1.8.26c).
//!
//! `support_tests.rs` asserts *whether* an instantiation is comparable; this
//! file asserts *what gets built* — the def's name, the delegation, and the
//! routing into ADR 29.6.26f P4's stack-safe spine, which is the property a
//! fixture-only test cannot see (a correct-on-a-3-element-list comparator that
//! recurses natively passes every short case and overflows on real input).

use std::collections::HashMap;

use tungsten_core::Type;

use crate::comparator::context::ComparatorTypes;
use crate::comparator::discover::synth_closure_bounded;
use crate::comparator::gate::CLOSURE_CAP;
use crate::comparator::mangling::comparator_symbol;
use crate::driver::{AdtTypes, RecordTypes};
use crate::elaborate::env::Constructor;
use crate::span::Span;

use super::{list::as_cons_list, synth_comparator_defs};

fn ctor(name: &str, index: usize, fields: Vec<Type>) -> Constructor {
    Constructor {
        name: name.to_string(),
        fields,
        index,
        visibility: None,
        span: Span::default(),
    }
}

/// A project defining `List<T> = Nil | Cons(T, List<T>)`.
fn project_with_list() -> ComparatorTypes {
    let mut adts = AdtTypes::new();
    adts.insert(
        "List".to_string(),
        (
            vec!["T".to_string()],
            vec![
                ctor("Nil", 0, vec![]),
                ctor(
                    "Cons",
                    1,
                    vec![Type::TyVar("T".into()), Type::TyVar("List".into())],
                ),
            ],
        ),
    );
    ComparatorTypes::new(
        RecordTypes::new(),
        &HashMap::new(),
        &crate::elaborate::TypeProvenance::default(),
        adts,
        &HashMap::new(),
    )
}

fn list_of(arg: Type) -> Type {
    Type::app("List", vec![arg])
}

/// The def keeps the **`App` spelling**, not the expansion's. Call sites mangle
/// sub-types as they stand, so naming the def after the expanded μ would leave
/// the caller's `compare_AppList_NatE` undefined — ADR 1.8.26b D2's dangling
/// symbol returning.
#[test]
fn the_def_is_named_after_the_application_not_its_expansion() {
    let types = project_with_list();
    let ty = list_of(Type::Nat);
    let (defs, subtypes) = synth_comparator_defs(&ty, &types).expect("List<Nat> is comparable");

    assert_eq!(defs[0].name, comparator_symbol(&ty));
    assert_eq!(defs[0].name, "compare_AppList_NatE");
    assert_eq!(
        subtypes,
        vec![types.expand_adt("List", &[Type::Nat]).unwrap()],
        "the expansion must be RETURNED, so the walk re-enters with the μ"
    );
}

/// Two instantiations of the same generic get **distinct** symbols. A collision
/// would define one comparator and call it for both, silently comparing one
/// element type's values with the other's comparator.
#[test]
fn two_instantiations_get_distinct_symbols() {
    assert_ne!(
        comparator_symbol(&list_of(Type::Nat)),
        comparator_symbol(&list_of(Type::Bool))
    );
}

/// The expansion is what `as_cons_list` recognises — the probe that routes a
/// list into the stack-safe spine. Asserted on the expanded shape because the
/// probe runs at the *head* of `synth_comparator_defs`, so the `App` itself
/// never matches it; the spine is reached on the next pass.
#[test]
fn the_expansion_matches_the_cons_list_probe() {
    let types = project_with_list();
    let ty = list_of(Type::Nat);

    assert_eq!(
        as_cons_list(&ty),
        None,
        "the application itself is not a μ, so the head probe cannot fire on it"
    );
    let expanded = types.expand_adt("List", &[Type::Nat]).unwrap();
    assert_eq!(
        as_cons_list(&expanded),
        Some(Type::Nat),
        "the expansion must be the cons-list shape, with the argument as element"
    );
}

/// AC 6, end to end: the closure over an instantiation contains the P4
/// **spine**. Without the returned sub-type the fixture would still pass while
/// a long `List<Stmt>` got a stack-recursive comparator.
#[test]
fn the_closure_over_an_instantiation_contains_the_stack_safe_spine() {
    let types = project_with_list();
    let walk = synth_closure_bounded(&list_of(Type::Nat), &types, CLOSURE_CAP);

    assert!(walk.converged);
    let names: Vec<&str> = walk.defs.iter().map(|d| d.name.as_str()).collect();
    assert!(
        names.iter().any(|n| n.ends_with("_spine")),
        "no stack-safe spine in the closure: {names:?}"
    );
    assert!(
        names.contains(&"compare_AppList_NatE"),
        "the application's own comparator is missing: {names:?}"
    );
    assert!(
        names.contains(&"compare_Nat"),
        "the element comparator is missing: {names:?}"
    );
}

/// The closure is **complete**: every `compare_*` symbol its bodies call is
/// defined in it. This is the property `gate::classify` enforces, and the one an
/// instantiation used to break — the delegation call would name a comparator
/// nothing synthesized.
#[test]
fn the_closure_over_an_instantiation_has_no_dangling_reference() {
    let types = project_with_list();
    let walk = synth_closure_bounded(&list_of(Type::Nat), &types, CLOSURE_CAP);
    assert_eq!(
        crate::comparator::gate::first_dangling_reference(&walk.defs),
        None
    );
}

/// A nested instantiation closes too: `List<List<Nat>>` needs the inner list's
/// comparator, reached through two expansions.
#[test]
fn a_nested_instantiation_closes() {
    let types = project_with_list();
    let walk = synth_closure_bounded(&list_of(list_of(Type::Nat)), &types, CLOSURE_CAP);

    assert!(walk.converged);
    assert_eq!(
        crate::comparator::gate::first_dangling_reference(&walk.defs),
        None
    );
    let names: Vec<&str> = walk.defs.iter().map(|d| d.name.as_str()).collect();
    assert_eq!(
        names.iter().filter(|n| n.ends_with("_spine")).count(),
        2,
        "both list levels need their own spine: {names:?}"
    );
}

/// An instantiation whose argument is noncomparable synthesizes **nothing** —
/// the refusal has to happen here as well as in the predicate, or the body
/// would emit a call to a comparator that is never defined.
#[test]
fn an_instantiation_with_a_noncomparable_argument_synthesizes_nothing() {
    let types = project_with_list();
    let ty = list_of(Type::arrow(Type::Nat, Type::Nat));
    assert!(synth_comparator_defs(&ty, &types).is_none());
}
