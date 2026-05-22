//! Ownership assignment, depot routing, determinism, signature parity, and IR dedup tests.

use tungsten_core::types::Type;

use crate::compile::mono::*;

use super::{make_def, make_unit, mono_request};

// ── Ownership tests (ADR 9.5.26b: all mono goes to __mono depot) ────

#[test]
fn test_assign_all_to_mono_depot() {
    let mut table = MonoRequestTable::new();
    let key = MonoKey::new(
        DefId::new(vec!["lib".into()], "f"),
        CanonicalTypeArgs::from_type(&Type::Nat),
    );
    table.add(mono_request(key.clone(), "caller", Type::Nat));
    table.freeze();

    let map = assign_owners(&table, &["lib".to_string(), "caller".to_string()]);
    let ownership = map.get(&key).unwrap();
    assert!(
        ownership.owner_unit.is_mono_depot(),
        "all mono instances should be routed to __mono depot"
    );
}

#[test]
fn test_assign_depot_when_unit_missing() {
    let mut table = MonoRequestTable::new();
    let key = MonoKey::new(
        DefId::new(vec!["nonexistent".into()], "f"),
        CanonicalTypeArgs::from_type(&Type::Nat),
    );
    table.add(mono_request(key.clone(), "caller", Type::Nat));
    table.freeze();

    let map = assign_owners(&table, &["caller".to_string()]);
    let ownership = map.get(&key).unwrap();
    assert!(ownership.owner_unit.is_mono_depot());
}

// ── Determinism test ────────────────────────────────────────────────

#[test]
fn test_deterministic_output() {
    let build = || {
        let mut table = MonoRequestTable::new();
        for name in &["f", "g", "h"] {
            table.add(mono_request(
                MonoKey::new(
                    DefId::new(vec!["m".into()], *name),
                    CanonicalTypeArgs::from_type(&Type::Nat),
                ),
                "caller",
                Type::Nat,
            ));
        }
        table.freeze();
        let map = assign_owners(&table, &["m".to_string(), "caller".to_string()]);
        let mut symbols: Vec<_> = map.entries().values().map(|o| o.symbol.clone()).collect();
        symbols.sort();
        symbols
    };

    let run1 = build();
    let run2 = build();
    assert_eq!(
        run1, run2,
        "mono symbols must be deterministic across builds"
    );
}

// ── Depot ownership ─────────────────────────────────────────────────

#[test]
fn test_all_instances_owned_by_depot() {
    let mut table = MonoRequestTable::new();
    table.add(mono_request(
        MonoKey::new(
            DefId::new(vec!["unknown".into()], "f"),
            CanonicalTypeArgs::from_type(&Type::Nat),
        ),
        "caller",
        Type::Nat,
    ));
    table.add(mono_request(
        MonoKey::new(
            DefId::new(vec!["known".into()], "g"),
            CanonicalTypeArgs::from_type(&Type::Nat),
        ),
        "caller",
        Type::Nat,
    ));
    table.freeze();

    let map = assign_owners(&table, &["known".to_string(), "caller".to_string()]);

    let depot_owned = map.owned_by(&CodegenUnitId::mono_depot());
    assert_eq!(
        depot_owned.len(),
        2,
        "all instances should be owned by depot"
    );

    // No regular unit owns any mono instances
    let known_owned = map.owned_by(&CodegenUnitId("known".into()));
    assert!(
        known_owned.is_empty(),
        "regular units should not own mono instances"
    );
}

// ── IR dedup: single owner for cross-unit references ────────────────

#[test]
fn test_two_units_referencing_same_mono_get_one_owner() {
    let mut table = MonoRequestTable::new();
    let key = MonoKey::new(
        DefId::new(vec!["m".into()], "f"),
        CanonicalTypeArgs::from_type(&Type::Nat),
    );
    table.add(mono_request(key.clone(), "unit_a", Type::Nat));
    table.add(mono_request(key.clone(), "unit_b", Type::Nat));
    table.freeze();

    let map = assign_owners(&table, &["m".into(), "unit_a".into(), "unit_b".into()]);

    // Only one ownership entry for f<Nat>
    assert_eq!(
        map.entries().len(),
        1,
        "duplicate ownership entries for same mono key"
    );

    let ownership = map.get(&key).unwrap();
    // Owner should be the mono depot
    assert!(ownership.owner_unit.is_mono_depot());

    // Both requesting units see the same symbol
    let sym = &ownership.symbol;
    assert!(!sym.is_empty());

    // Non-depot units get zero owned entries
    assert!(map.owned_by(&CodegenUnitId("unit_a".into())).is_empty());
    assert!(map.owned_by(&CodegenUnitId("unit_b".into())).is_empty());
}

// ── Define/declare signature parity ─────────────────────────────────

#[test]
fn test_owner_and_nonowner_see_same_symbol_and_type_arg() {
    let mut table = MonoRequestTable::new();
    let key = MonoKey::new(
        DefId::new(vec!["m".into()], "f"),
        CanonicalTypeArgs::from_type(&Type::Nat),
    );
    table.add(mono_request(key.clone(), "m", Type::Nat));
    table.add(mono_request(key.clone(), "caller", Type::Nat));
    table.freeze();

    let map = assign_owners(&table, &["m".into(), "caller".into()]);

    assert_eq!(map.entries().len(), 1);
    let ownership = map.get(&key).unwrap();
    // All mono instances go to depot (ADR 9.5.26b)
    assert!(ownership.owner_unit.is_mono_depot());

    let also = map.get(&key).unwrap();
    assert_eq!(ownership.symbol, also.symbol);
    assert_eq!(
        format!("{:?}", ownership.type_args),
        format!("{:?}", also.type_args),
        "type_args must match between owner and non-owner views"
    );
}

// ── Multiple type args produce distinct symbols ─────────────────────

#[test]
fn test_same_def_different_type_args_get_distinct_owners() {
    let mut table = MonoRequestTable::new();
    let k1 = MonoKey::new(
        DefId::new(vec!["m".into()], "f"),
        CanonicalTypeArgs::from_type(&Type::Nat),
    );
    let k2 = MonoKey::new(
        DefId::new(vec!["m".into()], "f"),
        CanonicalTypeArgs::from_type(&Type::Bool),
    );
    table.add(mono_request(k1.clone(), "caller", Type::Nat));
    table.add(mono_request(k2.clone(), "caller", Type::Bool));
    table.freeze();

    let map = assign_owners(&table, &["m".into(), "caller".into()]);

    assert_eq!(map.entries().len(), 2);
    let o1 = map.get(&k1).unwrap();
    let o2 = map.get(&k2).unwrap();
    assert_ne!(
        o1.symbol, o2.symbol,
        "f<Nat> and f<Bool> must have distinct symbols"
    );
}

// ── Ownership disjointness (AC 7: no duplicate defines) ─────────────

/// Verify that owned_by sets are disjoint: each mono key is owned by exactly
/// one unit, guaranteeing no duplicate `define` symbols across `.ll` files.
#[test]
fn test_owned_by_sets_are_disjoint_across_units() {
    use tungsten_core::terms::Term;

    // 3 units, 3 polymorphic defs, multiple cross-unit references
    let units = vec![
        make_unit(
            &["a"],
            "a.tg",
            vec![
                make_def(
                    "f",
                    Term::TyAbs("T".into(), Box::new(Term::Unit)),
                    Type::Forall("T".into(), Box::new(Type::Unit)),
                ),
                // a calls g<Nat> and h<Bool>
                make_def(
                    "use_g",
                    Term::ty_app(Term::Global("g".into()), Type::Nat),
                    Type::Nat,
                ),
                make_def(
                    "use_h",
                    Term::ty_app(Term::Global("h".into()), Type::Bool),
                    Type::Nat,
                ),
            ],
        ),
        make_unit(
            &["b"],
            "b.tg",
            vec![
                make_def(
                    "g",
                    Term::TyAbs("T".into(), Box::new(Term::Unit)),
                    Type::Forall("T".into(), Box::new(Type::Unit)),
                ),
                // b calls f<Nat> and h<Nat>
                make_def(
                    "use_f",
                    Term::ty_app(Term::Global("f".into()), Type::Nat),
                    Type::Nat,
                ),
                make_def(
                    "use_h2",
                    Term::ty_app(Term::Global("h".into()), Type::Nat),
                    Type::Nat,
                ),
            ],
        ),
        make_unit(
            &["c"],
            "c.tg",
            vec![
                make_def(
                    "h",
                    Term::TyAbs("T".into(), Box::new(Term::Unit)),
                    Type::Forall("T".into(), Box::new(Type::Unit)),
                ),
                // c calls f<Bool> and g<Bool>
                make_def(
                    "use_f2",
                    Term::ty_app(Term::Global("f".into()), Type::Bool),
                    Type::Nat,
                ),
                make_def(
                    "use_g2",
                    Term::ty_app(Term::Global("g".into()), Type::Bool),
                    Type::Nat,
                ),
            ],
        ),
    ];

    let table = discover_mono_requests(&units, std::path::Path::new(""), &Default::default());
    let mut table = table;
    table.freeze();

    let unit_names = vec!["a".to_string(), "b".to_string(), "c".to_string()];
    let map = assign_owners(&table, &unit_names);

    // Collect all owned keys — all should be in the depot
    let mut all_owned_keys = std::collections::HashSet::new();
    // No regular unit owns anything
    for name in &unit_names {
        let unit_id = CodegenUnitId(name.clone());
        let owned = map.owned_by(&unit_id);
        assert!(
            owned.is_empty(),
            "regular unit '{}' should not own mono instances",
            name
        );
    }
    // All instances are in the depot
    let depot_owned = map.owned_by(&CodegenUnitId::mono_depot());
    for o in &depot_owned {
        let inserted = all_owned_keys.insert(o.key.clone());
        assert!(inserted, "key {:?} duplicated in depot", o.key);
    }

    // Every key in the map must appear in the depot
    assert_eq!(
        all_owned_keys.len(),
        map.entries().len(),
        "some keys are not covered by the depot"
    );
}

/// Verify the depot dependency model invariant (ADR 9.5.26b §2.3):
/// per-function units that call specializations must be able to discover
/// those specializations in the ownership map for `declare` emission.
#[test]
fn test_depot_dependency_model_declares() {
    // Two per-function units each request a different specialization of "f".
    // Both specializations land in the __mono depot.
    // Each caller can discover its requested keys for `declare` emission.
    let key = MonoKey::new(
        DefId::new(vec!["lib".into()], "f"),
        CanonicalTypeArgs::from_type(&Type::Nat),
    );
    let key2 = MonoKey::new(
        DefId::new(vec!["lib".into()], "f"),
        CanonicalTypeArgs::from_type(&Type::Bool),
    );

    let mut table = MonoRequestTable::new();
    table.add(mono_request(key.clone(), "a__use_f_nat", Type::Nat));
    table.add(mono_request(key2.clone(), "b__use_f_bool", Type::Nat));
    table.freeze();

    let unit_names = vec![
        "a__f".to_string(),
        "a__use_f_nat".to_string(),
        "b__use_f_bool".to_string(),
    ];
    let map = assign_owners(&table, &unit_names);

    // Depot owns all specializations
    for (_key, ownership) in map.entries() {
        assert!(
            ownership.owner_unit.is_mono_depot(),
            "specialization should be owned by depot"
        );
        assert!(
            !ownership.symbol.is_empty(),
            "specialization must have a mangled symbol for declare"
        );
    }

    // Each caller discovers exactly its requested key
    let a_keys = table.keys_requested_by(&CodegenUnitId("a__use_f_nat".into()));
    assert_eq!(a_keys.len(), 1, "unit a should request 1 specialization");

    let b_keys = table.keys_requested_by(&CodegenUnitId("b__use_f_bool".into()));
    assert_eq!(b_keys.len(), 1, "unit b should request 1 specialization");

    assert_ne!(
        a_keys[0], b_keys[0],
        "different type args should produce different keys"
    );
}
