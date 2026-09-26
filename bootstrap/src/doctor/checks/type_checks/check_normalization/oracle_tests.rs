//! Tests for the per-module normalization oracle (ADR 22.7.26b): the wall-1
//! positive fixture and the two-tier stored-vs-fresh comparison. Split from
//! `tests.rs` (file-size gate); the pre-oracle check tests stay there.

use super::*;
use crate::elaborate::{ModuleExports, TypeDef, TypeDefKind};
use std::fs;
use tempfile::TempDir;

/// The resurrected wall-1 fixture (ADR 22.7.26b AC3; ADR 21.7.26e wall 1):
/// a record holding a generic instantiation in a field (`StrMap<CtorBucket>`)
/// — the exact type class the 21.7.26j whole-project check had to *skip*.
/// Under the per-module oracle it is checkable: the source-fresh re-collected
/// encoding exists, matches the healthy stored encoding (Consistent), and a
/// deliberately-corrupted stored encoding is caught as DIVERGENT — proving
/// the oracle detects the bug class it targets rather than skipping it.
#[test]
fn per_module_oracle_catches_corrupted_record_with_generic_field() {
    let dir = TempDir::new().unwrap();
    let shapes = dir.path().join("shapes.tg");
    fs::write(
        &shapes,
        concat!(
            "pub type StrMap<V> = SMEmpty | SMNode(String, V, StrMap<V>)\n",
            "pub type CtorBucket = { count: Nat }\n",
            "pub type CtorIndex = { buckets: StrMap<CtorBucket> }\n",
            "pub fn bucket_count(b: CtorBucket) -> Nat { b.count }\n",
        ),
    )
    .unwrap();
    let main = dir.path().join("main.tg");
    fs::write(
        &main,
        concat!(
            "pub mod shapes;\n",
            "use shapes::{StrMap, SMEmpty, SMNode, CtorBucket, CtorIndex, bucket_count};\n",
            "fn main() -> Nat { bucket_count(CtorBucket { count: 1 }) }\n",
        ),
    )
    .unwrap();

    let mut healthy: Option<LiveVerdict> = None;
    let mut corrupted: Option<LiveVerdict> = None;
    let result = driver::elaborate_project_with_inspector(&main, false, 20, &mut |normalizer| {
        let stored = normalizer
            .stored_encodings()
            .get("CtorIndex")
            .expect("CtorIndex must have a stored Phase-1e encoding")
            .clone();
        assert!(
            normalizer.per_module_fresh("CtorIndex").is_some(),
            "the record with a generic-instantiation field must be checkable \
             (present in the per-module fresh set), not a skip"
        );
        healthy = Some(classify_type_live(
            normalizer,
            "CtorIndex",
            &stored,
            CompareOpts {
                verbose: false,
                raw_only: false,
            },
        ));
        // A corrupted stored encoding: an extra Product layer no honest
        // re-derivation produces. Tier-2 normalization must NOT erase it.
        let corrupted_stored = Type::product(stored, Type::Nat);
        corrupted = Some(classify_type_live(
            normalizer,
            "CtorIndex",
            &corrupted_stored,
            CompareOpts {
                verbose: false,
                raw_only: false,
            },
        ));
    });
    assert!(result.is_ok(), "fixture project must elaborate");
    assert_eq!(healthy, Some(LiveVerdict::Consistent));
    assert_eq!(
        corrupted,
        Some(LiveVerdict::Divergent),
        "a corrupted stored encoding must be caught as DIVERGENT"
    );
}

/// Tier 2 of the per-module comparison (ADR 22.7.26b): named references can
/// be spelled at different inline depths by two honest derivations — the
/// same type as `TyVar(@Inner)` on one side and as Inner's full encoding on
/// the other. ADR 22.7.26c made the *stored* side deterministic (Phase-1e
/// encode order) and ADR 22.7.26d fixed the fresh side's Phase-1d
/// resolution order, so both sides now converge and tier 2 is
/// defense-in-depth rather than load-bearing. It must still behave as
/// specified — erase an inline-depth skew (should one ever reappear) while
/// a genuine structural mismatch stays divergent.
#[test]
fn per_module_comparison_tier2_erases_inline_depth_but_keeps_corruption() {
    let inner_encoding = Type::sum(Type::Unit, Type::Nat);
    let exports = ModuleExports {
        types: vec![(
            "Inner".to_string(),
            TypeDef {
                name: "Inner".to_string(),
                params: vec![],
                kind: TypeDefKind::Alias(inner_encoding.clone()),
                visibility: crate::ast::Visibility::Public,
                span: crate::span::Span::new(0, 0),
                defining_module: None,
                encoded_type: Some(inner_encoding.clone()),
                field_visibilities: Vec::new(),
            },
        )],
        values: vec![],
        constructors: vec![],
    };
    let mut ctx = tungsten_core::Context::new();
    let normalizer = ProjectNormalizer::seeded(&mut ctx, &exports, HashMap::new(), HashMap::new());

    // Same type, different inline depth: consistent via tier-2 normalization.
    let stored_inlined = inner_encoding;
    let fresh_deferred = Type::TyVar("@Inner".to_string());
    assert_eq!(
        classify_against_per_module_fresh(
            &normalizer,
            "Outer",
            &stored_inlined,
            &fresh_deferred,
            CompareOpts {
                verbose: false,
                raw_only: false
            },
        ),
        LiveVerdict::Consistent
    );

    // Genuinely different structure: divergent even after normalization.
    let stored_corrupt = Type::sum(Type::Unit, Type::String);
    assert_eq!(
        classify_against_per_module_fresh(
            &normalizer,
            "Outer",
            &stored_corrupt,
            &fresh_deferred,
            CompareOpts {
                verbose: false,
                raw_only: false
            },
        ),
        LiveVerdict::Divergent
    );
}

/// `--raw-only` (ADR 22.7.26c) drops tier 2: an inline-depth difference that
/// tier 2 would absorb is instead reported DIVERGENT. Since ADR 22.7.26d the
/// live pipeline no longer produces such differences (tier 1 alone measures
/// 0 divergent on main.tg), so the flag is a regression canary for the
/// retired instability — this test feeds it a synthetic depth skew.
#[test]
fn raw_only_reports_inline_depth_difference_as_divergent() {
    let inner_encoding = Type::sum(Type::Unit, Type::Nat);
    let exports = ModuleExports {
        types: vec![(
            "Inner".to_string(),
            TypeDef {
                name: "Inner".to_string(),
                params: vec![],
                kind: TypeDefKind::Alias(inner_encoding.clone()),
                visibility: crate::ast::Visibility::Public,
                span: crate::span::Span::new(0, 0),
                defining_module: None,
                encoded_type: Some(inner_encoding.clone()),
                field_visibilities: Vec::new(),
            },
        )],
        values: vec![],
        constructors: vec![],
    };
    let mut ctx = tungsten_core::Context::new();
    let normalizer = ProjectNormalizer::seeded(&mut ctx, &exports, HashMap::new(), HashMap::new());

    let stored_inlined = inner_encoding;
    let fresh_deferred = Type::TyVar("@Inner".to_string());

    // raw_only = false → tier 2 makes it Consistent (the default behavior).
    assert_eq!(
        classify_against_per_module_fresh(
            &normalizer,
            "Outer",
            &stored_inlined,
            &fresh_deferred,
            CompareOpts {
                verbose: false,
                raw_only: false
            },
        ),
        LiveVerdict::Consistent
    );

    // raw_only = true → tier 2 skipped, raw `==` fails → Divergent.
    assert_eq!(
        classify_against_per_module_fresh(
            &normalizer,
            "Outer",
            &stored_inlined,
            &fresh_deferred,
            CompareOpts {
                verbose: false,
                raw_only: true
            },
        ),
        LiveVerdict::Divergent
    );
}
