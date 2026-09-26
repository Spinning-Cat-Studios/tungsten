//! D3's agreement test (ADR 18.8.26b AC2): the bootstrap's driver and
//! `tungsten_core`'s must decide the same thing.
//!
//! The *predicate* is single-sourced — both sides call
//! `check_strict_positivity` — but each has its own SCC pass, because the
//! parent ADR's D1(b) forbids relocating `tarjan_scc_with_depth` out of
//! `bootstrap/`. Two Tarjans over one adjacency can drift on **grouping**
//! without either being wrong about positivity, and a split SCC is a false
//! accept. So the comparison is over groups **and** violations, not violations
//! alone: a driver that grouped differently and happened to agree on today's
//! violations is exactly the drift this test exists to catch.
//!
//! The fixture shapes are the corpus's: one case per
//! `tests/golden/error/positivity_*` and `tests/golden/check/positivity_*`
//! file, hand-built as engine input so the comparison stays a pure function
//! over injected data rather than a pair of compiler runs.

use std::collections::{BTreeMap, BTreeSet};

use tungsten_core::types::positivity::{analyze_positivity, PositivityDefs};
use tungsten_core::Type;

use crate::elaborate::env::TypeDef;
use crate::elaborate::positivity::{analyze, from_env_types};

use super::{adt, alias, record, tv};

/// Groups as a canonical sorted set-of-sets, so the two drivers' component
/// *order* (reverse-topological on both, but not asserted to be) cannot make
/// an agreement look like a disagreement.
fn canonical(groups: &[BTreeSet<String>]) -> BTreeSet<BTreeSet<String>> {
    groups.iter().cloned().collect()
}

/// Run both drivers over one corpus and assert they agree on everything the
/// gate reads.
///
/// Returns the shared violation count so each case can additionally pin what
/// the corpus is *supposed* to say — agreement between two drivers that both
/// find nothing is agreement about nothing.
fn agree(types: Vec<(String, TypeDef)>) -> usize {
    let map: BTreeMap<String, TypeDef> = types.into_iter().collect();
    let (defs, spans) = from_env_types(map.iter());
    let bootstrap = analyze(&defs, spans);
    let core = analyze_positivity(&defs);

    assert_eq!(
        canonical(&bootstrap.groups),
        canonical(&core.groups),
        "the two drivers disagree about the SCCs of one adjacency"
    );

    let mut bootstrap_violations = bootstrap.violations.clone();
    let mut core_violations = core.violations.clone();
    bootstrap_violations.sort();
    core_violations.sort();
    assert_eq!(
        bootstrap_violations, core_violations,
        "the two drivers disagree about the violations of one predicate"
    );

    core.violations.len()
}

/// `tests/golden/error/positivity_self_arrow.tg`, and — the same shape by
/// import — `positivity_cross_module/`.
#[test]
fn self_arrow_singleton() {
    let found = agree(vec![adt(
        "Bad",
        &[],
        vec![("Mk", vec![Type::arrow(tv("@Bad"), tv("@Bad"))])],
    )]);
    assert_eq!(found, 1);
}

/// `tests/golden/error/positivity_mutual.tg` — the SCC is what makes it
/// visible, so a grouping disagreement here changes the verdict.
#[test]
fn mutual_pair() {
    let found = agree(vec![
        adt(
            "A",
            &[],
            vec![("MkA", vec![Type::arrow(tv("@B"), Type::Nat)])],
        ),
        adt("B", &[], vec![("MkB", vec![tv("@A")])]),
    ]);
    assert_eq!(found, 1);
}

/// `tests/golden/error/positivity_double_negative.tg`.
#[test]
fn double_negative() {
    let found = agree(vec![adt(
        "Bad3",
        &[],
        vec![(
            "Mk",
            vec![Type::arrow(Type::arrow(tv("@Bad3"), Type::Nat), Type::Nat)],
        )],
    )]);
    assert_eq!(found, 1);
}

/// `tests/golden/error/positivity_inherited_param.tg` — the violation is
/// inherited through `Fn1`, so both sides must also agree on the `via` chain.
#[test]
fn inherited_through_a_parameter() {
    let found = agree(vec![
        adt(
            "Fn1",
            &["T"],
            vec![("Mk", vec![Type::arrow(tv("T"), Type::Nat)])],
        ),
        adt(
            "Bad2",
            &[],
            vec![("B", vec![Type::app("Fn1", vec![tv("@Bad2")])])],
        ),
    ]);
    assert_eq!(found, 1);
}

/// `tests/golden/error/positivity_record_cycle.tg` — the cycle runs through a
/// record, which is not a node in the elaborator's ADT-only graph.
#[test]
fn record_mediated_cycle() {
    let found = agree(vec![
        adt(
            "A",
            &[],
            vec![("MkA", vec![Type::arrow(tv("@R"), Type::Nat)])],
        ),
        record("R", vec![("a", tv("@A"))]),
    ]);
    assert_eq!(found, 1);
}

/// `tests/golden/error/positivity_alias_interposition.tg` — the alias is
/// inlined before either driver sees a graph, so both must attribute the
/// violation to `Bad4`'s constructor.
#[test]
fn alias_interposition() {
    let found = agree(vec![
        alias("F", &["T"], Type::arrow(tv("T"), Type::Nat)),
        adt(
            "Bad4",
            &[],
            vec![("B", vec![Type::app("F", vec![tv("@Bad4")])])],
        ),
    ]);
    assert_eq!(found, 1);
}

/// `tests/golden/error/positivity_alias_mutual_cycle.tg` — the cycle
/// `C -> D -> C` runs through an alias that is a node in neither graph.
#[test]
fn alias_mediated_mutual_cycle() {
    let found = agree(vec![
        alias("Fun", &["T"], Type::arrow(tv("T"), Type::Nat)),
        adt(
            "C",
            &[],
            vec![("MkC", vec![Type::app("Fun", vec![tv("@D")])])],
        ),
        adt("D", &[], vec![("MkD", vec![tv("@C")])]),
    ]);
    assert_eq!(found, 1);
}

/// `tests/golden/check/positivity_accepted.tg` — the acceptance half. Two
/// drivers that agreed only on rejections would still let a false rejection
/// through, which is the criterion the mirror is most likely to fail.
#[test]
fn accepted_corpus() {
    let found = agree(vec![
        adt(
            "Ok",
            &[],
            vec![("MkOk", vec![Type::arrow(Type::Nat, tv("@Ok"))])],
        ),
        adt(
            "List",
            &["T"],
            vec![
                ("Nil", vec![]),
                ("Cons", vec![tv("T"), Type::app("List", vec![tv("T")])]),
            ],
        ),
        adt(
            "Tree",
            &[],
            vec![("Node", vec![Type::app("List", vec![tv("@Tree")])])],
        ),
        adt("E", &[], vec![("MkE", vec![tv("@F")])]),
        adt("F", &[], vec![("MkF", vec![tv("@E")])]),
    ]);
    assert_eq!(found, 0);
}

/// `tests/golden/check/positivity_phantom.tg` — a discarded parameter erases
/// the occurrence, and the two drivers share the fixpoint that decides it.
#[test]
fn phantom_parameter_corpus() {
    let found = agree(vec![
        adt("Phantom", &["T"], vec![("P", vec![Type::Nat])]),
        alias("PhantomAlias", &["T"], Type::Nat),
        adt(
            "X",
            &[],
            vec![("MkX", vec![Type::app("Phantom", vec![tv("@X")])])],
        ),
        adt(
            "Y",
            &[],
            vec![(
                "MkY",
                vec![Type::app("Phantom", vec![Type::arrow(tv("@Y"), Type::Nat)])],
            )],
        ),
        adt(
            "Z",
            &[],
            vec![(
                "MkZ",
                vec![Type::app(
                    "PhantomAlias",
                    vec![Type::arrow(tv("@Z"), Type::Nat)],
                )],
            )],
        ),
    ]);
    assert_eq!(found, 0);
}

/// D1's implicit-close hazard, at this layer: the protocol has no `def_end`,
/// so a definition with no successor to close it is the one most likely to be
/// lost. Here the violating definition sorts **last** in the environment, so
/// losing it would leave both drivers agreeing on zero — which is why the case
/// pins the count rather than only the agreement.
#[test]
fn the_last_definition_in_the_environment_is_not_lost() {
    let found = agree(vec![
        adt("Aaa", &[], vec![("MkAaa", vec![Type::Nat])]),
        adt("Mmm", &[], vec![("MkMmm", vec![Type::Nat])]),
        adt(
            "Zzz",
            &[],
            vec![("MkZzz", vec![Type::arrow(tv("@Zzz"), Type::Nat)])],
        ),
    ]);
    assert_eq!(
        found, 1,
        "the last-registered definition must still be checked"
    );
}

/// The premise the whole mirror rests on, asserted rather than remembered.
///
/// The self-hosted elaborator instantiates generics **structurally** — it
/// substitutes arguments into the referenced type's stored encoding — so no
/// `Type::App` head survives in a self-host encoding. Everything about the
/// marshalling design follows from that: the graph's edges run through `TyVar`
/// placeholders, and the engine's "absent head ⇒ arguments forbidden" arm can
/// never fire on that side.
///
/// If a `tg_type_app` call ever appears in `src/compiler/`, that premise is
/// gone and `docs/repo-memory/elaboration-pipeline.md` § "What the SELF-HOSTED
/// encoder does differently" is wrong. A grep is a crude guard; it is also the
/// only one that fails on the day the premise changes rather than on the day
/// someone notices.
#[test]
fn the_self_hosted_encoder_constructs_no_app_heads() {
    let compiler = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .join("src/compiler");

    let mut offenders = Vec::new();
    let mut stack = vec![compiler.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("src/compiler is readable") {
            let path = entry.expect("readable entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "tg")
                && std::fs::read_to_string(&path)
                    .expect("readable .tg")
                    .contains("tg_type_app")
            {
                offenders.push(path);
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "the self-host now builds App heads — the mirror's graph assumptions and \
         elaboration-pipeline.md both need revisiting: {offenders:?}"
    );
}
