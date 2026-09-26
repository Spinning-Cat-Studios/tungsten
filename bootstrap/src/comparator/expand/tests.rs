//! Tests for the ADT-application expander (ADR 1.8.26c).
//!
//! The corpus-scale half of AC 5 — expanding every zero-parameter ADT of a real
//! elaborated project and diffing against its stored encoding — is the sibling
//! `encoder_agreement` module. What is asserted here is the shape produced from
//! hand-built definitions, where the expected encoding can be written out in
//! full.

use super::*;
use crate::elaborate::env::Constructor;
use crate::span::Span;

fn ctor(name: &str, index: usize, fields: Vec<Type>) -> Constructor {
    Constructor {
        name: name.to_string(),
        fields,
        index,
        visibility: None,
        span: Span::default(),
    }
}

/// `type List<T> = Nil | Cons(T, List<T>)` — the instantiation that blocks
/// ADR 29.6.26f AC 2, as `adt_types` actually stores it (measured: the
/// self-reference is a bare `TyVar("List")`, not an `App`).
fn list_adts() -> AdtTypes {
    let mut adts = AdtTypes::new();
    adts.insert(
        "List".to_string(),
        (
            vec!["T".to_string()],
            vec![
                ctor("Nil", 0, vec![]),
                ctor(
                    "Cons",
                    1,
                    vec![Type::TyVar("T".into()), Type::TyVar("List".into())],
                ),
            ],
        ),
    );
    adts
}

fn defs_for<'a>(
    adts: &'a AdtTypes,
    encoded: &'a HashMap<String, Type>,
    groups: &'a HashMap<String, Vec<String>>,
) -> AdtDefinitions<'a> {
    AdtDefinitions {
        adts,
        encoded,
        groups,
    }
}

fn expand_with(name: &str, args: &[Type], adts: &AdtTypes) -> Option<Type> {
    let encoded = HashMap::new();
    let groups = HashMap::new();
    expand_application(name, args, &defs_for(adts, &encoded, &groups))
}

/// The headline shape: `List<Nat>` is the cons-list `μ` the P4 stack-safe spine
/// recognises, with the argument in the element slot.
#[test]
fn a_generic_list_expands_to_the_cons_list_mu() {
    let expanded = expand_with("List", &[Type::Nat], &list_adts()).expect("List is a known ADT");
    assert_eq!(
        expanded,
        Type::mu(
            "α_List",
            Type::sum(
                Type::Unit,
                Type::product(Type::Nat, Type::TyVar("α_List".into()))
            )
        )
    );
}

/// AC 5, *substitute* arm: two instantiations of one generic differ **exactly**
/// at the element slot and nowhere else. Asserted by rebuilding one from the
/// other — an assertion on the two shapes separately would pass for an expander
/// that substituted in extra places too.
#[test]
fn two_instantiations_differ_only_at_the_argument_slot() {
    let adts = list_adts();
    let nats = expand_with("List", &[Type::Nat], &adts).expect("List<Nat>");
    let bools = expand_with("List", &[Type::Bool], &adts).expect("List<Bool>");

    assert_ne!(nats, bools, "the argument must actually reach the shape");
    assert_eq!(
        nats.substitute_nat_slot_for_bool(),
        bools,
        "the two expansions differ anywhere other than the element slot"
    );
}

/// A helper local to this assertion: rewrite every `Nat` leaf to `Bool`. If the
/// expander touched anything but the argument slot, the rewrite would not land
/// on the other instantiation.
trait NatToBool {
    fn substitute_nat_slot_for_bool(&self) -> Type;
}

impl NatToBool for Type {
    fn substitute_nat_slot_for_bool(&self) -> Type {
        match self {
            Type::Nat => Type::Bool,
            other => other.map_children(|c| c.substitute_nat_slot_for_bool()),
        }
    }
}

/// A non-recursive generic gets **no** μ-binder — the encoder only wraps one
/// when the ADT names itself. Getting this wrong would emit an `Unfold` against
/// a value that carries no `Fold` (the ADR 1.8.26b D2 failure).
#[test]
fn a_non_recursive_generic_expands_without_a_binder() {
    let mut adts = AdtTypes::new();
    adts.insert(
        "Option".to_string(),
        (
            vec!["T".to_string()],
            vec![
                ctor("None", 0, vec![]),
                ctor("Some", 1, vec![Type::TyVar("T".into())]),
            ],
        ),
    );
    let expanded = expand_with("Option", &[Type::String], &adts).expect("Option is a known ADT");
    assert_eq!(expanded, Type::sum(Type::Unit, Type::String));
}

/// Three or more constructors take the `Adt` arm of the ADR 2.2.26 sum policy,
/// carrying the application's arguments — the policy helper is shared with the
/// canonical encoder precisely so this cannot drift (ADR 21.7.26e wall 1).
#[test]
fn three_constructors_take_the_adt_arm_carrying_the_arguments() {
    let mut adts = AdtTypes::new();
    adts.insert(
        "Three".to_string(),
        (
            vec!["A".to_string()],
            vec![
                ctor("X", 0, vec![]),
                ctor("Y", 1, vec![Type::TyVar("A".into())]),
                ctor("Z", 2, vec![Type::Bool]),
            ],
        ),
    );
    let expanded = expand_with("Three", &[Type::Nat], &adts).expect("Three is a known ADT");
    assert_eq!(
        expanded,
        Type::adt(
            "Three".to_string(),
            vec![Type::Nat],
            vec![
                ("X".to_string(), Type::Unit),
                ("Y".to_string(), Type::Nat),
                ("Z".to_string(), Type::Bool),
            ]
        )
    );
}

/// Constructor payloads are **right**-nested, the layout values actually flow
/// through (ADR 1.8.26b D1). A left-nested expander would compare correct at
/// arity ≤2 and silently wrong at 3.
#[test]
fn a_three_field_constructor_payload_is_right_nested() {
    let mut adts = AdtTypes::new();
    adts.insert(
        "Wide".to_string(),
        (
            vec!["T".to_string()],
            vec![ctor(
                "W",
                0,
                vec![Type::TyVar("T".into()), Type::Bool, Type::String],
            )],
        ),
    );
    let expanded = expand_with("Wide", &[Type::Nat], &adts).expect("Wide is a known ADT");
    assert_eq!(
        expanded,
        Type::product(Type::Nat, Type::product(Type::Bool, Type::String))
    );
}

/// A reference to another ADT is inlined from its stored encoding, the way
/// `resolve_type_references_impl` does during encoding — this is what makes a
/// zero-parameter expansion comparable to `encoded_types[name]` at all.
#[test]
fn a_reference_to_another_adt_is_inlined_from_its_stored_encoding() {
    let mut adts = AdtTypes::new();
    adts.insert(
        "Holder".to_string(),
        (
            vec![],
            vec![ctor("H", 0, vec![Type::TyVar("Colour".into())])],
        ),
    );
    adts.insert(
        "Colour".to_string(),
        (
            vec![],
            vec![ctor("Red", 0, vec![]), ctor("Blue", 1, vec![])],
        ),
    );
    let mut encoded = HashMap::new();
    encoded.insert("Colour".to_string(), Type::sum(Type::Unit, Type::Unit));
    let groups = HashMap::new();

    let expanded = expand_application("Holder", &[], &defs_for(&adts, &encoded, &groups))
        .expect("Holder is a known ADT");
    assert_eq!(expanded, Type::sum(Type::Unit, Type::Unit));
}

/// A zero-argument reference is served from the **stored** encoding, and only
/// from there: an ADT the project defines but has no finalized encoding for is
/// left alone rather than re-derived.
///
/// The distinction is load-bearing, not stylistic. A stored encoding is closed,
/// so splicing it in is one lookup that cannot recurse. Re-deriving would expand
/// a body whose own references may be unresolvable — and the honest outcome for
/// such a name is the support predicate refusing it by path, which only happens
/// if the reference survives verbatim.
#[test]
fn an_adt_without_a_stored_encoding_is_left_verbatim() {
    let mut adts = AdtTypes::new();
    adts.insert(
        "Outer".to_string(),
        (
            vec![],
            vec![ctor("O", 0, vec![Type::app("Unfinalized", vec![])])],
        ),
    );
    adts.insert(
        "Unfinalized".to_string(),
        (vec![], vec![ctor("U", 0, vec![Type::Bool])]),
    );
    // `encoded` deliberately has no entry for `Unfinalized`.
    let expanded = expand_application(
        "Outer",
        &[],
        &defs_for(&adts, &HashMap::new(), &HashMap::new()),
    )
    .expect("Outer is a known ADT");
    assert_eq!(
        expanded,
        Type::app("Unfinalized", vec![]),
        "a zero-argument reference must come from the stored encoding or not at all"
    );
}

/// …but a **record** reference is not: stored encodings keep records nominal
/// (`TyVar("Span")`), and `ComparatorTypes::records()` resolves them downstream.
/// Inlining here would diverge from the stored shape.
#[test]
fn a_record_reference_is_left_nominal() {
    let mut adts = AdtTypes::new();
    adts.insert(
        "Node".to_string(),
        (vec![], vec![ctor("N", 0, vec![Type::TyVar("Span".into())])]),
    );
    // `Span` is a record: absent from `adts`, so it must survive verbatim even
    // if some stored encoding exists for it.
    let mut encoded = HashMap::new();
    encoded.insert("Span".to_string(), Type::product(Type::Nat, Type::Nat));
    let groups = HashMap::new();

    let expanded = expand_application("Node", &[], &defs_for(&adts, &encoded, &groups))
        .expect("Node is a known ADT");
    assert_eq!(expanded, Type::TyVar("Span".into()));
}

/// A mutual-recursion sibling becomes that sibling's μ-binder, and the cluster
/// gets one nested binder per member with self outermost (ADR 18.4.26i). A bare
/// `TyVar("Beta")` here would be refused downstream as an undefined name.
#[test]
fn a_mutual_group_sibling_becomes_its_binder_under_a_nested_chain() {
    let mut adts = AdtTypes::new();
    adts.insert(
        "Alpha".to_string(),
        (vec![], vec![ctor("A", 0, vec![Type::TyVar("Beta".into())])]),
    );
    adts.insert(
        "Beta".to_string(),
        (
            vec![],
            vec![ctor("B", 0, vec![Type::TyVar("Alpha".into())])],
        ),
    );
    let encoded = HashMap::new();
    let mut groups = HashMap::new();
    groups.insert(
        "Alpha".to_string(),
        vec!["Alpha".to_string(), "Beta".to_string()],
    );

    let expanded = expand_application("Alpha", &[], &defs_for(&adts, &encoded, &groups))
        .expect("Alpha is a known ADT");
    assert_eq!(
        expanded,
        Type::mu("α_Alpha", Type::mu("α_Beta", Type::TyVar("α_Beta".into())))
    );
}

/// A field naming a record through the Type-Body Collection `@`-prefix loses
/// it: the prefix is elaboration-internal, `records()` is keyed without it, and
/// stored encodings carry none. Leaving it in reported every AST node type an
/// incomplete closure caused by `@Ident is not defined`.
#[test]
fn the_at_prefix_does_not_leak_out_of_an_expansion() {
    let mut adts = AdtTypes::new();
    adts.insert(
        "Node".to_string(),
        (
            vec!["T".to_string()],
            vec![ctor("N", 0, vec![Type::TyVar("@Ident".into())])],
        ),
    );
    assert_eq!(
        expand_with("Node", &[Type::Nat], &adts),
        Some(Type::TyVar("Ident".into()))
    );
}

/// An unknown name expands to nothing — the caller then reports an opaque leaf
/// naming the path, which is how a generic *alias* or *record* (§2 Non-Goals)
/// is refused rather than silently accepted.
#[test]
fn an_unknown_name_does_not_expand() {
    assert_eq!(expand_with("Nope", &[Type::Nat], &list_adts()), None);
}

/// An under-applied generic leaves its unbound parameter as a free `TyVar`,
/// which the support predicate refuses by path. Silently defaulting it would
/// synthesize a comparator for a type nobody wrote.
#[test]
fn an_under_applied_generic_leaves_its_parameter_free() {
    let expanded = expand_with("List", &[], &list_adts()).expect("List is a known ADT");
    assert_eq!(
        expanded,
        Type::mu(
            "α_List",
            Type::sum(
                Type::Unit,
                Type::product(Type::TyVar("T".into()), Type::TyVar("α_List".into()))
            )
        )
    );
}
