use super::*;

// 18.9.26f AC1: the table's length is pinned, so a row added without a
// match arm (or the reverse) is a deliberate two-line change.
#[test]
fn primitive_table_has_seven_rows() {
    assert_eq!(PRIMITIVE_TYPES.len(), 7);
}

// 18.9.26f AC1: every row round-trips through both directions.
#[test]
fn primitive_table_round_trips_with_primitive_name() {
    for (name, ty) in PRIMITIVE_TYPES {
        assert_eq!(ty.primitive_name(), Some(*name));
        assert_eq!(Type::primitive_by_name(name).as_ref(), Some(ty));
        assert!(ty.is_primitive());
    }
}

#[test]
fn non_primitive_types_have_no_primitive_name() {
    let samples = [
        Type::arrow(Type::Nat, Type::Nat),
        Type::TyVar("a".to_string()),
        Type::Ptr(Box::new(Type::Nat)),
        Type::App("List".to_string(), vec![]),
        Type::Error,
    ];
    for ty in &samples {
        assert_eq!(ty.primitive_name(), None, "{ty:?}");
        assert!(!ty.is_primitive(), "{ty:?}");
    }
}

#[test]
fn unknown_names_resolve_to_no_primitive() {
    assert_eq!(Type::primitive_by_name("Eq"), None);
    assert_eq!(Type::primitive_by_name("nat"), None);
    assert_eq!(Type::primitive_by_name(""), None);
}
