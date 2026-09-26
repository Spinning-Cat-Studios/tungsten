//! Tests for the comparability predicate and its diagnostic twin.
//!
//! Split out of `support.rs` at ADR 1.8.26c: the instantiation arm brought a
//! cycle key, a depth bound and a second failure class, and the file was
//! already near the 400-line cap.

use super::*;
use crate::driver::{AdtTypes, RecordTypes};
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

fn records_with_point() -> ComparatorTypes {
    let mut records = RecordTypes::new();
    records.insert("Point".to_string(), vec![("x".to_string(), Type::Nat)]);
    ComparatorTypes::from_records(records)
}

/// A project defining `List<T> = Nil | Cons(T, List<T>)` and nothing else.
fn project_with_list() -> ComparatorTypes {
    project_with(list_definition(), RecordTypes::new())
}

fn list_definition() -> AdtTypes {
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

fn project_with(adts: AdtTypes, records: RecordTypes) -> ComparatorTypes {
    ComparatorTypes::new(
        records,
        &std::collections::HashMap::new(),
        &crate::elaborate::TypeProvenance::default(),
        adts,
        &std::collections::HashMap::new(),
    )
}

fn list_of(arg: Type) -> Type {
    Type::app("List", vec![arg])
}

// ---------------------------------------------------------------------------
// The instantiation arm (ADR 1.8.26c AC 1 / AC 2)
// ---------------------------------------------------------------------------

/// The arm this ADR exists for: an instantiation reached as a field type
/// resolves through `adt_types` instead of falling to the opaque-leaf arm.
#[test]
fn a_generic_instantiation_is_supported_when_its_argument_is() {
    let types = project_with_list();
    assert!(is_supported(&list_of(Type::Nat), &types));
    assert!(check_comparable(&list_of(Type::Nat), &types).is_ok());
}

/// AC 2 — the non-vacuity twin. "Support generic instantiations" is satisfiable
/// by accepting everything, so a noncomparable **argument** must still be
/// refused, and the report must name the argument's path rather than the
/// application as a whole.
#[test]
fn a_generic_instantiation_with_a_noncomparable_argument_is_refused() {
    let types = project_with_list();
    let opaque_arg = Type::arrow(Type::Nat, Type::Nat);
    let ty = list_of(opaque_arg);

    assert!(!is_supported(&ty, &types));
    let Err(Noncomparable::Opaque(path)) = check_comparable(&ty, &types) else {
        panic!("a list of functions is noncomparable BY POLICY, not unsettled");
    };
    assert!(
        path.contains("Arrow"),
        "the report must name the offending ARGUMENT, not just the application: {path}"
    );
}

/// The instantiation resolves through `adt_types`, so a project without them
/// still refuses — the empty map is a real answer ("resolve no instantiations"),
/// not a silent accept. This is what `from_records` promises its callers.
#[test]
fn without_adt_definitions_an_instantiation_stays_opaque() {
    let types = records_with_point();
    assert!(!is_supported(&list_of(Type::Nat), &types));
    assert!(matches!(
        check_comparable(&list_of(Type::Nat), &types),
        Err(Noncomparable::Opaque(_))
    ));
}

/// A generic **record** or **alias** is out of scope (§2 Non-Goals): neither has
/// a home for arguments, so an application naming one stays an opaque leaf
/// naming the path rather than being silently treated as the un-instantiated
/// type.
#[test]
fn a_generic_record_application_is_still_refused() {
    let types = records_with_point();
    let instantiated = Type::app("Point", vec![Type::Nat]);
    assert!(is_supported(&Type::app("Point", vec![]), &types));
    assert!(!is_supported(&instantiated, &types));
    assert!(check_comparable(&instantiated, &types).is_err());
}

// ---------------------------------------------------------------------------
// Termination (ADR 1.8.26c AC 4)
// ---------------------------------------------------------------------------

/// Recursion *through* an instantiation — the case that exists today, where
/// `List<Expr>` sits inside `Expr` — terminates with a verdict.
#[test]
fn recursion_through_an_instantiation_terminates() {
    // `type Tree = Node(List<Tree>)`, the self-reference reached only via the
    // instantiation.
    let mut adts = list_definition();
    adts.insert(
        "Tree".to_string(),
        (
            vec![],
            vec![ctor("Node", 0, vec![list_of(Type::TyVar("Tree".into()))])],
        ),
    );
    let types = project_with(adts, RecordTypes::new());

    // The encoding as the elaborator stores it: the μ is explicit, and the
    // recursive occurrence inside the instantiation is the bound variable.
    let tree = Type::mu("α_Tree", list_of(Type::TyVar("α_Tree".into())));
    assert!(is_supported(&tree, &types));
    assert!(check_comparable(&tree, &types).is_ok());
}

/// **Direct** polymorphic recursion does not reach the bound, and this pins
/// why — ADR 1.8.26c predicted `Nest<T> = Nil | Cons(T, Nest<(T, T)>)` as the
/// divergence case, and measurement says otherwise.
///
/// `replace_self_reference` rewrites an `App` whose *head* is the ADT to the
/// μ-binder **whatever its arguments are**, so `Nest<(T, T)>` collapses to
/// `α_Nest` and the expansion is a plain μ. That is the canonical encoder's own
/// rule — the whole compiler encodes direct polymorphic recursion as ordinary
/// recursion — so the comparator agreeing with it is the correct outcome here,
/// not a gap. The bound is still load-bearing; the case that reaches it is
/// below.
#[test]
fn direct_polymorphic_recursion_collapses_to_the_binder() {
    let mut adts = AdtTypes::new();
    adts.insert(
        "Nest".to_string(),
        (
            vec!["T".to_string()],
            vec![
                ctor("Nil", 0, vec![]),
                ctor(
                    "Cons",
                    1,
                    vec![
                        Type::TyVar("T".into()),
                        Type::app(
                            "Nest",
                            vec![Type::product(
                                Type::TyVar("T".into()),
                                Type::TyVar("T".into()),
                            )],
                        ),
                    ],
                ),
            ],
        ),
    );
    let types = project_with(adts, RecordTypes::new());
    assert_eq!(
        types.expand_adt("Nest", &[Type::Nat]),
        Some(Type::mu(
            "α_Nest",
            Type::sum(
                Type::Unit,
                Type::product(Type::Nat, Type::TyVar("α_Nest".into()))
            )
        )),
        "the parameterized self-application must collapse to the binder"
    );
    assert!(check_comparable(&Type::app("Nest", vec![Type::Nat]), &types).is_ok());
}

/// The divergence case that IS reachable: **mutual** polymorphic recursion with
/// no SCC group to collapse it. Each round reaches a strictly larger argument,
/// so no cycle key ever repeats and only the depth bound terminates the walk.
///
/// Empty recursion groups is not a contrived configuration — it is exactly what
/// `elab_compare`'s early gate passes, because SCCs are not computed until
/// Recursion Grouping. Without the bound, a direct `__compare` on such a type
/// would spin the *elaborator*.
///
/// It must be reported **unsettled**, not misreported as an opaque leaf: the
/// first says "we stopped looking", the second says "noncomparable by policy",
/// and they call for different fixes.
#[test]
fn mutual_polymorphic_recursion_is_reported_unsettled() {
    // `type Ping<T> = PNil | PCons(T, Pong<(T, T)>)`
    // `type Pong<T> = QNil | QCons(T, Ping<T>)`
    let pair = |head: &str, other: &str, grow: bool| {
        let arg = if grow {
            Type::product(Type::TyVar("T".into()), Type::TyVar("T".into()))
        } else {
            Type::TyVar("T".into())
        };
        (
            head.to_string(),
            (
                vec!["T".to_string()],
                vec![
                    ctor("Nil", 0, vec![]),
                    ctor(
                        "Cons",
                        1,
                        vec![Type::TyVar("T".into()), Type::app(other, vec![arg])],
                    ),
                ],
            ),
        )
    };
    let mut adts = AdtTypes::new();
    let (name, def) = pair("Ping", "Pong", true);
    adts.insert(name, def);
    let (name, def) = pair("Pong", "Ping", false);
    adts.insert(name, def);

    let types = project_with(adts, RecordTypes::new());
    let ty = Type::app("Ping", vec![Type::Nat]);

    assert_eq!(
        check_comparable(&ty, &types),
        Err(Noncomparable::Unsettled(INSTANTIATION_DEPTH_CAP)),
        "a bound exhaustion is `we stopped looking`, not `noncomparable by policy`"
    );
    assert!(!is_supported(&ty, &types));
}

/// A **name**-keyed cycle set would accept `List<Arrow>` on the strength of
/// having seen `List<Nat>`, and the walk would report a type comparable while
/// synthesis emitted a call to a comparator the closure never defines. Pins the
/// structural key by exhibiting the pair that distinguishes them.
#[test]
fn the_cycle_key_is_structural_not_name_keyed() {
    let types = project_with_list();
    // Both are "List". Only a structurally-keyed set can tell them apart.
    let good = list_of(Type::Nat);
    let bad = list_of(Type::arrow(Type::Nat, Type::Nat));
    let both = Type::product(good, bad);

    assert!(
        !is_supported(&both, &types),
        "a name-keyed set would accept the second `List` on the first's key"
    );
}

/// A `Nat`-argument instantiation nested well below the cap still settles — the
/// bound must not be so tight that ordinary nesting trips it.
#[test]
fn ordinary_nesting_stays_well_inside_the_bound() {
    let types = project_with_list();
    let nested = list_of(list_of(list_of(Type::Nat)));
    assert!(is_supported(&nested, &types));
    assert!(check_comparable(&nested, &types).is_ok());
}

// ---------------------------------------------------------------------------
// The pre-existing predicate surface
// ---------------------------------------------------------------------------

#[test]
fn an_undefined_named_type_is_not_comparable() {
    let types = ComparatorTypes::default();
    assert!(!is_supported(&Type::TyVar("Nope".to_string()), &types));
    assert!(check_comparable(&Type::TyVar("Nope".to_string()), &types).is_err());
}

// ---------------------------------------------------------------------------
// Path rendering (ADR 1.8.26c retrospective)
//
// `is_supported` is now *defined* as `check_comparable(..).is_ok()`, so an
// agreement test between them would assert nothing. What became fallible
// instead is the stack-built path: it is assembled leaf-ward and rendered
// root-first, so an off-by-one in the reversal or a dropped frame produces a
// plausible-looking but wrong location. These pin it.
// ---------------------------------------------------------------------------

/// A path through every segment kind, rendered in root-first order.
#[test]
fn the_reported_path_names_the_offender_root_first() {
    let types = ComparatorTypes::default();
    // (Nat × (Unit + <opaque>))  →  the offender is at `$.1.inr`
    let ty = Type::product(
        Type::Nat,
        Type::sum(Type::Unit, Type::arrow(Type::Nat, Type::Nat)),
    );
    let Err(Noncomparable::Opaque(path)) = check_comparable(&ty, &types) else {
        panic!("an arrow is noncomparable by policy");
    };
    assert!(
        path.starts_with("$.1.inr:"),
        "expected the path `$.1.inr`, got: {path}"
    );
}

/// A record field contributes its **name**, not an index — the §2.3 grammar
/// distinguishes `.field` from `[i]` because they point a reader at different
/// things.
#[test]
fn a_record_field_is_named_in_the_path() {
    let mut records = RecordTypes::new();
    records.insert(
        "Holder".to_string(),
        vec![
            ("ok".to_string(), Type::Nat),
            ("env".to_string(), Type::arrow(Type::Nat, Type::Nat)),
        ],
    );
    let types = ComparatorTypes::from_records(records);

    let Err(Noncomparable::Opaque(path)) = check_comparable(&Type::TyVar("Holder".into()), &types)
    else {
        panic!("the `env` field is noncomparable by policy");
    };
    assert!(path.starts_with("$.env:"), "got: {path}");
}

/// The path survives descent through an ADT variant and a μ-binder — the two
/// arms that do *not* extend it uniformly (a `Mu` adds no segment at all).
#[test]
fn a_variant_index_extends_the_path_and_a_mu_binder_does_not() {
    let types = ComparatorTypes::default();
    let ty = Type::mu(
        "α_T",
        Type::adt(
            "T".to_string(),
            vec![],
            vec![
                ("A".to_string(), Type::Nat),
                ("B".to_string(), Type::Nat),
                ("C".to_string(), Type::arrow(Type::Nat, Type::Nat)),
            ],
        ),
    );
    let Err(Noncomparable::Opaque(path)) = check_comparable(&ty, &types) else {
        panic!("variant C's payload is noncomparable by policy");
    };
    assert!(
        path.starts_with("$.2:"),
        "the μ-binder must add no segment and the variant index must be 2: {path}"
    );
}

/// A comparable type reports no path at all — the success arm must not be
/// paying for, or leaking, the diagnostic.
#[test]
fn a_comparable_type_yields_no_path() {
    let types = project_with_list();
    assert_eq!(check_comparable(&list_of(Type::Nat), &types), Ok(()));
    assert!(is_supported(&list_of(Type::Nat), &types));
}
