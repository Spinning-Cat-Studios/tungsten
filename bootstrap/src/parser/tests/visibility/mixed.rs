//! Mixed visibility integration test.

use crate::ast::*;
use crate::parser::tests::parse_ok;

#[test]
fn test_mixed_visibility_items() {
    let file = parse_ok(
        r#"
        pub fn public_fn() { 1 }
        pub(crate) fn crate_fn() { 2 }
        fn private_fn() { 3 }
        pub type PublicType = Nat
        pub(crate) type CrateType = Bool
        type PrivateType = Unit
    "#,
    );
    assert_eq!(file.items.len(), 6);

    match &file.items[0] {
        Item::Function(f) => assert_eq!(f.visibility, Visibility::Public),
        _ => panic!("Expected function"),
    }

    match &file.items[1] {
        Item::Function(f) => assert_eq!(f.visibility, Visibility::Crate),
        _ => panic!("Expected function"),
    }

    match &file.items[2] {
        Item::Function(f) => assert_eq!(f.visibility, Visibility::Private),
        _ => panic!("Expected function"),
    }

    match &file.items[3] {
        Item::TypeAlias(t) => assert_eq!(t.visibility, Visibility::Public),
        _ => panic!("Expected type alias"),
    }

    match &file.items[4] {
        Item::TypeAlias(t) => assert_eq!(t.visibility, Visibility::Crate),
        _ => panic!("Expected type alias"),
    }

    match &file.items[5] {
        Item::TypeAlias(t) => assert_eq!(t.visibility, Visibility::Private),
        _ => panic!("Expected type alias"),
    }
}
