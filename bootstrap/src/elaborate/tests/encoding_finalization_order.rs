//! Phase-1e encoding-order invariance tests (ADR 22.7.26c).
//!
//! `cache_type_encodings` used to iterate `env.types` (a `HashMap`) in hash
//! order; an ADT's encoding inlined a referenced type's cached encoding only
//! if that reference happened to be already Phase-1e-cached when the referrer
//! was encoded, so the stored trees depended on iteration order.
//!
//! The reproducer shape is an **alias-hidden cycle** — `T` (ADT) → `A`
//! (alias) → `U` (ADT) → `T` — which the ADT-only Phase-1c.5 dependency graph
//! cannot see (aliases are not nodes), so no mutual-recursion group breaks
//! the cycle deterministically. Whichever of `T`/`U` encodes first leaves its
//! own name as the in-stack cycle break in BOTH stored trees (observed:
//! `{T: TyVar(T), U: TyVar(T)}` in one order, `{T: TyVar(U), U: TyVar(U)}`
//! in the other). Eight independent copies of the triple make an accidental
//! full agreement between two hash orders (2⁻⁸ per run pair) vanishingly
//! unlikely, so the test reliably fails on the unordered loop and passes
//! deterministically once Encoding Finalization orders its loop.
//!
//! The env is seeded directly with bare `TyVar("Name")` cross-references
//! (the spelling `resolve_type_ref_tyvar` resolves — `@`-prefixed refs are
//! resolved earlier, in Deferred-TyVar Resolution) rather than going through Phases 1c/1d,
//! which would pre-inline the references and mask the order sensitivity.

use std::collections::HashMap;

use tungsten_core::{Context, Type};

use crate::ast::Visibility;
use crate::elaborate::env::{Constructor, TypeDef, TypeDefKind};
use crate::elaborate::Elaborator;
use crate::span::Span;

/// A single-constructor ADT `name` whose constructor holds one field.
fn single_ctor_adt(name: &str, ctor_name: &str, field: Type) -> TypeDef {
    TypeDef {
        name: name.to_string(),
        params: Vec::new(),
        kind: TypeDefKind::ADT(vec![Constructor {
            name: ctor_name.to_string(),
            fields: vec![field],
            index: 0,
            visibility: None,
            span: Span::new(0, 0),
        }]),
        visibility: Visibility::Public,
        span: Span::new(0, 0),
        defining_module: None,
        encoded_type: None,
        field_visibilities: Vec::new(),
    }
}

/// A non-parameterized alias `name = body`.
fn alias(name: &str, body: Type) -> TypeDef {
    TypeDef {
        name: name.to_string(),
        params: Vec::new(),
        kind: TypeDefKind::Alias(body),
        visibility: Visibility::Public,
        span: Span::new(0, 0),
        defining_module: None,
        encoded_type: None,
        field_visibilities: Vec::new(),
    }
}

/// The fixed type set: eight independent alias-hidden cycles
/// (`Cycle{i}T` → alias `Cycle{i}A` → `Cycle{i}U` → `Cycle{i}T`) plus one
/// acyclic direct ADT→ADT chain (the §1 target path).
fn order_sensitive_type_set() -> Vec<TypeDef> {
    let mut defs = Vec::new();
    for i in 0..8 {
        let (t, a, u) = (
            format!("Cycle{i}T"),
            format!("Cycle{i}A"),
            format!("Cycle{i}U"),
        );
        defs.push(single_ctor_adt(
            &t,
            &format!("MkT{i}"),
            Type::TyVar(a.clone()),
        ));
        defs.push(alias(&a, Type::TyVar(u.clone())));
        defs.push(single_ctor_adt(
            &u,
            &format!("MkU{i}"),
            Type::TyVar(t.clone()),
        ));
    }
    defs.push(single_ctor_adt(
        "ChainHead",
        "MkHead",
        Type::TyVar("ChainTail".to_string()),
    ));
    defs.push(single_ctor_adt("ChainTail", "MkTail", Type::Nat));
    defs
}

/// Seed a fresh elaborator with `defs` in the given order, run Encoding Finalization, and
/// return the stored encodings.
fn encodings_for_insertion_order(defs: &[TypeDef]) -> HashMap<String, Type> {
    let mut ctx = Context::new();
    let mut elaborator = Elaborator::new(&mut ctx);
    for def in defs {
        elaborator.env.define_type(def.clone());
    }
    elaborator.cache_type_encodings();
    elaborator.get_encoded_types()
}

/// AC1 (ADR 22.7.26c): the same type set inserted in ≥3 shuffled orders must
/// yield identical `encoded_types` maps — strict structural `==` per tree,
/// deliberately NOT `normalize_for_comparison`.
#[test]
fn phase1e_encodings_invariant_under_insertion_order() {
    let forward = order_sensitive_type_set();
    let mut reverse = forward.clone();
    reverse.reverse();
    // Interleave: all ADT `U`s first, then aliases, then ADT `T`s.
    let mut grouped = forward.clone();
    grouped.sort_by_key(|def| match &def.kind {
        TypeDefKind::ADT(_) if def.name.ends_with('U') => 0,
        TypeDefKind::Alias(_) => 1,
        _ => 2,
    });

    let baseline = encodings_for_insertion_order(&forward);
    assert!(!baseline.is_empty(), "type set must produce encodings");

    for (label, order) in [("reverse", &reverse), ("grouped", &grouped)] {
        let other = encodings_for_insertion_order(order);
        let mut divergent: Vec<&String> = baseline
            .iter()
            .filter(|(name, encoding)| other.get(*name) != Some(encoding))
            .map(|(name, _)| name)
            .collect();
        divergent.sort();
        for name in &divergent {
            eprintln!(
                "✗ {name}: forward = {} / {label} = {}",
                baseline[*name].display_detailed(),
                other
                    .get(*name)
                    .map(|t| t.display_detailed())
                    .unwrap_or_else(|| "<missing>".to_string())
            );
        }
        assert!(
            divergent.is_empty(),
            "stored encodings differ between forward and {label} insertion order: {divergent:?}"
        );
    }
}
