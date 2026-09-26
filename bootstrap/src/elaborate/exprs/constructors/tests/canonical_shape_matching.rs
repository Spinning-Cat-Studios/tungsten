//! Tests for `try_match_adt_type` against canonical encoding shapes
//! (ADR 21.7.26e third-encoder fold).
//!
//! `encode_adt_constructors_to_sum` used to be a third parallel ADT encoder
//! whose field-product and `Sum`-chain shapes both diverged from the canonical
//! stored encoder, so:
//!   - a ≥3-field constructor never pattern-matched its own canonical
//!     encoding (silent `None`), and
//!   - a 3+-constructor ADT was rendered as a nested `Sum` chain that could
//!     false-match a genuine nested sum type argument.
//! These tests pin the folded behaviour on both axes.
//!
//! The field-product axis is **right**-nested since ADR 1.8.26b D1 (it was
//! left-nested when 21.7.26e folded the encoders). What these tests assert is
//! unchanged by that: there is exactly ONE canonical spelling, and the other
//! nesting must not match. Only which of the two is canonical moved.

use crate::ast::Visibility;
use crate::elaborate::env::{Constructor, TypeDef, TypeDefKind};
use crate::elaborate::Elaborator;
use crate::span::Span;
use tungsten_core::{Context, Type};

fn make_elaborator() -> Elaborator<'static> {
    let ctx = Box::leak(Box::new(Context::new()));
    Elaborator::new(ctx)
}

fn constructor(name: &str, fields: Vec<Type>, index: usize) -> Constructor {
    Constructor {
        name: name.to_string(),
        fields,
        index,
        span: Span::new(0, 0),
        visibility: None,
    }
}

fn define_adt(elab: &mut Elaborator<'_>, name: &str, params: &[&str], ctors: Vec<Constructor>) {
    elab.env.define_type(TypeDef {
        name: name.to_string(),
        params: params.iter().map(|p| (*p).to_string()).collect(),
        kind: TypeDefKind::ADT(ctors),
        visibility: Visibility::Public,
        span: Span::new(0, 0),
        defining_module: None,
        encoded_type: None,
        field_visibilities: Vec::new(),
    });
}

/// A 2-ctor ADT with a 3-field constructor (the `StrMap`/`Tree` node shape):
/// its canonical encoding uses RIGHT-nested products (ADR 1.8.26b D1), the
/// same nesting the constructed value carries.
#[test]
fn multi_field_ctor_matches_right_nested_canonical_product() {
    let mut elab = make_elaborator();
    define_adt(
        &mut elab,
        "Entry",
        &[],
        vec![
            constructor("EntryEmpty", vec![], 0),
            constructor("EntryNode", vec![Type::Nat, Type::String, Type::Bool], 1),
        ],
    );

    // Canonical shape: Unit + (Nat × (String × Bool)) — right-nested product,
    // matching the value `AdtConstruct` carries.
    let canonical = Type::sum(
        Type::Unit,
        Type::product(Type::Nat, Type::product(Type::String, Type::Bool)),
    );
    assert_eq!(
        elab.try_match_adt_type(&canonical),
        Some("Entry".to_string())
    );

    // The other nesting: Unit + ((Nat × String) × Bool). It must NOT match —
    // nothing canonical produces this spelling for Entry, and accepting both
    // would be the "two spellings of one type" defect the fold exists to
    // prevent.
    let left_nested = Type::sum(
        Type::Unit,
        Type::product(Type::product(Type::Nat, Type::String), Type::Bool),
    );
    assert_eq!(elab.try_match_adt_type(&left_nested), None);
}

/// A 3-ctor ADT encodes as `Type::Adt` (ADR 2.2.26), never as a nested Sum
/// chain — so a genuine nested sum type argument must NOT match it, and its
/// canonical `Adt` shape MUST.
#[test]
fn three_ctor_adt_matches_adt_shape_not_nested_sum_chain() {
    let mut elab = make_elaborator();
    define_adt(
        &mut elab,
        "Signal",
        &[],
        vec![
            constructor("Red", vec![], 0),
            constructor("Amber", vec![], 1),
            constructor("Green", vec![Type::Nat], 2),
        ],
    );

    // The old encoder rendered Signal as Unit + (Unit + Nat) — a shape a
    // genuine nested sum type argument can also have. That false positive
    // is the bug: this must stay unmatched now.
    let nested_sum_argument = Type::sum(Type::Unit, Type::sum(Type::Unit, Type::Nat));
    assert_eq!(elab.try_match_adt_type(&nested_sum_argument), None);

    // The canonical Adt shape matches.
    let canonical_adt = Type::adt(
        "Signal".to_string(),
        vec![],
        vec![
            ("Red".to_string(), Type::Unit),
            ("Amber".to_string(), Type::Unit),
            ("Green".to_string(), Type::Nat),
        ],
    );
    assert_eq!(
        elab.try_match_adt_type(&canonical_adt),
        Some("Signal".to_string())
    );
}

/// Direct coverage of the `Adt`-vs-`Adt` arm's individual conjuncts: each
/// case falsifies exactly one condition while the rest hold, so an `&&`→`||`
/// mutant at any chain position flips the verdict (mutation-gate killers).
#[test]
fn adt_pattern_match_rejects_each_mismatch_axis_independently() {
    let elab = make_elaborator();
    let no_params: Vec<String> = vec![];
    let variants = |last: (&str, Type)| {
        vec![
            ("X".to_string(), Type::Unit),
            ("Y".to_string(), Type::Nat),
            (last.0.to_string(), last.1),
        ]
    };
    let base = Type::adt("A".to_string(), vec![], variants(("Z", Type::Bool)));

    // Identical shapes match.
    assert!(elab.types_pattern_match(&base, &base, &no_params));

    // ADT name differs; args, variant count, and payloads all agree.
    let renamed = Type::adt("B".to_string(), vec![], variants(("Z", Type::Bool)));
    assert!(!elab.types_pattern_match(&renamed, &base, &no_params));

    // Type-arg count differs; everything else agrees.
    let extra_arg = Type::adt(
        "A".to_string(),
        vec![Type::Nat],
        variants(("Z", Type::Bool)),
    );
    assert!(!elab.types_pattern_match(&extra_arg, &base, &no_params));

    // Variant count differs; shared prefix agrees.
    let fewer_variants = Type::adt(
        "A".to_string(),
        vec![],
        vec![("X".to_string(), Type::Unit), ("Y".to_string(), Type::Nat)],
    );
    assert!(!elab.types_pattern_match(&fewer_variants, &base, &no_params));

    // Variant name differs; count and payloads agree.
    let renamed_variant = Type::adt("A".to_string(), vec![], variants(("W", Type::Bool)));
    assert!(!elab.types_pattern_match(&renamed_variant, &base, &no_params));

    // Variant payload differs; names and count agree.
    let changed_payload = Type::adt("A".to_string(), vec![], variants(("Z", Type::String)));
    assert!(!elab.types_pattern_match(&changed_payload, &base, &no_params));
}

/// Generic ADTs still match through type-parameter binding after the fold
/// (the pre-existing `Option<Nat>` behaviour, unchanged).
#[test]
fn generic_two_ctor_adt_still_matches_concrete_instantiation() {
    let mut elab = make_elaborator();
    define_adt(
        &mut elab,
        "Option",
        &["T"],
        vec![
            constructor("None", vec![], 0),
            constructor("Some", vec![Type::TyVar("T".to_string())], 1),
        ],
    );

    let sum_concrete = Type::sum(Type::Unit, Type::Nat);
    assert_eq!(
        elab.try_match_adt_type(&sum_concrete),
        Some("Option".to_string())
    );
}
