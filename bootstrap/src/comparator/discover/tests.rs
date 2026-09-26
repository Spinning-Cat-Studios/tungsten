//! Tests for comparator discovery + transitive closure (ADR 29.6.26f §T11.8).
//!
//! These mutate the process-global request registry, so they run serially via
//! `requests::TEST_EXCLUSIVE` — the guard declared beside the registry itself,
//! not one local to this file. A file-local mutex serialized only this module,
//! and any test elsewhere that elaborates a project clears the registry (ADR
//! 28.7.26a).

use super::*;
use tungsten_core::terms::SpannedTerm;
use tungsten_core::{Term, Type};

use crate::comparator::ComparatorTypes;
use crate::comparator::{mangling::comparator_symbol, requests};
use crate::driver::RecordTypes;
use crate::elaborate::CoreDef;
use crate::span::Span;

/// These tests use no record types.
/// A def whose body references the comparators of the given types (as call heads).
fn def_referencing(name: &str, tys: &[Type]) -> CoreDef {
    let mut body = Term::Unit;
    for ty in tys {
        body = Term::App(
            Box::new(Term::Global(comparator_symbol(ty))),
            Box::new(body),
        );
    }
    CoreDef {
        name: name.to_string(),
        ty: Type::Unit,
        term: SpannedTerm::generated(body),
        span: Span::new(0, 0),
    }
}

fn def_named(name: &str) -> CoreDef {
    CoreDef {
        name: name.to_string(),
        ty: Type::Unit,
        term: SpannedTerm::generated(Term::Unit),
        span: Span::new(0, 0),
    }
}

/// Register the comparator symbols for `tys` (as the `__compare` form would).
fn register(tys: &[Type]) {
    for ty in tys {
        requests::register(comparator_symbol(ty), ty.clone());
    }
}

#[test]
fn synthesizes_referenced_primitive_comparators() {
    let _g = requests::TEST_EXCLUSIVE.lock().unwrap();
    requests::clear();
    register(&[Type::Nat, Type::String]);
    let defs = vec![def_referencing("caller", &[Type::Nat, Type::String])];
    let synth = synth_missing_comparators(&defs, &ComparatorTypes::default());
    let names: Vec<&str> = synth.iter().map(|d| d.name.as_str()).collect();
    assert_eq!(names, vec!["compare_Nat", "compare_String"]); // sorted
}

#[test]
fn skips_already_defined_comparators() {
    let _g = requests::TEST_EXCLUSIVE.lock().unwrap();
    requests::clear();
    register(&[Type::Nat]);
    let defs = vec![
        def_referencing("caller", &[Type::Nat]),
        def_named("compare_Nat"),
    ];
    assert!(synth_missing_comparators(&defs, &ComparatorTypes::default()).is_empty());
}

#[test]
fn dedups_across_multiple_call_sites() {
    let _g = requests::TEST_EXCLUSIVE.lock().unwrap();
    requests::clear();
    register(&[Type::Nat]);
    let defs = vec![
        def_referencing("a", &[Type::Nat]),
        def_referencing("b", &[Type::Nat]),
    ];
    let synth = synth_missing_comparators(&defs, &ComparatorTypes::default());
    assert_eq!(synth.len(), 1);
    assert_eq!(synth[0].name, "compare_Nat");
}

#[test]
fn closes_over_product_field_comparators() {
    let _g = requests::TEST_EXCLUSIVE.lock().unwrap();
    requests::clear();
    let prod = Type::Product(Box::new(Type::Nat), Box::new(Type::Bool));
    register(&[prod.clone()]);
    // Only the top-level product comparator is referenced directly...
    let defs = vec![def_referencing("caller", &[prod])];
    let synth = synth_missing_comparators(&defs, &ComparatorTypes::default());
    let names: Vec<&str> = synth.iter().map(|d| d.name.as_str()).collect();
    // ...but its field comparators are pulled in transitively (and sorted).
    assert_eq!(
        names,
        vec!["compare_Bool", "compare_Nat", "compare_PNat_BoolE"]
    );
}

#[test]
fn ignores_unregistered_symbols() {
    let _g = requests::TEST_EXCLUSIVE.lock().unwrap();
    requests::clear();
    // Referenced but never registered → cannot resolve a type → skipped.
    let defs = vec![def_referencing("caller", &[Type::Nat])];
    assert!(synth_missing_comparators(&defs, &ComparatorTypes::default()).is_empty());
}

/// `List<Nat>` = `μα. Unit + (Nat × α)` as the elaborator presents it.
fn list_nat() -> Type {
    Type::Mu(
        "α_List".to_string(),
        Box::new(Type::Sum(
            Box::new(Type::Unit),
            Box::new(Type::Product(
                Box::new(Type::Nat),
                Box::new(Type::TyVar("α_List".to_string())),
            )),
        )),
    )
}

/// AC 9 — per-type comparator dedup on the codegen path. `synth_closure_for` is
/// exactly what the codegen `ComparatorSynth` callback emits; asserting its output
/// has **no duplicate symbols** proves each `compare_T` is emitted once per type
/// (the codegen `emitted_comparators` set enforces the same per-unit). A `List`
/// pulls in a wrapper + spine + element comparator — each must appear exactly once.
#[test]
fn synth_closure_for_emits_each_comparator_once() {
    use std::collections::HashSet;
    let _g = requests::TEST_EXCLUSIVE.lock().unwrap();
    requests::clear();

    let closure = synth_closure_for(&list_nat(), &ComparatorTypes::default());
    let names: Vec<&str> = closure.iter().map(|d| d.name.as_str()).collect();

    // No symbol is emitted twice (the dedup invariant codegen relies on).
    let unique: HashSet<&str> = names.iter().copied().collect();
    assert_eq!(
        unique.len(),
        names.len(),
        "duplicate comparator symbols emitted: {names:?}"
    );

    // The cons-list emits its wrapper, its tail-recursive spine, and the element
    // (`compare_Nat`) comparator — each exactly once (ADR-P4).
    let wrapper = comparator_symbol(&list_nat());
    let spine = format!("{wrapper}_spine");
    assert_eq!(names.iter().filter(|n| **n == wrapper).count(), 1);
    assert_eq!(names.iter().filter(|n| **n == spine).count(), 1);
    assert_eq!(names.iter().filter(|n| **n == "compare_Nat").count(), 1);
}
