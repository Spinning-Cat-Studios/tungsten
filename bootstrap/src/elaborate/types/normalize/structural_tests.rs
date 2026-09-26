//! Tests for `types_structurally_equal_impl`'s `Adt` arm (ADR 22.7.26b).
//!
//! Before the fix, two `Type::Adt` values fell through to `_ => false`, so the
//! crate's canonical structural-equality function reported *identical* ADTs as
//! unequal (21.7.26j's first main.tg run flagged 44 healthy ADTs this way).

use crate::elaborate::Elaborator;
use tungsten_core::{Context, Type};

fn make_elaborator() -> Elaborator<'static> {
    let ctx = Box::leak(Box::new(Context::new()));
    Elaborator::new(ctx)
}

/// A representative ADT value: `Option<Nat>` with `None: Unit`, `Some: Nat`.
fn option_nat_adt() -> Type {
    Type::Adt(
        "Option".to_string(),
        vec![Type::Nat],
        vec![
            ("None".to_string(), Type::Unit),
            ("Some".to_string(), Type::Nat),
        ],
    )
}

#[test]
fn identical_adts_compare_equal() {
    let elab = make_elaborator();
    assert!(elab.types_structurally_equal_impl(&option_nat_adt(), &option_nat_adt()));
}

#[test]
fn adt_name_mismatch_is_unequal() {
    let elab = make_elaborator();
    let renamed = Type::Adt(
        "Result".to_string(),
        vec![Type::Nat],
        vec![
            ("None".to_string(), Type::Unit),
            ("Some".to_string(), Type::Nat),
        ],
    );
    assert!(!elab.types_structurally_equal_impl(&option_nat_adt(), &renamed));
}

#[test]
fn adt_type_arg_mismatch_is_unequal() {
    let elab = make_elaborator();
    let other_arg = Type::Adt(
        "Option".to_string(),
        vec![Type::Bool],
        vec![
            ("None".to_string(), Type::Unit),
            ("Some".to_string(), Type::Nat),
        ],
    );
    assert!(!elab.types_structurally_equal_impl(&option_nat_adt(), &other_arg));
}

#[test]
fn adt_variant_name_mismatch_is_unequal() {
    let elab = make_elaborator();
    let other_variant = Type::Adt(
        "Option".to_string(),
        vec![Type::Nat],
        vec![
            ("Nil".to_string(), Type::Unit),
            ("Some".to_string(), Type::Nat),
        ],
    );
    assert!(!elab.types_structurally_equal_impl(&option_nat_adt(), &other_variant));
}

#[test]
fn adt_variant_field_type_mismatch_is_unequal() {
    let elab = make_elaborator();
    let other_field = Type::Adt(
        "Option".to_string(),
        vec![Type::Nat],
        vec![
            ("None".to_string(), Type::Unit),
            ("Some".to_string(), Type::String),
        ],
    );
    assert!(!elab.types_structurally_equal_impl(&option_nat_adt(), &other_field));
}

#[test]
fn adt_variant_count_mismatch_is_unequal() {
    let elab = make_elaborator();
    let fewer_variants = Type::Adt(
        "Option".to_string(),
        vec![Type::Nat],
        vec![("None".to_string(), Type::Unit)],
    );
    assert!(!elab.types_structurally_equal_impl(&option_nat_adt(), &fewer_variants));
}

#[test]
fn adt_nested_in_compound_type_compares_equal() {
    let elab = make_elaborator();
    let wrapped_a = Type::product(option_nat_adt(), Type::Bool);
    let wrapped_b = Type::product(option_nat_adt(), Type::Bool);
    assert!(elab.types_structurally_equal_impl(&wrapped_a, &wrapped_b));
}

#[test]
fn adt_against_non_adt_is_unequal() {
    let elab = make_elaborator();
    assert!(!elab.types_structurally_equal_impl(&option_nat_adt(), &Type::Nat));
}
