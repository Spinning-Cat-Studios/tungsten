//! Tests for the E0010 note gate — when the enrichment fires and what it says
//! (ADR 21.7.26f / D2). The tree walk it sits on top of is tested separately in
//! `tests_structural_divergence.rs`. Tests: <this file>.

use super::tests::{differing_below_the_display_depth, resolved_vs_unresolved};
use super::{
    identical_render_note, truncate_to_budget, Divergence, MAX_RENDER_CHARS, MAX_RENDER_DEPTH,
};
use tungsten_core::Type;

/// Assert the fixture really is an identical-render pair — otherwise the tests
/// below would pass for the wrong reason.
fn assert_renders_match(a: &Type, b: &Type) {
    assert_eq!(
        crate::driver::output::format_type_for_display(a),
        crate::driver::output::format_type_for_display(b),
        "fixture precondition: the two types must render the same"
    );
}

#[test]
fn differently_rendering_types_produce_no_note() {
    // The zero-noise guarantee: an ordinary mismatch must be untouched.
    assert_eq!(identical_render_note(&Type::Nat, &Type::Bool), None);
    assert_eq!(
        identical_render_note(
            &Type::Arrow(Box::new(Type::Nat), Box::new(Type::Nat)),
            &Type::Arrow(Box::new(Type::Bool), Box::new(Type::Bool)),
        ),
        None
    );
}

#[test]
fn structurally_identical_types_produce_no_note() {
    // Nothing is hidden, so there is nothing to explain.
    assert_eq!(identical_render_note(&Type::Nat, &Type::Nat), None);
}

#[test]
fn identically_rendering_but_divergent_types_produce_a_note() {
    let (resolved, unresolved) = resolved_vs_unresolved();
    assert_renders_match(&resolved, &unresolved);

    let note = identical_render_note(&resolved, &unresolved).expect("a note is warranted");
    assert!(
        note.contains("render identically but differ structurally"),
        "{note}"
    );
    assert!(note.contains("type argument #1 of `StrMap`"), "{note}");
    assert!(note.contains("expected: Adt("), "{note}");
    assert!(note.contains("found:    App("), "{note}");
}

#[test]
fn a_divergence_hidden_by_display_truncation_is_still_located() {
    // Both sides print `Outer<Middle<Inner<...>>>`; the note must name the
    // route the `...` swallowed.
    let (a, b) = differing_below_the_display_depth();
    assert_renders_match(&a, &b);

    let note = identical_render_note(&a, &b).expect("a note is warranted");
    assert!(note.contains("type argument #1 of `Outer`"), "{note}");
    assert!(note.contains("type argument #1 of `Inner`"), "{note}");
    assert!(note.contains("expected: Nat"), "{note}");
    assert!(note.contains("found:    Bool"), "{note}");
}

// ── output bounding (ADR 21.7.26f follow-up F1) ──────────────────────────────
//
// The note exists to make a mismatch readable. An unbounded structural render
// of the stored encodings it fires on would defeat that, so both nesting depth
// and total length are capped.

/// An ADT with `n` constructors — shallow, but arbitrarily wide.
fn wide_adt(n: usize, payload: Type) -> Type {
    Type::Adt(
        "Token".to_string(),
        vec![],
        (0..n)
            .map(|i| (format!("Ctor{i}"), payload.clone()))
            .collect(),
    )
}

/// A type nested `n` levels deep.
fn deep_chain(n: usize, leaf: Type) -> Type {
    (0..n).fold(leaf, |acc, _| Type::Ptr(Box::new(acc)))
}

// NB the walk descends *to* the divergence, so a large type only reaches the
// renderer when the divergence sits AT it — i.e. when the two sides have
// different shapes at that position. Pairing two large types that differ deep
// inside would render only their small differing leaves, and would test
// nothing.

#[test]
fn a_wide_type_at_the_divergence_is_cut_to_the_length_budget() {
    // Breadth is what depth-capping alone cannot bound: 400 constructors sit at
    // depth 1 and still run to thousands of characters.
    let wide = wide_adt(400, Type::Nat);
    assert!(
        wide.display_detailed().chars().count() > 1_000,
        "fixture precondition: the unbounded render must actually be huge"
    );

    let d = super::first_divergence(&wide, &Type::Nat).expect("shapes differ at the root");
    assert!(
        d.expected.chars().count() <= MAX_RENDER_CHARS + 1, // +1 for the ellipsis
        "expected side ran to {} chars: {}",
        d.expected.chars().count(),
        d.expected
    );
}

#[test]
fn a_bounded_render_marks_where_it_elided() {
    let wide = wide_adt(400, Type::Nat);
    let d = super::first_divergence(&wide, &Type::Nat).expect("shapes differ at the root");
    assert!(
        d.expected.ends_with('…'),
        "a truncated render must say it was truncated: {}",
        d.expected
    );
}

#[test]
fn structure_below_the_render_depth_is_elided() {
    let deep = deep_chain(MAX_RENDER_DEPTH + 6, Type::Nat);
    let d = super::first_divergence(&deep, &Type::Nat).expect("shapes differ at the root");
    assert!(
        d.expected.contains('…'),
        "nesting past the depth cap must elide: {}",
        d.expected
    );
}

#[test]
fn a_divergence_past_the_walk_depth_cap_is_still_bounded() {
    // The other way a large type reaches the renderer: the walk gives up at
    // MAX_DIVERGENCE_DEPTH and reports whatever subtree it stopped on.
    let a = deep_chain(40, Type::Nat);
    let b = deep_chain(40, Type::Bool);
    let d = super::first_divergence(&a, &b).expect("leaves differ");
    assert_eq!(d.path.len(), super::MAX_DIVERGENCE_DEPTH);
    assert!(
        d.expected.chars().count() <= MAX_RENDER_CHARS + 1,
        "expected side ran to {} chars",
        d.expected.chars().count()
    );
}

#[test]
fn a_small_type_is_rendered_in_full() {
    // The bound must not disturb the common case it was added to protect.
    let d = super::first_divergence(&Type::Nat, &Type::Bool).expect("differ");
    assert_eq!(d.expected, "Nat");
    assert_eq!(d.found, "Bool");
}

#[test]
fn truncation_counts_characters_not_bytes() {
    // Type spellings carry multi-byte characters (`α_List`, `μ`, `…`); a byte
    // cut would panic or emit mojibake.
    let text = "α".repeat(20);
    let cut = truncate_to_budget(text, 5);
    assert_eq!(cut, "ααααα…");
}

#[test]
fn truncation_leaves_text_within_budget_untouched() {
    assert_eq!(truncate_to_budget("Nat".to_string(), 160), "Nat");
    // Exactly at budget is not over budget — no ellipsis.
    assert_eq!(truncate_to_budget("abc".to_string(), 3), "abc");
}

#[test]
fn a_root_divergence_note_says_top_level() {
    let d = Divergence {
        path: vec![],
        expected: "Nat".to_string(),
        found: "Bool".to_string(),
    };
    // Root divergences have no route to describe, so the note says so rather
    // than printing an empty "first divergence at :".
    assert_eq!(d.location(), "at the top level");
}

#[test]
fn a_nested_divergence_note_joins_the_route() {
    let d = Divergence {
        path: vec![
            "the parameter type".into(),
            "the right of the product".into(),
        ],
        expected: "Nat".to_string(),
        found: "Bool".to_string(),
    };
    assert_eq!(
        d.location(),
        "at the parameter type → the right of the product"
    );
}
