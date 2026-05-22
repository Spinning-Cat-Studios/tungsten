//! Symbol mangling and validation tests.

use std::collections::HashMap;

use tungsten_core::types::Type;

use crate::compile::mono::*;

use super::mono_request;

#[test]
fn test_mangle_mono_symbol_contains_type_info() {
    let key = MonoKey::new(
        DefId::new(vec!["list".into()], "map"),
        CanonicalTypeArgs::from_type(&Type::Nat),
    );
    let symbol = symbols::mangle_mono_symbol(&key);
    assert!(symbol.starts_with("_tg_"));
    assert!(symbol.contains("_I_"));
    assert!(symbol.contains("Nat"));
}

#[test]
fn test_different_type_args_different_symbols() {
    let k1 = MonoKey::new(
        DefId::new(vec!["m".into()], "f"),
        CanonicalTypeArgs::from_type(&Type::Nat),
    );
    let k2 = MonoKey::new(
        DefId::new(vec!["m".into()], "f"),
        CanonicalTypeArgs::from_type(&Type::Bool),
    );
    let s1 = symbols::mangle_mono_symbol(&k1);
    let s2 = symbols::mangle_mono_symbol(&k2);
    assert_ne!(s1, s2);
}

#[test]
fn test_validate_symbols_clean() {
    let mut table = MonoRequestTable::new();
    table.add(mono_request(
        MonoKey::new(
            DefId::new(vec!["m".into()], "f"),
            CanonicalTypeArgs::from_type(&Type::Nat),
        ),
        "m",
        Type::Nat,
    ));
    table.add(mono_request(
        MonoKey::new(
            DefId::new(vec!["m".into()], "f"),
            CanonicalTypeArgs::from_type(&Type::Bool),
        ),
        "m",
        Type::Bool,
    ));
    table.freeze();

    let map = assign_owners(&table, &["m".to_string()]);
    assert!(validate_symbols(&map).is_ok());
}

#[test]
fn test_sanitize_type_args_special_characters() {
    let complex_type = Type::Forall(
        "α".into(),
        Box::new(Type::Arrow(
            Box::new(Type::TyVar("α".into())),
            Box::new(Type::Nat),
        )),
    );
    let k1 = MonoKey::new(
        DefId::new(vec!["m".into()], "f"),
        CanonicalTypeArgs::from_type(&complex_type),
    );
    let symbol = symbols::mangle_mono_symbol(&k1);
    assert!(
        symbol
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_'),
        "symbol contains invalid characters: {}",
        symbol
    );
    assert!(!symbol.is_empty());
}

#[test]
fn test_sanitize_type_args_simple_nat() {
    let k = MonoKey::new(
        DefId::new(vec!["m".into()], "f"),
        CanonicalTypeArgs::from_type(&Type::Nat),
    );
    let symbol = symbols::mangle_mono_symbol(&k);
    assert!(symbol.contains("Nat"), "expected Nat in symbol: {}", symbol);
}

#[test]
fn test_validate_symbols_detects_collision() {
    let key1 = MonoKey::new(
        DefId::new(vec!["a".into()], "f"),
        CanonicalTypeArgs("Nat".into()),
    );
    let key2 = MonoKey::new(
        DefId::new(vec!["b".into()], "g"),
        CanonicalTypeArgs("Bool".into()),
    );
    let mut entries = HashMap::new();
    entries.insert(
        key1.clone(),
        MonoOwnership {
            key: key1,
            owner_unit: CodegenUnitId("a".into()),
            symbol: "COLLIDING_SYMBOL".into(),
            type_args: vec![Type::Nat],
        },
    );
    entries.insert(
        key2.clone(),
        MonoOwnership {
            key: key2,
            owner_unit: CodegenUnitId("b".into()),
            symbol: "COLLIDING_SYMBOL".into(),
            type_args: vec![Type::Bool],
        },
    );
    let map = MonoOwnershipMap::new(entries);

    let result = validate_symbols(&map);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.contains("collision"),
        "expected collision error: {}",
        err
    );
}
