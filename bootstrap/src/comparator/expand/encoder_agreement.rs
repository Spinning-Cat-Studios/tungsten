//! ADR 1.8.26c **AC 5, encode arm**: the expander agrees with the canonical
//! stored encoder on a real elaborated project.
//!
//! `expand.rs`'s own tests assert shapes against hand-written expectations,
//! which proves the builders are wired up but not that the *result* is what the
//! compiler stores. This file elaborates actual `.tg` source and diffs
//! `expand_adt(name, [])` against `project.encoded_types[name]` for every
//! zero-parameter ADT the file declares — the only arm where a stored encoding
//! exists to compare against, since a parameterized type has none
//! (`elaborate::codegen_types`).
//!
//! Why this arm and not a spot check: "substituting and encoding yields the same
//! shape the stored encoder would have produced" is an assertion this tree has
//! been burned by. `encoding_utils.rs` records a third parallel encoder copy
//! whose right-nested products silently failed `types_pattern_match`, and ADR
//! 21.7.26e found three more normalize-side divergences. Each looked right.
//!
//! The corpus deliberately spans every shape the sum/μ policy branches on
//! (ADR 2.2.26: 1 / 2 / 3+ constructors), plus recursion, mutual recursion,
//! wide constructors and cross-ADT references — so a fixture that reached only
//! the easy arm cannot pass vacuously. The census assertion at the end is what
//! keeps it honest: an expander that returned `None` for everything would
//! otherwise satisfy an all-quantified diff over an empty set.

use std::collections::HashSet;

use tungsten_core::Type;

use crate::comparator::ComparatorTypes;
use crate::driver::{self, ProjectOutput};

/// Source spanning the shapes the encoder branches on. Kept in one file so the
/// whole corpus costs a single elaboration.
const CORPUS: &str = r#"
type Wrapped = W(Nat)
type Colour = Red | Blue
type Direction = North | South | East | West
type Wide = Triple(Nat, Bool, String)
type Chain = CNil | CCons(Nat, Chain)
type Holder = H(Colour)
type Alpha = ANil | ACons(Beta)
type Beta = BNil | BCons(Alpha)
type List<T> = Nil | Cons(T, List<T>)
type Boxed = B(List<Nat>)

fn main() -> Nat { 0 }
"#;

/// Elaborate [`CORPUS`] once and hand back the project plus a `ComparatorTypes`
/// built from it exactly as the doctor check does.
fn elaborate_corpus() -> (ProjectOutput, ComparatorTypes) {
    // Elaboration clears the process-global comparator request registry, so
    // this must not run alongside a test that populates it (ADR 28.7.26a). The
    // guard is deliberately the one declared beside that registry.
    let _exclusive = crate::comparator::requests::TEST_EXCLUSIVE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = tempfile::TempDir::new().expect("temp dir");
    let path = dir.path().join("corpus.tg");
    std::fs::write(&path, CORPUS).expect("write corpus");
    let project = driver::elaborate_project(&path, false, 20, None).expect("corpus elaborates");
    let types = ComparatorTypes::new(
        project.record_types.clone(),
        &project.encoded_types,
        &project.type_provenance,
        project.adt_types.clone(),
        &project.mutual_recursion_groups,
    );
    (project, types)
}

/// The zero-parameter ADTs of the corpus, sorted, so a failure names the same
/// type on every run (`adt_types` is a `HashMap`).
fn zero_parameter_adts(project: &ProjectOutput) -> Vec<String> {
    let mut names: Vec<String> = project
        .adt_types
        .iter()
        .filter(|(_, (params, _))| params.is_empty())
        .map(|(name, _)| name.clone())
        .collect();
    names.sort();
    names
}

/// AC 5, encode arm: for every zero-parameter ADT with a stored encoding, the
/// expansion **is** that encoding.
#[test]
fn a_zero_parameter_expansion_equals_the_stored_encoding() {
    let (project, types) = elaborate_corpus();
    let mut compared = 0usize;
    let mut divergent: Vec<String> = Vec::new();

    for name in zero_parameter_adts(&project) {
        let Some(stored) = project.encoded_types.get(&name) else {
            continue;
        };
        let Some(expanded) = types.expand_adt(&name, &[]) else {
            divergent.push(format!("{name}: expansion produced nothing"));
            continue;
        };
        compared += 1;
        if &expanded != stored {
            divergent.push(format!(
                "{name}:\n  stored   {stored:?}\n  expanded {expanded:?}"
            ));
        }
    }

    assert!(
        divergent.is_empty(),
        "the expander is a SECOND encoder, not the shared one:\n{}",
        divergent.join("\n")
    );
    // Non-vacuity: the corpus declares ten types; a diff that compared none of
    // them would pass above while proving nothing (ADR 28.7.26e).
    assert!(
        compared >= 8,
        "only {compared} types were actually compared — the arm is near-vacuous"
    );
}

/// The corpus reaches every branch of the sum/μ policy. Without this, the arm
/// above could pass while exercising only two-constructor non-recursive ADTs —
/// the one shape that is hard to get wrong.
#[test]
fn the_corpus_reaches_every_encoding_shape() {
    let (project, _) = elaborate_corpus();
    let mut shapes: HashSet<&'static str> = HashSet::new();
    for name in zero_parameter_adts(&project) {
        let Some(stored) = project.encoded_types.get(&name) else {
            continue;
        };
        shapes.insert(shape_of(stored));
        if let Type::Mu(_, body) = stored {
            shapes.insert(shape_of(body));
            if matches!(body.as_ref(), Type::Mu(_, _)) {
                shapes.insert("nested-mu");
            }
        }
    }
    for required in ["bare", "sum", "adt", "mu", "nested-mu"] {
        assert!(
            shapes.contains(required),
            "the corpus never produces a `{required}` encoding, so the diff never checks one: {shapes:?}"
        );
    }
}

/// A coarse name for an encoding's outermost shape, matching the ADR 2.2.26
/// policy branches.
fn shape_of(ty: &Type) -> &'static str {
    match ty {
        Type::Mu(_, _) => "mu",
        Type::Sum(_, _) => "sum",
        Type::Adt(_, _, _) => "adt",
        _ => "bare",
    }
}

/// AC 5, substitute arm, on a real project: the generic the whole ADR is about
/// expands to the cons-list μ, and two instantiations differ only at the
/// element slot.
#[test]
fn a_real_generic_instantiates_at_the_element_slot_only() {
    let (_, types) = elaborate_corpus();
    let nats = types.expand_adt("List", &[Type::Nat]).expect("List<Nat>");
    let strings = types
        .expand_adt("List", &[Type::String])
        .expect("List<String>");

    assert_eq!(
        nats,
        Type::mu(
            "α_List",
            Type::sum(
                Type::Unit,
                Type::product(Type::Nat, Type::TyVar("α_List".into()))
            )
        )
    );
    assert_eq!(
        strings,
        Type::mu(
            "α_List",
            Type::sum(
                Type::Unit,
                Type::product(Type::String, Type::TyVar("α_List".into()))
            )
        )
    );
}

/// The expansion of a generic is also the shape the *stored* encoder inlines
/// when that generic appears instantiated inside another type — so the two
/// producers agree on the parameterized case too, which no stored encoding for
/// `List` itself could show.
#[test]
fn an_inlined_instantiation_matches_the_expansion() {
    let (project, types) = elaborate_corpus();
    // `type Boxed = B(List<Nat>)` — one constructor, one field, so the stored
    // encoding IS the inlined `List<Nat>`.
    let stored = project
        .encoded_types
        .get("Boxed")
        .expect("Boxed has a stored encoding");
    assert_eq!(
        stored,
        &types.expand_adt("List", &[Type::Nat]).expect("List<Nat>"),
        "the encoder inlined a different shape than the expander builds"
    );
}
