//! Phase-1d resolution-order invariance tests (ADR 22.7.26d).
//!
//! `resolve_deferred_type_references` used to iterate `env.types` (a
//! `HashMap`) in hash order. Resolving a referrer inlines its referent's
//! *current* body: deep if the referent was already resolved earlier in the
//! same loop, shallow if not — because the encoding-path resolver
//! (`resolve_type_ref_tyvar`, types/resolve_refs.rs) looks up **bare** names
//! only, a not-yet-resolved referent's body embeds with its own `@`-prefixed
//! references frozen inside. So the inline depth of a resolved body was a
//! function of hash order.
//!
//! The reproducer shape is the record class ADR 22.7.26d's evidence pinned
//! (`MatchArm`/`FieldInit`/`PatternInfo`/`TypeDef` on main.tg): a record
//! whose field references an ADT that itself references another ADT —
//! `Deep{i}R` (record) → `@Deep{i}E` (ADT) → `@Deep{i}P` (ADT). When `R` is
//! resolved before `E`, the embedded `E` expansion keeps `TyVar("@Deep{i}P")`;
//! when `E` resolves first, `P` is fully inlined. Eight independent triples
//! make accidental agreement between two hash orders (~2⁻⁸ per run pair)
//! vanishingly unlikely, so the invariance test reliably fails on the
//! unordered loop and passes deterministically once Deferred-TyVar Resolution resolves in
//! reverse-topological dependency order (referents before referrers).
//!
//! The env is seeded with `@`-prefixed `TyVar` cross-references — the
//! Phase-1c deferred spelling that Deferred-TyVar Resolution exists to resolve (the mirror of
//! the bare-name seeding note in `encoding_finalization_order.rs`).

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

/// A non-parameterized record `name` with one field.
fn single_field_record(name: &str, field_name: &str, field_ty: Type) -> TypeDef {
    TypeDef {
        name: name.to_string(),
        params: Vec::new(),
        kind: TypeDefKind::Record(vec![(field_name.to_string(), field_ty)]),
        visibility: Visibility::Public,
        span: Span::new(0, 0),
        defining_module: None,
        encoded_type: None,
        field_visibilities: Vec::new(),
    }
}

/// The fixed type set: eight independent record→ADT→ADT chains
/// (`Deep{i}R` record → `@Deep{i}E` ADT → `@Deep{i}P` ADT → Nat), the
/// ADR 22.7.26d record class.
fn depth_sensitive_type_set() -> Vec<TypeDef> {
    let mut defs = Vec::new();
    for i in 0..8 {
        let (record, embedded, payload) = (
            format!("Deep{i}R"),
            format!("Deep{i}E"),
            format!("Deep{i}P"),
        );
        defs.push(single_field_record(
            &record,
            "payload",
            Type::TyVar(format!("@{embedded}")),
        ));
        defs.push(single_ctor_adt(
            &embedded,
            &format!("MkE{i}"),
            Type::TyVar(format!("@{payload}")),
        ));
        defs.push(single_ctor_adt(&payload, &format!("MkP{i}"), Type::Nat));
    }
    defs
}

/// Seed a fresh elaborator with `defs` in the given order, run Deferred-TyVar Resolution, and
/// return each type's resolved body types (ctor fields / record fields /
/// alias body).
fn resolved_bodies_for_insertion_order(defs: &[TypeDef]) -> HashMap<String, Vec<Type>> {
    let mut ctx = Context::new();
    let mut elaborator = Elaborator::new(&mut ctx);
    for def in defs {
        elaborator.env.define_type(def.clone());
    }
    elaborator.resolve_deferred_type_references();

    defs.iter()
        .map(|def| {
            let resolved = elaborator
                .env
                .lookup_type(&def.name)
                .expect("seeded type must survive Deferred-TyVar Resolution");
            let body_types: Vec<Type> = match &resolved.kind {
                TypeDefKind::ADT(ctors) => ctors
                    .iter()
                    .flat_map(|ctor| ctor.fields.iter().cloned())
                    .collect(),
                TypeDefKind::Record(fields) => fields
                    .iter()
                    .map(|(_, field_ty)| field_ty.clone())
                    .collect(),
                TypeDefKind::Alias(ty) => vec![ty.clone()],
                TypeDefKind::Stub => Vec::new(),
            };
            (def.name.clone(), body_types)
        })
        .collect()
}

/// AC1 (ADR 22.7.26d): the same type set inserted in ≥3 shuffled orders must
/// yield identical post-Phase-1d bodies — strict structural `==` per tree,
/// deliberately NOT `normalize_for_comparison` (normalization is the tier-2
/// workaround this ADR retires from load-bearing duty).
#[test]
fn phase1d_resolved_bodies_invariant_under_insertion_order() {
    let forward = depth_sensitive_type_set();
    let mut reverse = forward.clone();
    reverse.reverse();
    // Referrers first: all records, then the embedded ADTs, then payloads —
    // the order that maximizes shallow embeds on the unordered loop.
    let mut records_first = forward.clone();
    records_first.sort_by_key(|def| match &def.kind {
        TypeDefKind::Record(_) => 0,
        TypeDefKind::ADT(_) if def.name.ends_with('E') => 1,
        _ => 2,
    });

    let baseline = resolved_bodies_for_insertion_order(&forward);
    assert!(
        !baseline.is_empty(),
        "type set must produce resolved bodies"
    );

    for (label, order) in [("reverse", &reverse), ("records_first", &records_first)] {
        let other = resolved_bodies_for_insertion_order(order);
        let mut divergent: Vec<&String> = baseline
            .iter()
            .filter(|(name, bodies)| other.get(*name) != Some(bodies))
            .map(|(name, _)| name)
            .collect();
        divergent.sort();
        for name in &divergent {
            eprintln!(
                "✗ {name}: forward = {:?} / {label} = {:?}",
                baseline[*name],
                other.get(*name)
            );
        }
        assert!(
            divergent.is_empty(),
            "Phase-1d resolved bodies differ between forward and {label} insertion order: {divergent:?}"
        );
    }
}

/// Canonical depth (ADR 22.7.26d): after a dependency-ordered Deferred-TyVar Resolution, no
/// resolved record body retains a deferred `@`-prefixed named reference —
/// every ADT referent was resolved before its referrer, so each embed carries
/// the referent's full expansion. (On the unordered loop this fails whenever
/// any record wins the hash-order race against its referent.)
#[test]
fn phase1d_leaves_no_deferred_refs_in_record_bodies() {
    let defs = depth_sensitive_type_set();
    let resolved = resolved_bodies_for_insertion_order(&defs);

    let mut offenders: Vec<String> = resolved
        .iter()
        .filter(|(_, bodies)| {
            bodies
                .iter()
                .any(|ty| ty.free_type_vars().iter().any(|var| var.starts_with('@')))
        })
        .map(|(name, _)| name.clone())
        .collect();
    offenders.sort();
    assert!(
        offenders.is_empty(),
        "resolved bodies still hold deferred @-references: {offenders:?}"
    );
}

/// The nominal-back-edge SCC class (ADR 22.7.26d residual): a cycle that
/// passes *through a record* — `Registry` (ADT) → `Catalog` (record) →
/// `Entry` (ADT) → `Payload` (ADT) → `Registry` — is not a genuine inline
/// cycle, because a reference **to a record** stays nominal by design. Left
/// in the ordering graph, the `Registry → Catalog` edge welds all four into
/// one SCC whose lexicographic member order resolves `Catalog` first,
/// freezing `TyVar("@Payload")` inside the not-yet-resolved `Entry` embed
/// (main.tg: the stable `TypeDef`-record divergence, where `@TypeExpr` froze
/// inside the `TypeDefBody` embed). With record-target edges dropped from
/// the ordering graph, the SCC dissolves, `Entry` and `Payload` resolve
/// before `Catalog`, and the record's field embeds `Entry`'s full expansion
/// — the only deferred ref left is the nominal record reference `@Catalog`
/// itself (expected: records stay nominal).
#[test]
fn phase1d_record_backedge_cycle_resolves_record_after_adt_referents() {
    let two_field_ctor_adt = |name: &str, ctor: &str, first: Type, second: Type| TypeDef {
        name: name.to_string(),
        params: Vec::new(),
        kind: TypeDefKind::ADT(vec![Constructor {
            name: ctor.to_string(),
            fields: vec![first, second],
            index: 0,
            visibility: None,
            span: Span::new(0, 0),
        }]),
        visibility: Visibility::Public,
        span: Span::new(0, 0),
        defining_module: None,
        encoded_type: None,
        field_visibilities: Vec::new(),
    };
    let defs = vec![
        single_ctor_adt(
            "Registry",
            "MkRegistry",
            Type::TyVar("@Catalog".to_string()),
        ),
        single_field_record("Catalog", "entry", Type::TyVar("@Entry".to_string())),
        two_field_ctor_adt(
            "Entry",
            "MkEntry",
            Type::TyVar("@Payload".to_string()),
            Type::Nat,
        ),
        two_field_ctor_adt(
            "Payload",
            "MkPayload",
            Type::Nat,
            Type::TyVar("@Registry".to_string()),
        ),
    ];
    let resolved = resolved_bodies_for_insertion_order(&defs);

    let catalog_field = &resolved["Catalog"][0];
    let mut deferred_refs: Vec<String> = catalog_field
        .free_type_vars()
        .into_iter()
        .filter(|var| var.starts_with('@'))
        .collect();
    deferred_refs.sort();
    deferred_refs.dedup();
    assert_eq!(
        deferred_refs,
        vec!["@Catalog".to_string()],
        "Catalog's field must embed Entry and Payload fully, leaving only the \
         nominal record reference: got {catalog_field}"
    );
}
