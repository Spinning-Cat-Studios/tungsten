//! Unit tests for lowering-route selection + divergence attribution
//! (ADR 12.7.26c): route applicability per type shape, layout agreement
//! across routes, denotation equality, and route attribution.

use super::*;
use crate::types::CodegenConstructor;
use inkwell::context::Context;
use std::collections::HashMap;

/// Register `Verdict = Pass | Fail(String)` (2 nullary+payload ctors).
fn verdict_lowering(context: &Context) -> TypeLowering<'_> {
    let mut lowering = TypeLowering::new(context);
    let mut adts = HashMap::new();
    adts.insert(
        "Verdict".to_string(),
        (
            vec![],
            vec![
                CodegenConstructor {
                    name: "Pass".to_string(),
                    fields: vec![],
                    index: 0,
                },
                CodegenConstructor {
                    name: "Fail".to_string(),
                    fields: vec![Type::String],
                    index: 1,
                },
            ],
        ),
    );
    lowering.register_adt_types(adts);
    lowering
}

#[test]
fn every_route_lowers_a_two_ctor_adt_to_the_same_blob() {
    let context = Context::create();
    let mut lowering = verdict_lowering(&context);
    let layouts = lowering.route_layouts("Verdict", &[]);
    // Named, App, Structural, FlatAdt all apply to a nullary ADT.
    assert_eq!(layouts.len(), 4, "all four routes apply to a nullary ADT");
    let first = layouts[0].1;
    for (route, layout) in &layouts {
        assert_eq!(*layout, first, "route {} diverged", route.short());
    }
}

#[test]
fn named_route_is_inapplicable_to_a_parameterized_type() {
    let context = Context::create();
    let mut lowering = TypeLowering::new(&context);
    let mut adts = HashMap::new();
    adts.insert(
        "Option".to_string(),
        (
            vec!["T".to_string()],
            vec![
                CodegenConstructor {
                    name: "None".to_string(),
                    fields: vec![],
                    index: 0,
                },
                CodegenConstructor {
                    name: "Some".to_string(),
                    fields: vec![Type::TyVar("T".to_string())],
                    index: 1,
                },
            ],
        ),
    );
    lowering.register_adt_types(adts);
    assert!(
        lowering
            .lower_via_route(Route::Named, "Option", &[Type::String])
            .is_none(),
        "named route must not apply to Option<String>"
    );
    assert!(lowering
        .lower_via_route(Route::App, "Option", &[Type::String])
        .is_some());
}

#[test]
fn flat_adt_route_is_inapplicable_to_a_single_constructor_adt() {
    // A single-ctor ADT lowers to its bare payload (no tag) via named/app/
    // structural; `Type::Adt` is never spelled for it, so the flat-adt
    // route must not be compared — else `lower_adt`'s unconditional blob
    // reports a false divergence (the compiler `EqBaseType`/`Path` shape).
    let context = Context::create();
    let mut lowering = TypeLowering::new(&context);
    let mut adts = HashMap::new();
    adts.insert(
        "Wrapper".to_string(),
        (
            vec![],
            vec![CodegenConstructor {
                name: "Wrap".to_string(),
                fields: vec![Type::Nat, Type::String],
                index: 0,
            }],
        ),
    );
    lowering.register_adt_types(adts);

    assert!(
        lowering
            .lower_via_route(Route::FlatAdt, "Wrapper", &[])
            .is_none(),
        "flat-adt route must not apply to a single-ctor ADT"
    );
    // The three real routes still apply and must agree (bare payload).
    let layouts = lowering.route_layouts("Wrapper", &[]);
    assert_eq!(
        layouts.len(),
        3,
        "named/app/structural apply; flat-adt excluded"
    );
    assert!(!layouts.iter().any(|(r, _)| *r == Route::FlatAdt));
    let first = layouts[0].1;
    assert!(
        layouts.iter().all(|(_, l)| *l == first),
        "real routes must agree"
    );
}

#[test]
fn tyvar_and_structural_sum_are_the_same_denotation() {
    let context = Context::create();
    let lowering = verdict_lowering(&context);
    let named = Type::TyVar("Verdict".to_string());
    let structural = Type::sum(Type::Unit, Type::String);
    assert!(
        lowering.same_denotation(&named, &structural),
        "TyVar(Verdict) and its Sum encoding must denote the same type"
    );
}

#[test]
fn attribute_route_finds_the_route_that_produced_a_layout() {
    let context = Context::create();
    let mut lowering = verdict_lowering(&context);
    let observed = lowering
        .lower_via_route(Route::Structural, "Verdict", &[])
        .unwrap();
    let route = lowering.attribute_route(&Type::TyVar("Verdict".to_string()), observed);
    assert!(
        route.is_some(),
        "a genuine layout must attribute to some route"
    );
}
