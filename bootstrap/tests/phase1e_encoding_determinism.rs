//! Regression guard for ADR 22.7.26c — cross-run stability of stored
//! Phase-1e type encodings.
//!
//! Before the fix, `cache_type_encodings` iterated `env.types` (a `HashMap`)
//! in hash order, and an ADT's encoding inlined a referenced type's cached
//! encoding only if that reference happened to be already Phase-1e-cached
//! when the referrer was encoded. Different iteration order → different
//! inline depth → the same type stored differently run-to-run.
//!
//! These tests elaborate the same project twice (fresh in-process runs, no
//! elaboration cache in play — the tempdir has no `.tungsten` and cache
//! writes require `TUNGSTEN_ELAB_CACHE=1`) and assert every stored encoding
//! is structurally identical (`==` per tree, deliberately NOT
//! `normalize_for_comparison` — the very relation the ADR makes redundant).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use tungsten_bootstrap::driver;
use tungsten_core::Type;

/// Elaborate a project and return its stored Phase-1e encodings.
fn stored_encodings(entry: &Path) -> HashMap<String, Type> {
    driver::elaborate_project(entry, false, 100, None)
        .expect("project must elaborate cleanly")
        .encoded_types
}

/// Compare two stored-encoding maps with strict structural `==`, returning
/// the names that differ (present-in-one or unequal trees).
fn encoding_divergences(
    first: &HashMap<String, Type>,
    second: &HashMap<String, Type>,
) -> Vec<String> {
    let mut divergent: Vec<String> = Vec::new();
    for (name, first_encoding) in first {
        match second.get(name) {
            Some(second_encoding) if first_encoding == second_encoding => {}
            _ => divergent.push(name.clone()),
        }
    }
    for name in second.keys() {
        if !first.contains_key(name) {
            divergent.push(name.clone());
        }
    }
    divergent.sort();
    divergent.dedup();
    divergent
}

/// Assert `runs` consecutive elaborations of `entry` agree on every stored
/// encoding, with per-type detail on failure.
fn assert_cross_run_stable(entry: &Path, runs: usize) {
    let baseline = stored_encodings(entry);
    assert!(
        !baseline.is_empty(),
        "fixture must produce stored encodings"
    );
    for run in 1..runs {
        let repeat = stored_encodings(entry);
        let divergent = encoding_divergences(&baseline, &repeat);
        if !divergent.is_empty() {
            for name in &divergent {
                eprintln!("✗ {name} diverged between run 0 and run {run}:");
                match (baseline.get(name), repeat.get(name)) {
                    (Some(a), Some(b)) => {
                        eprintln!("  run 0: {}", a.display_detailed());
                        eprintln!("  run {run}: {}", b.display_detailed());
                    }
                    (a, b) => eprintln!(
                        "  present in run 0: {}, in run {run}: {}",
                        a.is_some(),
                        b.is_some()
                    ),
                }
            }
            panic!(
                "{} stored encoding(s) not byte-stable across elaborations: {:?}",
                divergent.len(),
                divergent
            );
        }
    }
}

/// A multi-module fixture covering the shapes that exercised the Phase-1e
/// order sensitivity: cross-module ADT→ADT references, a mutual-recursion
/// group (Tree/Forest), an alias to a group member, an alias to a plain ADT,
/// a record with ADT fields, and an ADT referencing all of the above.
fn write_multi_module_fixture(dir: &Path) -> PathBuf {
    let entry = dir.join("main.tg");
    std::fs::write(
        &entry,
        "mod types_a;\nmod types_b;\n\nfn main() -> Nat { 0 }\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("types_a.tg"),
        r#"
pub type Color = Red | Green | Blue
pub type Tree = Leaf | Node(Forest)
pub type Forest = FEmpty | FMore(Tree, Forest)
pub type ColorAlias = Color
"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("types_b.tg"),
        r#"
use types_a::{Tree, Color};

pub type Wrapper = Wrap(Tree) | Tint(Color)
pub type TreeAlias = Tree
pub type Panel = { main_color: Color, count: Nat }
pub type Deep = DeepOne(Wrapper) | DeepTwo(Panel)
"#,
    )
    .unwrap();
    entry
}

#[test]
fn stored_encodings_stable_across_runs_multi_module_fixture() {
    let dir = tempfile::tempdir().unwrap();
    let entry = write_multi_module_fixture(dir.path());
    assert_cross_run_stable(&entry, 4);
}

/// The headline check on the real L2 compiler tree (ADR 22.7.26c AC2).
/// Heavy (two full elaborations of `src/compiler/main.tg`), so ignored by
/// default; run explicitly with:
/// `cargo test -p tungsten_bootstrap --no-default-features --test phase1e_encoding_determinism -- --ignored`
/// after a `tungsten cache clean` (a warm `.tungsten` cache would make the
/// two runs trivially identical).
#[test]
#[ignore = "heavy: elaborates src/compiler/main.tg twice"]
fn stored_encodings_stable_across_runs_main_tg() {
    let entry = Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/compiler/main.tg");
    if !entry.exists() {
        eprintln!("src/compiler/main.tg not found — skipping");
        return;
    }
    report_bare_adt_tyvar_refs(&entry);
    assert_cross_run_stable(&entry, 3);
}

/// Collect every bare `TyVar` name inside `ty` whose stripped spelling names
/// a known ADT. μ-bound variables (`α_`-prefixed) are skipped — they are
/// binder references, not inline decisions.
fn collect_adt_tyvar_refs(ty: &Type, adt_names: &HashSet<&str>, hits: &mut HashSet<String>) {
    if let Type::TyVar(name) = ty {
        let stripped = name.strip_prefix('@').unwrap_or(name);
        if !stripped.starts_with("α_") && adt_names.contains(stripped) {
            hits.insert(stripped.to_string());
        }
        return;
    }
    for child in ty.children() {
        collect_adt_tyvar_refs(child, adt_names, hits);
    }
}

/// AC3 evidence (ADR 22.7.26c, canonical inlining): report stored **ADT**
/// encodings that still name another defined ADT via a bare `TyVar`. After
/// the reverse-topological encode order, every such residual should be a
/// cycle break (a recursive reference), not an inline miss; the report is
/// printed for manual classification rather than asserted, because
/// alias-hidden cycles legitimately keep a plain `TyVar` cycle break (the
/// encoding-shape non-goal).
fn report_bare_adt_tyvar_refs(entry: &Path) {
    let output =
        driver::elaborate_project(entry, false, 100, None).expect("project must elaborate cleanly");
    let adt_names: HashSet<&str> = output.adt_types.keys().map(|s| s.as_str()).collect();
    let mut total = 0usize;
    let mut affected: Vec<String> = Vec::new();
    for (name, encoding) in &output.encoded_types {
        if !output.adt_types.contains_key(name) {
            continue;
        }
        let mut hits: HashSet<String> = HashSet::new();
        collect_adt_tyvar_refs(encoding, &adt_names, &mut hits);
        hits.remove(name.as_str());
        if !hits.is_empty() {
            let mut sorted: Vec<String> = hits.into_iter().collect();
            sorted.sort();
            total += sorted.len();
            affected.push(format!("{name} → {sorted:?}"));
        }
    }
    affected.sort();
    eprintln!(
        "AC3 scan: {} bare ADT TyVar reference(s) across {} stored ADT encoding(s)",
        total,
        affected.len()
    );
    for line in &affected {
        eprintln!("  {line}");
    }
}
