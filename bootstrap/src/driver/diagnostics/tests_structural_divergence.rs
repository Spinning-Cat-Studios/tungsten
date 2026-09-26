//! Tests for identical-render E0010 enrichment (ADR 21.7.26f / D2).
//!
//! This file covers the tree walk (`first_divergence`); the note gate built on
//! top of it is covered in `tests_structural_divergence_note.rs`. The whole
//! thing is a pure function of the two `Type` trees, so it needs no live
//! compiler bug to drive it. Tests: <this file>.

use super::{first_divergence, MAX_DIVERGENCE_DEPTH};
use tungsten_core::Type;

fn nat() -> Type {
    Type::Nat
}

fn product(a: Type, b: Type) -> Type {
    Type::Product(Box::new(a), Box::new(b))
}

/// A `StrMap<CtorBucket>` pair in the 21.7.26c shape: identical name and
/// arity, but one side's argument is the *resolved* ADT (carrying its
/// variants) where the other's is an unresolved application. Both render as
/// `StrMap<CtorBucket>` because `format_type_for_display` renders `App` and
/// `Adt` through the same `name<args>` path — the display collapses exactly
/// the distinction that matters.
pub(super) fn resolved_vs_unresolved() -> (Type, Type) {
    let resolved_arg = Type::Adt(
        "CtorBucket".to_string(),
        vec![],
        vec![("Bucket".to_string(), product(Type::String, Type::Nat))],
    );
    let unresolved_arg = Type::App("CtorBucket".to_string(), vec![]);
    (
        Type::App("StrMap".to_string(), vec![resolved_arg]),
        Type::App("StrMap".to_string(), vec![unresolved_arg]),
    )
}

/// A pair that agrees down to the display-depth cut-off and differs below it —
/// the other way identical renders arise: `format_type_for_display` truncates
/// past `MAX_TYPE_DISPLAY_DEPTH`, so the difference is literally printed as
/// `...` on both sides.
pub(super) fn differing_below_the_display_depth() -> (Type, Type) {
    let deep = |leaf: Type| {
        Type::App(
            "Outer".to_string(),
            vec![Type::App(
                "Middle".to_string(),
                vec![Type::App("Inner".to_string(), vec![leaf])],
            )],
        )
    };
    (deep(Type::Nat), deep(Type::Bool))
}

// ── the walk ────────────────────────────────────────────────────────────────

#[test]
fn structurally_equal_trees_have_no_divergence() {
    assert_eq!(first_divergence(&nat(), &nat()), None);
    let big = product(Type::Bool, product(Type::String, nat()));
    assert_eq!(first_divergence(&big, &big), None);
}

#[test]
fn a_root_level_difference_reports_an_empty_path() {
    let d = first_divergence(&Type::Nat, &Type::Bool).expect("Nat and Bool differ");
    assert!(
        d.path.is_empty(),
        "root divergence has no route: {:?}",
        d.path
    );
    assert_eq!(d.expected, "Nat");
    assert_eq!(d.found, "Bool");
}

#[test]
fn arrow_positions_are_named_distinctly() {
    let param = first_divergence(
        &Type::Arrow(Box::new(Type::Nat), Box::new(Type::Bool)),
        &Type::Arrow(Box::new(Type::String), Box::new(Type::Bool)),
    )
    .expect("parameter types differ");
    assert_eq!(param.path, vec!["the parameter type"]);

    let ret = first_divergence(
        &Type::Arrow(Box::new(Type::Nat), Box::new(Type::Bool)),
        &Type::Arrow(Box::new(Type::Nat), Box::new(Type::String)),
    )
    .expect("return types differ");
    assert_eq!(ret.path, vec!["the return type"]);
}

#[test]
fn the_parameter_side_is_reported_before_the_return_side() {
    // "First divergence" must mean leftmost-outermost, or the note points the
    // reader at the wrong end of the type.
    let d = first_divergence(
        &Type::Arrow(Box::new(Type::Nat), Box::new(Type::Nat)),
        &Type::Arrow(Box::new(Type::Bool), Box::new(Type::Bool)),
    )
    .expect("both sides differ");
    assert_eq!(d.path, vec!["the parameter type"]);
}

#[test]
fn product_and_sum_sides_are_named_distinctly() {
    let left = first_divergence(
        &product(Type::Nat, Type::Bool),
        &product(Type::String, Type::Bool),
    )
    .expect("left sides differ");
    assert_eq!(left.path, vec!["the left of the product"]);

    let right = first_divergence(
        &Type::Sum(Box::new(Type::Nat), Box::new(Type::Bool)),
        &Type::Sum(Box::new(Type::Nat), Box::new(Type::String)),
    )
    .expect("right sides differ");
    assert_eq!(right.path, vec!["the right of the sum"]);
}

#[test]
fn nested_paths_accumulate_outermost_first() {
    let d = first_divergence(
        &Type::Arrow(
            Box::new(product(Type::Nat, Type::Bool)),
            Box::new(Type::Unit),
        ),
        &Type::Arrow(
            Box::new(product(Type::Nat, Type::String)),
            Box::new(Type::Unit),
        ),
    )
    .expect("nested right-of-product differs");
    assert_eq!(
        d.path,
        vec!["the parameter type", "the right of the product"]
    );
}

#[test]
fn type_arguments_are_numbered_from_one() {
    let (resolved, unresolved) = resolved_vs_unresolved();
    let d = first_divergence(&resolved, &unresolved).expect("the argument differs");
    assert_eq!(d.path, vec!["type argument #1 of `StrMap`"]);
    assert!(
        d.expected.starts_with("Adt("),
        "expected side should show the resolved ADT: {}",
        d.expected
    );
    assert!(
        d.found.starts_with("App("),
        "found side should show the unresolved application: {}",
        d.found
    );
}

#[test]
fn differing_constructor_names_are_reported_as_such() {
    let a = Type::Adt(
        "Result".to_string(),
        vec![],
        vec![("Ok".to_string(), Type::Nat)],
    );
    let b = Type::Adt(
        "Result".to_string(),
        vec![],
        vec![("Okay".to_string(), Type::Nat)],
    );
    let d = first_divergence(&a, &b).expect("constructor names differ");
    assert_eq!(d.path, vec!["a constructor name"]);
    assert_eq!(d.expected, "Ok");
    assert_eq!(d.found, "Okay");
}

#[test]
fn a_constructor_payload_divergence_names_the_constructor() {
    let a = Type::Adt(
        "Result".to_string(),
        vec![],
        vec![("Ok".to_string(), Type::Nat)],
    );
    let b = Type::Adt(
        "Result".to_string(),
        vec![],
        vec![("Ok".to_string(), Type::Bool)],
    );
    let d = first_divergence(&a, &b).expect("payloads differ");
    assert_eq!(d.path, vec!["the payload of `Ok`"]);
}

#[test]
fn a_differing_mu_binder_is_itself_the_divergence() {
    // Rebinding under a different variable is a whole-type difference, not a
    // difference *inside* the body — descending would mislead.
    let a = Type::Mu("α_A".to_string(), Box::new(Type::Unit));
    let b = Type::Mu("α_B".to_string(), Box::new(Type::Unit));
    let d = first_divergence(&a, &b).expect("binders differ");
    assert!(d.path.is_empty());
}

#[test]
fn a_matching_mu_binder_descends_into_the_body() {
    let a = Type::Mu("α_L".to_string(), Box::new(Type::Nat));
    let b = Type::Mu("α_L".to_string(), Box::new(Type::Bool));
    let d = first_divergence(&a, &b).expect("bodies differ");
    assert_eq!(d.path, vec!["the μ body"]);
}

#[test]
fn a_matching_forall_binder_descends_into_the_body() {
    let a = Type::Forall("T".to_string(), Box::new(Type::Nat));
    let b = Type::Forall("T".to_string(), Box::new(Type::Bool));
    let d = first_divergence(&a, &b).expect("bodies differ");
    assert_eq!(d.path, vec!["the ∀ body"]);
}

#[test]
fn differently_named_apps_diverge_at_the_application_itself() {
    // Different heads mean the shapes don't correspond; there is no meaningful
    // per-argument route to report.
    let a = Type::App("StrMap".to_string(), vec![Type::Nat]);
    let b = Type::App("IntMap".to_string(), vec![Type::Nat]);
    let d = first_divergence(&a, &b).expect("heads differ");
    assert!(d.path.is_empty());
}

#[test]
fn apps_of_differing_arity_diverge_at_the_application_itself() {
    let a = Type::App("Pair".to_string(), vec![Type::Nat]);
    let b = Type::App("Pair".to_string(), vec![Type::Nat, Type::Bool]);
    let d = first_divergence(&a, &b).expect("arities differ");
    assert!(d.path.is_empty());
}

/// Build an ADT with one `Bucket` variant of the given payload.
fn adt(name: &str, type_args: Vec<Type>, variants: Vec<(String, Type)>) -> Type {
    Type::Adt(name.to_string(), type_args, variants)
}

#[test]
fn differently_named_adts_diverge_at_the_adt_itself() {
    // The guard must reject a name mismatch: pairing `Ok` in `Result` against
    // `Ok` in `Either` and reporting "the payload of `Ok`" would describe a
    // correspondence that does not exist.
    let a = adt("Result", vec![], vec![("Ok".to_string(), Type::Nat)]);
    let b = adt("Either", vec![], vec![("Ok".to_string(), Type::Nat)]);
    let d = first_divergence(&a, &b).expect("names differ");
    assert!(d.path.is_empty(), "unexpected route: {:?}", d.path);
    assert!(d.expected.starts_with("Adt(Result"), "{}", d.expected);
    assert!(d.found.starts_with("Adt(Either"), "{}", d.found);
}

#[test]
fn adts_of_differing_type_arity_diverge_at_the_adt_itself() {
    let a = adt(
        "Box",
        vec![Type::Nat],
        vec![("Wrap".to_string(), Type::Nat)],
    );
    let b = adt(
        "Box",
        vec![Type::Nat, Type::Bool],
        vec![("Wrap".to_string(), Type::Nat)],
    );
    let d = first_divergence(&a, &b).expect("type arities differ");
    assert!(d.path.is_empty(), "unexpected route: {:?}", d.path);
}

#[test]
fn adts_of_differing_variant_count_diverge_at_the_adt_itself() {
    // Zipping variant lists of different lengths would silently ignore the
    // extra constructor and could report "no divergence" for types that differ.
    let a = adt("Result", vec![], vec![("Ok".to_string(), Type::Nat)]);
    let b = adt(
        "Result",
        vec![],
        vec![
            ("Ok".to_string(), Type::Nat),
            ("Err".to_string(), Type::String),
        ],
    );
    let d = first_divergence(&a, &b).expect("variant counts differ");
    assert!(d.path.is_empty(), "unexpected route: {:?}", d.path);
}

#[test]
fn ptr_and_ref_positions_are_named() {
    let ptr = first_divergence(
        &Type::Ptr(Box::new(Type::Nat)),
        &Type::Ptr(Box::new(Type::Bool)),
    )
    .expect("pointees differ");
    assert_eq!(ptr.path, vec!["the pointee"]);

    let r = first_divergence(
        &Type::Ref(Box::new(Type::Nat)),
        &Type::Ref(Box::new(Type::Bool)),
    )
    .expect("referents differ");
    assert_eq!(r.path, vec!["the referent"]);
}

#[test]
fn the_walk_stops_at_the_depth_cap() {
    // Beyond the cap a path stops being readable, so the walk reports where it
    // stopped rather than descending forever.
    let mut deep_a = Type::Nat;
    let mut deep_b = Type::Bool;
    for _ in 0..(MAX_DIVERGENCE_DEPTH + 5) {
        deep_a = Type::Ptr(Box::new(deep_a));
        deep_b = Type::Ptr(Box::new(deep_b));
    }
    let d = first_divergence(&deep_a, &deep_b).expect("the leaves differ");
    assert_eq!(d.path.len(), MAX_DIVERGENCE_DEPTH);
}
