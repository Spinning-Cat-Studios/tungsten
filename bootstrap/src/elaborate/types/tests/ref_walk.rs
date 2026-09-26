//! Unit tests for the unified type-reference walker (ADR 23.7.26a).
//!
//! These pin the *strategy asymmetry* — the single behavioural difference
//! between the Phase-1d and encoding-path resolvers (ADR 22.7.26d) — at the
//! walker level, plus the shared structural default via `map_children`.

use std::collections::HashSet;

use crate::ast::Visibility;
use crate::elaborate::env::{TypeDef, TypeDefKind};
use crate::elaborate::types::ref_walk::TypeRefStrategy;
use crate::elaborate::Elaborator;
use tungsten_core::Type;

use super::resolve_refs::{dummy_span, make_elaborator, register_list_adt};

/// Register `X` as a simple alias to `Nat` under the given (possibly
/// `@`-prefixed) name.
fn register_nat_alias(elab: &mut Elaborator<'_>, name: &str) {
    elab.env.define_type(TypeDef {
        name: name.to_string(),
        params: vec![],
        kind: TypeDefKind::Alias(Type::Nat),
        visibility: Visibility::Public,
        span: dummy_span(),
        defining_module: None,
        encoded_type: None,
        field_visibilities: Vec::new(),
    });
}

/// The load-bearing asymmetry (ADR 22.7.26d): Deferred-TyVar Resolution strips `@` and
/// resolves a deferred `TyVar("@X")`; the encoding strategy looks up bare
/// names only, so the same `@`-ref embeds unresolved.
#[test]
fn test_at_ref_resolves_under_deferred_but_not_encoding() {
    let mut elab = make_elaborator();
    register_nat_alias(&mut elab, "X");

    let at_ref = Type::TyVar("@X".to_string());

    let mut stack = HashSet::new();
    let deferred = elab.walk_type_refs(&at_ref, TypeRefStrategy::Deferred, &mut stack);
    assert_eq!(
        deferred,
        Type::Nat,
        "Deferred-TyVar Resolution strips @ and resolves"
    );

    let mut stack = HashSet::new();
    let encoding = elab.walk_type_refs(&at_ref, TypeRefStrategy::Encoding, &mut stack);
    assert_eq!(encoding, at_ref, "encoding path must NOT resolve an @-ref");
}

/// Both strategies resolve a bare `TyVar("X")` to the alias body.
#[test]
fn test_bare_ref_resolves_under_both_strategies() {
    let mut elab = make_elaborator();
    register_nat_alias(&mut elab, "X");

    let bare_ref = Type::TyVar("X".to_string());
    for strategy in [TypeRefStrategy::Deferred, TypeRefStrategy::Encoding] {
        let mut stack = HashSet::new();
        let resolved = elab.walk_type_refs(&bare_ref, strategy, &mut stack);
        assert_eq!(resolved, Type::Nat);
    }
}

/// The Phase-1d in-stack guard checks the `@`-STRIPPED name: with "X" on the
/// stack, `TyVar("@X")` must stay frozen (cycle), not resolve.
#[test]
fn test_deferred_guard_checks_stripped_name() {
    let mut elab = make_elaborator();
    register_nat_alias(&mut elab, "X");

    let at_ref = Type::TyVar("@X".to_string());
    let mut stack = HashSet::new();
    stack.insert("X".to_string());
    let result = elab.walk_type_refs(&at_ref, TypeRefStrategy::Deferred, &mut stack);
    assert_eq!(result, at_ref, "in-stack @-ref must stay frozen");
}

/// The encoding in-stack guard checks the bare AND the stripped spelling:
/// either one on the stack freezes the ref. (The env deliberately holds a
/// type under the literal name "@X" so guard-vs-resolve outcomes differ —
/// a guard-semantics pin, not a scenario reachable from parsed source.)
#[test]
fn test_encoding_guard_checks_bare_and_stripped_names() {
    let mut elab = make_elaborator();
    register_nat_alias(&mut elab, "@X");
    register_nat_alias(&mut elab, "X");

    let at_ref = Type::TyVar("@X".to_string());

    // Stripped spelling on the stack → frozen.
    let mut stack = HashSet::new();
    stack.insert("X".to_string());
    let result = elab.walk_type_refs(&at_ref, TypeRefStrategy::Encoding, &mut stack);
    assert_eq!(result, at_ref, "stripped-name stack entry must freeze");

    // Bare spelling on the stack → frozen.
    let mut stack = HashSet::new();
    stack.insert("@X".to_string());
    let result = elab.walk_type_refs(&at_ref, TypeRefStrategy::Encoding, &mut stack);
    assert_eq!(result, at_ref, "bare-name stack entry must freeze");

    // Nothing on the stack → the literal "@X" env entry resolves.
    let mut stack = HashSet::new();
    let result = elab.walk_type_refs(&at_ref, TypeRefStrategy::Encoding, &mut stack);
    assert_eq!(result, Type::Nat);
}

/// Structural arms are shared via `map_children`: the walker recurses into
/// Product / Adt / Eq children under either strategy, applying that
/// strategy's name-arm behaviour at the leaves.
#[test]
fn test_structural_default_recurses_with_strategy_at_leaves() {
    let mut elab = make_elaborator();
    register_nat_alias(&mut elab, "X");

    let product = Type::product(Type::TyVar("@X".to_string()), Type::TyVar("X".to_string()));

    let mut stack = HashSet::new();
    let deferred = elab.walk_type_refs(&product, TypeRefStrategy::Deferred, &mut stack);
    assert_eq!(deferred, Type::product(Type::Nat, Type::Nat));

    let mut stack = HashSet::new();
    let encoding = elab.walk_type_refs(&product, TypeRefStrategy::Encoding, &mut stack);
    assert_eq!(
        encoding,
        Type::product(Type::TyVar("@X".to_string()), Type::Nat),
        "@-ref embeds unresolved on the encoding path, bare ref resolves"
    );

    let adt = Type::adt(
        "T",
        vec![Type::TyVar("X".to_string())],
        vec![("A".to_string(), Type::TyVar("X".to_string()))],
    );
    let mut stack = HashSet::new();
    let resolved_adt = elab.walk_type_refs(&adt, TypeRefStrategy::Encoding, &mut stack);
    assert_eq!(
        resolved_adt,
        Type::adt("T", vec![Type::Nat], vec![("A".to_string(), Type::Nat)]),
        "Adt type args AND variant payloads recurse via map_children"
    );
}

/// A cycle-detected `App` (head on the stack) keeps its head but still
/// resolves its args — the `map_children` fallthrough, matching both
/// original walkers' explicit cycle arms.
#[test]
fn test_cycle_detected_app_resolves_args_only() {
    let mut elab = make_elaborator();
    register_list_adt(&mut elab);
    register_nat_alias(&mut elab, "X");

    let app = Type::app("List", vec![Type::TyVar("X".to_string())]);
    for strategy in [TypeRefStrategy::Deferred, TypeRefStrategy::Encoding] {
        let mut stack = HashSet::new();
        stack.insert("List".to_string());
        let result = elab.walk_type_refs(&app, strategy, &mut stack);
        assert_eq!(
            result,
            Type::app("List", vec![Type::Nat]),
            "in-stack App keeps head, resolves args"
        );
    }
}

/// A free `App` head is expanded by the strategy, with args resolved before
/// the head — and the strategies' `App` helpers genuinely differ (the doc
/// table in `ref_walk.rs`): `Encoding` encodes the ADT to its μ-type, while
/// `Deferred`'s `resolve_tyvars_app` pre-inserts the head into the stack
/// around the whole expansion, so the self-encode is cycle-frozen and falls
/// back to an `App` with resolved args. Both observables match the
/// pre-unification walkers byte-for-byte.
#[test]
fn test_free_app_expands_per_strategy_with_resolved_args() {
    let mut elab = make_elaborator();
    register_list_adt(&mut elab);
    register_nat_alias(&mut elab, "X");

    let app = Type::app("List", vec![Type::TyVar("X".to_string())]);

    let mut stack = HashSet::new();
    let encoding = elab.walk_type_refs(&app, TypeRefStrategy::Encoding, &mut stack);
    assert!(
        matches!(encoding, Type::Mu(..)),
        "encoding path must μ-encode free List<X>, got {encoding:?}"
    );
    assert!(
        !format!("{encoding:?}").contains("TyVar(\"X\")"),
        "the arg must be resolved (no free TyVar X) in {encoding:?}"
    );

    let mut stack = HashSet::new();
    let deferred = elab.walk_type_refs(&app, TypeRefStrategy::Deferred, &mut stack);
    assert_eq!(
        deferred,
        Type::app("List", vec![Type::Nat]),
        "Deferred-TyVar Resolution pre-inserts the head, freezing the self-encode: App stays, args resolve"
    );
}
