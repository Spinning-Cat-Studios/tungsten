//! Data model tests: DefId, MonoKey, MonoRequestTable, MonoOwnershipMap.

use std::collections::HashMap;

use tungsten_core::types::Type;

use crate::compile::mono::*;

/// Helper to create a MonoRequest with type_arg.
fn mono_request(key: MonoKey, requester: &str, type_arg: Type) -> MonoRequest {
    MonoRequest {
        key,
        requester_unit: CodegenUnitId(requester.into()),
        type_args: vec![type_arg],
    }
}

// ── DefId tests ─────────────────────────────────────────────────────

#[test]
fn test_def_id_display() {
    let id = DefId::new(vec!["compiler".into(), "lexer".into()], "scan");
    assert_eq!(id.to_string(), "compiler::lexer::scan");
}

#[test]
fn test_def_id_top_level_display() {
    let id = DefId::new(vec![], "main");
    assert_eq!(id.to_string(), "main");
}

#[test]
fn test_def_id_owner_unit() {
    let id = DefId::new(vec!["compiler".into(), "lexer".into()], "scan");
    assert_eq!(id.owner_unit_id().0, "compiler__lexer");
}

// ── MonoKey tests ───────────────────────────────────────────────────

#[test]
fn test_mono_key_equality() {
    let k1 = MonoKey::new(
        DefId::new(vec!["m".into()], "f"),
        CanonicalTypeArgs::from_type(&Type::Nat),
    );
    let k2 = MonoKey::new(
        DefId::new(vec!["m".into()], "f"),
        CanonicalTypeArgs::from_type(&Type::Nat),
    );
    assert_eq!(k1, k2);
}

#[test]
fn test_mono_key_different_type_args() {
    let k1 = MonoKey::new(
        DefId::new(vec!["m".into()], "f"),
        CanonicalTypeArgs::from_type(&Type::Nat),
    );
    let k2 = MonoKey::new(
        DefId::new(vec!["m".into()], "f"),
        CanonicalTypeArgs::from_type(&Type::Bool),
    );
    assert_ne!(k1, k2);
}

#[test]
fn test_mono_key_different_defs() {
    let k1 = MonoKey::new(
        DefId::new(vec!["m".into()], "f"),
        CanonicalTypeArgs::from_type(&Type::Nat),
    );
    let k2 = MonoKey::new(
        DefId::new(vec!["m".into()], "g"),
        CanonicalTypeArgs::from_type(&Type::Nat),
    );
    assert_ne!(k1, k2);
}

// ── MonoRequestTable tests ──────────────────────────────────────────

#[test]
fn test_request_table_add_and_freeze() {
    let mut table = MonoRequestTable::new();
    assert!(!table.is_frozen());

    table.add(mono_request(
        MonoKey::new(
            DefId::new(vec!["m".into()], "f"),
            CanonicalTypeArgs::from_type(&Type::Nat),
        ),
        "a",
        Type::Nat,
    ));

    assert_eq!(table.requests().len(), 1);
    table.freeze();
    assert!(table.is_frozen());
}

#[test]
#[should_panic(expected = "mono request added after freeze")]
fn test_request_table_add_after_freeze_panics() {
    let mut table = MonoRequestTable::new();
    table.freeze();
    table.add(mono_request(
        MonoKey::new(
            DefId::new(vec![], "f"),
            CanonicalTypeArgs::from_type(&Type::Nat),
        ),
        "a",
        Type::Nat,
    ));
}

#[test]
fn test_unique_keys_deduplicates() {
    let mut table = MonoRequestTable::new();
    let key = MonoKey::new(
        DefId::new(vec!["m".into()], "f"),
        CanonicalTypeArgs::from_type(&Type::Nat),
    );

    // Two requests for the same key from different units
    table.add(mono_request(key.clone(), "a", Type::Nat));
    table.add(mono_request(key.clone(), "b", Type::Nat));

    let unique = table.unique_keys();
    assert_eq!(unique.len(), 1);
}

// ── keys_requested_by tests ─────────────────────────────────────────

#[test]
fn test_keys_requested_by_returns_correct_keys() {
    let mut table = MonoRequestTable::new();
    let key_a = MonoKey::new(
        DefId::new(vec!["lib".into()], "f"),
        CanonicalTypeArgs::from_type(&Type::Nat),
    );
    let key_b = MonoKey::new(
        DefId::new(vec!["lib".into()], "g"),
        CanonicalTypeArgs::from_type(&Type::Bool),
    );
    table.add(mono_request(key_a.clone(), "alpha", Type::Nat));
    table.add(mono_request(key_b.clone(), "beta", Type::Bool));
    table.add(mono_request(key_a.clone(), "beta", Type::Nat)); // beta also requests f<Nat>
    table.freeze();

    let alpha_keys = table.keys_requested_by(&CodegenUnitId("alpha".into()));
    assert_eq!(alpha_keys.len(), 1);
    assert_eq!(alpha_keys[0], key_a);

    let beta_keys = table.keys_requested_by(&CodegenUnitId("beta".into()));
    assert_eq!(beta_keys.len(), 2);

    let gamma_keys = table.keys_requested_by(&CodegenUnitId("gamma".into()));
    assert!(gamma_keys.is_empty());
}

#[test]
fn test_keys_requested_by_deduplicates() {
    let mut table = MonoRequestTable::new();
    let key = MonoKey::new(
        DefId::new(vec!["lib".into()], "f"),
        CanonicalTypeArgs::from_type(&Type::Nat),
    );
    // Same unit requests same key twice (e.g., from two call sites)
    table.add(mono_request(key.clone(), "alpha", Type::Nat));
    table.add(mono_request(key.clone(), "alpha", Type::Nat));
    table.freeze();

    let keys = table.keys_requested_by(&CodegenUnitId("alpha".into()));
    assert_eq!(keys.len(), 1, "should deduplicate same key from same unit");
}

// ── MonoOwnershipMap tests ──────────────────────────────────────────

#[test]
fn test_owned_by_filters_correctly() {
    let mut table = MonoRequestTable::new();
    table.add(mono_request(
        MonoKey::new(
            DefId::new(vec!["a".into()], "f"),
            CanonicalTypeArgs::from_type(&Type::Nat),
        ),
        "b",
        Type::Nat,
    ));
    table.add(mono_request(
        MonoKey::new(
            DefId::new(vec!["b".into()], "g"),
            CanonicalTypeArgs::from_type(&Type::Nat),
        ),
        "a",
        Type::Nat,
    ));
    table.freeze();

    let map = assign_owners(&table, &["a".to_string(), "b".to_string()]);

    // All instances go to the mono depot (ADR 9.5.26b)
    let owned_by_a = map.owned_by(&CodegenUnitId("a".into()));
    let owned_by_b = map.owned_by(&CodegenUnitId("b".into()));
    assert!(
        owned_by_a.is_empty(),
        "regular units should not own mono instances"
    );
    assert!(
        owned_by_b.is_empty(),
        "regular units should not own mono instances"
    );

    let depot_owned = map.owned_by(&CodegenUnitId::mono_depot());
    assert_eq!(depot_owned.len(), 2);
}

#[test]
fn test_owned_by_deterministic_order() {
    // Insert entries with names that would sort differently from insertion order.
    // owned_by() must return them sorted by symbol for deterministic IR emission.
    let mut entries = HashMap::new();
    let names = ["zebra", "alpha", "middle"];
    for name in &names {
        let key = MonoKey::new(
            DefId::new(vec!["m".into()], *name),
            CanonicalTypeArgs::from_type(&Type::Nat),
        );
        let depot = CodegenUnitId::mono_depot();
        entries.insert(
            key.clone(),
            MonoOwnership {
                key,
                owner_unit: depot,
                symbol: format!("m__{name}__Nat"),
                type_args: vec![Type::Nat],
            },
        );
    }
    let map = MonoOwnershipMap::new(entries);
    let owned = map.owned_by(&CodegenUnitId::mono_depot());
    let symbols: Vec<&str> = owned.iter().map(|o| o.symbol.as_str()).collect();
    assert_eq!(
        symbols,
        vec!["m__alpha__Nat", "m__middle__Nat", "m__zebra__Nat"],
        "owned_by must return entries sorted by symbol for deterministic emission"
    );
}

#[test]
#[should_panic(expected = "absent from frozen ownership map")]
fn test_get_or_ice_panics_on_missing() {
    let map = MonoOwnershipMap::new(HashMap::new());
    let key = MonoKey::new(
        DefId::new(vec![], "missing"),
        CanonicalTypeArgs::from_type(&Type::Nat),
    );
    map.get_or_ice(&key);
}

// ── Ownership type_arg round-trip ───────────────────────────────────

#[test]
fn test_ownership_preserves_type_arg() {
    let mut table = MonoRequestTable::new();
    let nat_type = Type::Arrow(Box::new(Type::Nat), Box::new(Type::Bool));
    let key = MonoKey::new(
        DefId::new(vec!["m".into()], "f"),
        CanonicalTypeArgs::from_type(&nat_type),
    );
    table.add(mono_request(key.clone(), "m", nat_type.clone()));
    table.freeze();

    let map = assign_owners(&table, &["m".to_string()]);
    let ownership = map.get(&key).unwrap();
    assert_eq!(
        ownership.type_args,
        vec![nat_type],
        "ownership should preserve the original type args"
    );
}
