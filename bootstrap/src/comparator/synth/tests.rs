//! Tests for `compare_T` `CoreDef` synthesis (ADR 29.6.26f §T11.2).

use super::*;
use crate::comparator::ComparatorTypes;
use crate::driver::RecordTypes;
use tungsten_core::{Term, Type};

/// Most tests use no record types.
fn product_nat_bool() -> Type {
    Type::Product(Box::new(Type::Nat), Box::new(Type::Bool))
}

/// `List<Nat>` as the elaborator presents it: `μα. Unit + (Nat × α)`.
fn list_nat_mu() -> Type {
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

#[test]
fn supported_types() {
    let r = ComparatorTypes::default();
    assert!(is_supported(&Type::Nat, &r));
    assert!(is_supported(&Type::Bool, &r));
    assert!(is_supported(&Type::String, &r));
    assert!(is_supported(&Type::Unit, &r));
    assert!(is_supported(&product_nat_bool(), &r));
    // Sums (e.g. the encoding of `Option<Nat>` = `Unit + Nat`).
    assert!(is_supported(
        &Type::Sum(Box::new(Type::Unit), Box::new(Type::Nat)),
        &r
    ));
    // Named/opaque types are not yet synthesizable.
    assert!(!is_supported(&Type::TyVar("Expr".to_string()), &r));
    // A composite is supported only if every component is.
    assert!(!is_supported(
        &Type::Product(
            Box::new(Type::Nat),
            Box::new(Type::TyVar("Expr".to_string()))
        ),
        &r
    ));
}

#[test]
fn unsupported_type_yields_no_comparator() {
    assert!(synth_comparator(
        &Type::TyVar("Expr".to_string()),
        &ComparatorTypes::default()
    )
    .is_none());
}

#[test]
fn flat_adt_supported_and_dispatches_via_adtmatch() {
    let color = Type::Adt(
        "Color".to_string(),
        vec![],
        vec![
            ("Red".to_string(), Type::Unit),
            ("Green".to_string(), Type::Unit),
            ("Blue".to_string(), Type::Unit),
        ],
    );
    assert!(is_supported(&color, &ComparatorTypes::default()));
    let (def, subtypes) =
        synth_comparator(&color, &ComparatorTypes::default()).expect("flat ADT is synthesizable");
    assert_eq!(def.name, "compare_AdtColor_E");
    assert_eq!(subtypes, vec![Type::Unit, Type::Unit, Type::Unit]);
    let Term::Lambda(_, _, inner) = &def.term.term else {
        panic!("not a lambda");
    };
    let Term::Lambda(_, _, body) = inner.as_ref() else {
        panic!("not curried");
    };
    assert!(
        matches!(body.as_ref(), Term::AdtMatch(..)),
        "flat ADT dispatches via AdtMatch"
    );
}

#[test]
fn cons_list_mu_emits_tailrec_spine() {
    // ADR-P4: a cons-list `μα. Unit + (Nat × α)` synthesizes a wrapper + a
    // stack-safe tail-recursive spine, referencing only the *element* comparator.
    let list = list_nat_mu();
    assert!(
        is_supported(&list, &ComparatorTypes::default()),
        "recursive list is supported via μ-bound var"
    );

    // Primary (wrapper) references only the element type (Nat), not the unrolled body.
    let (wrapper, subtypes) =
        synth_comparator(&list, &ComparatorTypes::default()).expect("μ type is synthesizable");
    assert_eq!(
        subtypes,
        vec![Type::Nat],
        "spine closes over the element type"
    );

    // Full def set: wrapper + spine.
    let (defs, _) =
        synth_comparator_defs(&list, &ComparatorTypes::default()).expect("synthesizable");
    assert_eq!(defs.len(), 2, "cons-list emits wrapper + spine");
    let wrapper_sym = comparator_symbol(&list);
    let spine_sym = format!("{wrapper_sym}_spine");
    assert_eq!(defs[0].name, wrapper_sym);
    assert_eq!(defs[1].name, spine_sym);

    // Wrapper body seeds the spine at index 0: `spine(l)(r)(0)`.
    let Term::Lambda(_, _, inner) = &wrapper.term.term else {
        panic!("wrapper not a lambda");
    };
    let Term::Lambda(_, _, body) = inner.as_ref() else {
        panic!("wrapper not curried");
    };
    // Peel `App(App(App(Global(spine), l), r), 0)`.
    let Term::App(f2, seed) = body.as_ref() else {
        panic!("wrapper body not an application");
    };
    assert!(
        matches!(seed.as_ref(), Term::NatLit(0)),
        "spine seeded at index 0, got {seed:?}"
    );
    let Term::App(f1, _r) = f2.as_ref() else {
        panic!("not curried (2)");
    };
    let Term::App(g, _l) = f1.as_ref() else {
        panic!("not curried (3)");
    };
    assert!(
        matches!(g.as_ref(), Term::Global(name) if *name == spine_sym),
        "wrapper calls the spine global, got {g:?}"
    );

    // Spine unfolds both operands and self-recurses (contains a Global(spine) call).
    let spine_src = format!("{:?}", defs[1].term.term);
    assert!(spine_src.contains("Unfold"), "spine unfolds operands");
    assert!(
        spine_src.contains(&format!("Global(\"{spine_sym}\")")),
        "spine self-recurses"
    );
}

#[test]
fn record_emits_field_paths_and_closes_over_field_types() {
    // `type Point = { x: Nat, y: Nat }` → App("Point", []) resolved via records.
    let point = Type::App("Point".to_string(), vec![]);
    let mut records = RecordTypes::new();
    records.insert(
        "Point".to_string(),
        vec![("x".to_string(), Type::Nat), ("y".to_string(), Type::Nat)],
    );
    let point_types = ComparatorTypes::from_records(records);
    assert!(is_supported(&point, &point_types));
    // Without the record map it is unsupported.
    assert!(!is_supported(&point, &ComparatorTypes::default()));

    let (def, subtypes) = synth_comparator(&point, &point_types).expect("record is synthesizable");
    assert_eq!(def.name, "compare_NamedPoint");
    // AC 8: the record comparator closes over its *field* types (not an anonymous
    // product), so each field's comparator is synthesized.
    assert_eq!(subtypes, vec![Type::Nat, Type::Nat]);
    // AC 8: the body emits source-level `.field` (Field) segments by name, not
    // positional `Pos`. `Field` is `AdtConstruct(PathSeg, 0, "x"/"y")`.
    let src = format!("{:?}", def.term.term);
    assert!(
        src.contains("\"x\""),
        "field path uses the field name `x`: {src}"
    );
    assert!(
        src.contains("\"y\""),
        "field path uses the field name `y`: {src}"
    );
}

#[test]
fn sum_comparator_references_both_arms() {
    let ty = Type::Sum(Box::new(Type::Unit), Box::new(Type::Nat));
    let (def, subtypes) =
        synth_comparator(&ty, &ComparatorTypes::default()).expect("sum is synthesizable");
    assert_eq!(subtypes, vec![Type::Unit, Type::Nat]);
    let Term::Lambda(_, _, inner) = &def.term.term else {
        panic!("not a lambda");
    };
    let Term::Lambda(_, _, body) = inner.as_ref() else {
        panic!("not curried");
    };
    assert!(
        matches!(body.as_ref(), Term::Case(..)),
        "sum body dispatches via Case"
    );
}

#[test]
fn nat_comparator_coredef_shape() {
    let (def, subtypes) =
        synth_comparator(&Type::Nat, &ComparatorTypes::default()).expect("Nat is synthesizable");
    assert_eq!(def.name, "compare_Nat");
    assert!(subtypes.is_empty(), "leaf has no sub-types");
    assert_eq!(
        def.ty,
        Type::Arrow(
            Box::new(Type::Nat),
            Box::new(Type::Arrow(
                Box::new(Type::Nat),
                Box::new(Type::TyVar("CompareResult".to_string()))
            ))
        )
    );
    let Term::Lambda(a, ty_a, _) = &def.term.term else {
        panic!("body not a lambda: {:?}", def.term.term);
    };
    assert_eq!(a, "a");
    assert_eq!(*ty_a, Type::Nat);
    assert!(def.term.span.is_none());
}

#[test]
fn product_comparator_references_field_comparators() {
    let (def, subtypes) = synth_comparator(&product_nat_bool(), &ComparatorTypes::default())
        .expect("(Nat × Bool) is synthesizable");
    assert_eq!(def.name, "compare_PNat_BoolE");
    assert_eq!(subtypes, vec![Type::Nat, Type::Bool]);
    let Term::Lambda(l, _, inner) = &def.term.term else {
        panic!("not a lambda");
    };
    assert_eq!(l, "l");
    let Term::Lambda(r, _, body) = inner.as_ref() else {
        panic!("not curried");
    };
    assert_eq!(r, "r");
    assert!(
        matches!(body.as_ref(), Term::Case(..)),
        "product body should short-circuit via Case, got {:?}",
        body
    );
}

// ===========================================================================
// P3: Comparable<T> diagnostics — path to first incomparable field
// ===========================================================================

#[test]
fn check_comparable_accepts_primitives() {
    let r = ComparatorTypes::default();
    assert_eq!(check_comparable(&Type::Nat, &r), Ok(()));
    assert_eq!(check_comparable(&Type::Bool, &r), Ok(()));
    assert_eq!(check_comparable(&Type::String, &r), Ok(()));
    assert_eq!(check_comparable(&Type::Unit, &r), Ok(()));
}

#[test]
fn check_comparable_accepts_composites() {
    let r = ComparatorTypes::default();
    assert_eq!(check_comparable(&product_nat_bool(), &r), Ok(()));
    assert_eq!(
        check_comparable(&Type::Sum(Box::new(Type::Unit), Box::new(Type::Nat)), &r),
        Ok(())
    );
}

#[test]
fn check_comparable_rejects_opaque_named_type() {
    let r = ComparatorTypes::default();
    let result = check_comparable(&Type::TyVar("Expr".to_string()), &r);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("Expr"),
        "error should mention undefined type: {}",
        err
    );
}

#[test]
fn check_comparable_reports_path_to_product_field() {
    let r = ComparatorTypes::default();
    let ty = Type::Product(
        Box::new(Type::Nat),
        Box::new(Type::TyVar("Opaque".to_string())),
    );
    let result = check_comparable(&ty, &r);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains(".1"),
        "error should point to second field: {}",
        err
    );
}

#[test]
fn check_comparable_rejects_undefined_record() {
    let r = ComparatorTypes::default();
    let ty = Type::App("Point".to_string(), vec![]);
    let result = check_comparable(&ty, &r);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("Point"),
        "error should mention undefined record: {}",
        err
    );
}
