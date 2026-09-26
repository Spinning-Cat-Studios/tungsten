//! Tests for `info type members constructors --raw` (ADR 1.8.26c retrospective).
//!
//! The rendering is a pure function over constructor lists, so these need no
//! filesystem, no elaborated project and no captured stdout.

use super::*;
use tungsten_bootstrap::span::Span;

fn ctor(name: &str, index: usize, fields: Vec<Type>) -> Constructor {
    Constructor {
        name: name.to_string(),
        fields,
        index,
        visibility: None,
        span: Span::default(),
    }
}

/// The defect `--raw` exists for: `Display` **strips the Type-Body Collection
/// `@`-prefix**, so the default listing cannot answer "does this field name the
/// type directly, or through a still-deferred reference?".
///
/// That is not a cosmetic difference. `records()` and `encoded_types` are keyed
/// without the prefix, so an `@` that survives into a walk is reported as
/// `@Ident is not defined` about a record the project defines — the entire
/// 81-failure residual ADR 1.8.26c measured after its instantiation arm landed.
///
/// Asserted as a *premise*, not an aspiration: if `Display` ever grew the
/// distinction this test fails and `--raw`'s justification shrinks. The
/// zero-argument `App` is deliberately asserted the other way — `Display`
/// **does** separate it (`List<>` vs `List`), correcting the wider claim the
/// 1.8.26c retrospective originally made.
#[test]
fn display_strips_the_at_prefix_but_keeps_the_application_brackets() {
    let bare = Type::TyVar("List".into());
    let prefixed = Type::TyVar("@List".into());
    let application = Type::app("List", vec![]);

    assert_eq!(
        format!("{bare}"),
        format!("{prefixed}"),
        "the `@`-prefix is invisible in the default listing — this is what --raw recovers"
    );
    assert_ne!(
        format!("{bare}"),
        format!("{application}"),
        "a zero-argument App is already distinguishable; --raw is not needed for it"
    );
}

/// …and `--raw` separates them, by both spelling label and debug form.
#[test]
fn raw_separates_what_display_collapses() {
    let rendered = render_raw_fields(&[ctor(
        "C",
        0,
        vec![
            Type::TyVar("List".into()),
            Type::TyVar("@List".into()),
            Type::app("List", vec![]),
        ],
    )]);

    assert!(
        rendered.contains("C.0 tyvar := TyVar(\"List\")"),
        "{rendered}"
    );
    assert!(
        rendered.contains("C.1 at-tyvar := TyVar(\"@List\")"),
        "{rendered}"
    );
    assert!(
        rendered.contains("C.2 app/0 := App(\"List\", [])"),
        "{rendered}"
    );
}

/// The labels are this command's own vocabulary, so the output explains them.
/// Without the legend a reader meeting `app/0` has to find the source to learn
/// what it means — which is the friction `--raw` exists to remove, reappearing
/// one level up.
#[test]
fn the_legend_defines_every_label_the_output_can_emit() {
    let rendered = render_raw_fields(&[ctor("C", 0, vec![Type::Nat])]);
    let legend = rendered
        .lines()
        .take_while(|line| !line.contains(":="))
        .collect::<Vec<_>>()
        .join(" ");
    for label in ["tyvar", "at-tyvar", "mu-binder", "app/0", "app/n"] {
        assert!(
            legend.contains(label),
            "the legend must define `{label}`: {legend}"
        );
    }
}

/// A μ-binder occurrence is called out separately from an ordinary `TyVar`: it
/// is bound by an enclosing `Mu`, not a free reference to a named type, and
/// confusing the two is the ADR 18.4.26i reading trap.
#[test]
fn a_mu_binder_occurrence_is_labelled_distinctly() {
    let rendered = render_raw_fields(&[ctor("Cons", 1, vec![Type::TyVar("α_List".into())])]);
    assert!(rendered.contains("mu-binder"), "{rendered}");
    assert!(!rendered.contains(" tyvar :="), "{rendered}");
}

/// A parameterized application is distinguished from a zero-argument one —
/// the boundary every named-type resolution site keys on.
#[test]
fn an_applied_generic_is_distinguished_from_a_bare_name() {
    let rendered = render_raw_fields(&[ctor(
        "C",
        0,
        vec![
            Type::app("List", vec![]),
            Type::app("List", vec![Type::Nat]),
        ],
    )]);
    assert!(rendered.contains("app/0"), "{rendered}");
    assert!(rendered.contains("app/n"), "{rendered}");
}

/// A no-field constructor is reported explicitly rather than silently omitted:
/// an absent line reads as "this constructor is missing", not "arity 0".
#[test]
fn a_nullary_constructor_is_reported_not_omitted() {
    let rendered = render_raw_fields(&[ctor("Nil", 0, vec![])]);
    assert!(rendered.contains("[0] Nil — no fields"), "{rendered}");
}

/// Output is ordered by constructor index, so two runs over the same ADT can be
/// diffed. `adt_types` is a `HashMap` and the constructor vector's order is not
/// guaranteed to be the source order.
#[test]
fn output_is_ordered_by_constructor_index() {
    let rendered = render_raw_fields(&[ctor("Cons", 1, vec![Type::Nat]), ctor("Nil", 0, vec![])]);
    let nil = rendered.find("Nil").expect("Nil is listed");
    let cons = rendered.find("Cons").expect("Cons is listed");
    assert!(nil < cons, "index 0 must precede index 1:\n{rendered}");
}

/// Every field of every constructor appears — a renderer that stopped at the
/// first field would look right on the one-field ADTs that dominate.
#[test]
fn every_field_of_every_constructor_is_listed() {
    let rendered = render_raw_fields(&[
        ctor("A", 0, vec![Type::Nat, Type::Bool]),
        ctor("B", 1, vec![Type::String]),
    ]);
    for expected in ["A.0", "A.1", "B.0"] {
        assert!(
            rendered.contains(expected),
            "missing {expected}:\n{rendered}"
        );
    }
}

/// Structural (non-named) field types still render, under a label that does not
/// pretend to name their head.
#[test]
fn a_structural_field_renders_without_a_named_head() {
    let rendered = render_raw_fields(&[ctor("P", 0, vec![Type::product(Type::Nat, Type::Bool)])]);
    assert!(rendered.contains("structural"), "{rendered}");
    assert!(rendered.contains("Product("), "{rendered}");
}
