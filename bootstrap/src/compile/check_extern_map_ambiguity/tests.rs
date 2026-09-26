//! AC tests for `doctor check extern-map-ambiguity` (ADR 12.7.26b §5),
//! updated for the ADR 12.7.26a provenance rule: references resolved by
//! (own-module → import-table) are no longer findings; what remains is
//! exactly what real codegen hard-errors on (D1).

use std::process::ExitCode;

use super::*;

/// Write `files` into a temp dir, elaborate `entry`, and run the check's
/// core over the same codegen inputs real codegen would gather.
fn findings_for(files: &[(&str, &str)], entry: &str) -> Vec<AmbiguousReference> {
    let dir = tempfile::TempDir::new().unwrap();
    for (name, content) in files {
        let path = dir.path().join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, content).unwrap();
    }
    let entry_path = dir.path().join(entry);
    let project = driver::elaborate_project(&entry_path, false, 20, None).unwrap();
    let source_root = entry_path.parent().unwrap();
    let inputs = gather_codegen_inputs(
        &project.codegen_units,
        source_root,
        &CompileFlags::default(),
        &project,
    )
    .unwrap();
    find_ambiguous_references(&project.codegen_units, source_root, &inputs, &project)
}

/// The `def_key`s of a finding's candidates, in report order.
fn candidate_keys(finding: &AmbiguousReference) -> Vec<&str> {
    let candidates: &[AmbiguityCandidate] = &finding.candidates;
    candidates.iter().map(|c| c.def_key.as_str()).collect()
}

/// The LLVM symbols of the candidates the clobber-last map selects.
fn clobber_winner_symbols(finding: &AmbiguousReference) -> Vec<&str> {
    let candidates: &[AmbiguityCandidate] = &finding.candidates;
    candidates
        .iter()
        .filter(|c| c.clobber_winner)
        .map(|c| c.llvm_symbol.as_str())
        .collect()
}

/// The colliding pair used by most fixtures below.
const MODULE_A_DESCRIBE: &str = "pub fn describe() -> Nat {\n    11\n}\n";
const MODULE_B_DESCRIBE: &str =
    "pub fn describe() -> Nat {\n    22\n}\n\npub fn label() -> Nat {\n    33\n}\n";

/// The original ADR 12.7.26a reproducer — `main` imports `a`'s `describe` —
/// is RESOLVED by the import-table rule and no longer reported.
#[test]
fn imported_colliding_reference_resolved_not_reported() {
    let main = "mod a;\nmod b;\n\nuse a::{describe};\nuse b::{label};\n\nfn main() -> Nat {\n    describe() + label()\n}\n";
    let findings = findings_for(
        &[
            ("a.tg", MODULE_A_DESCRIBE),
            ("b.tg", MODULE_B_DESCRIBE),
            ("main.tg", main),
        ],
        "main.tg",
    );
    assert!(
        findings.is_empty(),
        "the imported reference resolves via the 12.7.26a import table: {findings:?}"
    );
}

/// An UNIMPORTED reference to a colliding name has no provenance — reported,
/// with the clobber-last pick marked (D4).
#[test]
fn unimported_colliding_reference_reported() {
    let main = "mod a;\nmod b;\n\nfn main() -> Nat {\n    describe()\n}\n";
    let findings = findings_for(
        &[
            ("a.tg", MODULE_A_DESCRIBE),
            ("b.tg", MODULE_B_DESCRIBE),
            ("main.tg", main),
        ],
        "main.tg",
    );
    assert_eq!(findings.len(), 1, "{findings:?}");
    let finding = &findings[0];
    assert_eq!(finding.unit, "main__tungsten_main");
    assert_eq!(finding.referenced_name, "describe");
    assert_eq!(finding.mono_instance, None);
    assert_eq!(
        candidate_keys(finding),
        ["a__describe::describe", "b__describe::describe"]
    );
    assert_eq!(
        clobber_winner_symbols(finding),
        ["b__describe__describe"],
        "clobber-last picks the key-sort-last candidate"
    );
}

/// A depot instance calling a colliding name its OWN module defines is
/// resolved by the own-module rule and must not be reported.
#[test]
fn depot_instance_own_module_colliding_call_not_reported() {
    let module_a =
        "pub fn describe() -> Nat {\n    11\n}\n\npub fn tag_of<T>(x: T) -> Nat {\n    describe()\n}\n";
    let module_b =
        "pub fn describe() -> Nat {\n    22\n}\n\npub fn label_of<T>(x: T) -> Nat {\n    describe()\n}\n";
    let main = "mod a;\nmod b;\n\nuse a::{tag_of};\nuse b::{label_of};\n\nfn main() -> Nat {\n    tag_of(true) + label_of(7)\n}\n";
    let findings = findings_for(
        &[("a.tg", module_a), ("b.tg", module_b), ("main.tg", main)],
        "main.tg",
    );
    assert!(
        findings.is_empty(),
        "own-module depot calls are resolved, not ambiguous: {findings:?}"
    );
}

/// A generic body calling an IMPORTED colliding name resolves through the
/// instance module's import table (ADR 12.7.26a D5) — no longer reported.
#[test]
fn depot_instance_imported_colliding_call_resolved() {
    let util_a = "pub fn pick() -> Nat {\n    1\n}\n";
    let util_b = "pub fn pick() -> Nat {\n    2\n}\n";
    let generic = "use util_a::{pick};\n\npub fn choose<T>(x: T) -> Nat {\n    pick()\n}\n";
    let main = "mod util_a;\nmod util_b;\nmod gen;\n\nuse gen::{choose};\n\nfn main() -> Nat {\n    choose(5)\n}\n";
    let findings = findings_for(
        &[
            ("util_a.tg", util_a),
            ("util_b.tg", util_b),
            ("gen.tg", generic),
            ("main.tg", main),
        ],
        "main.tg",
    );
    assert!(
        findings.is_empty(),
        "the instance's import table selects util_a (D5): {findings:?}"
    );
}

/// A `gen/mod.tg`-style depot instance calling its own-module colliding name
/// now resolves: `DefInfo.module_path` matching replaced the
/// `module_path.join(\"__\")` unit-name reconstruction that missed for
/// `mod.tg` modules (ADR 12.7.26a D4).
#[test]
fn depot_instance_mod_tg_own_module_call_resolved() {
    let gen_mod =
        "pub fn pick() -> Nat {\n    1\n}\n\npub fn choose<T>(x: T) -> Nat {\n    pick()\n}\n";
    let other = "pub fn pick() -> Nat {\n    2\n}\n";
    let main =
        "mod gen;\nmod other;\n\nuse gen::{choose};\n\nfn main() -> Nat {\n    choose(5)\n}\n";
    let findings = findings_for(
        &[
            ("gen/mod.tg", gen_mod),
            ("other.tg", other),
            ("main.tg", main),
        ],
        "main.tg",
    );
    assert!(
        findings.is_empty(),
        "mod.tg own-module lookups resolve via DefInfo.module_path (D4): {findings:?}"
    );
}

/// A same-module sibling call to a colliding name resolves via the own-module
/// rule — per-function units make it a cross-unit extern, but provenance now
/// selects the sibling def instead of the clobbered entry.
#[test]
fn same_module_sibling_call_resolved() {
    let module_a =
        "pub fn describe() -> Nat {\n    11\n}\n\npub fn caller() -> Nat {\n    describe()\n}\n";
    let module_b = "pub fn describe() -> Nat {\n    22\n}\n";
    let main = "mod a;\nmod b;\n\nuse a::{caller};\n\nfn main() -> Nat {\n    caller()\n}\n";
    let findings = findings_for(
        &[("a.tg", module_a), ("b.tg", module_b), ("main.tg", main)],
        "main.tg",
    );
    assert!(
        findings.is_empty(),
        "same-module sibling calls resolve via the own-module rule: {findings:?}"
    );
}

/// Double-alias imports of the same original name stay ambiguous (D3):
/// elaboration marks the import target `Ambiguous`, so the reference is
/// reported rather than resolved to either alias.
#[test]
fn double_alias_import_reported_as_ambiguous() {
    let main = "mod a;\nmod b;\n\nuse a::{describe as da};\nuse b::{describe as db};\n\nfn main() -> Nat {\n    da() + db()\n}\n";
    let findings = findings_for(
        &[
            ("a.tg", MODULE_A_DESCRIBE),
            ("b.tg", MODULE_B_DESCRIBE),
            ("main.tg", main),
        ],
        "main.tg",
    );
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0].referenced_name, "describe");
    assert_eq!(candidate_keys(&findings[0]).len(), 2);
}

/// Colliding defs that no compiled body references produce no findings —
/// collisions alone are not ambiguity.
#[test]
fn unreferenced_collisions_report_nothing() {
    let main = "mod a;\nmod b;\n\nfn main() -> Nat {\n    7\n}\n";
    let findings = findings_for(
        &[
            ("a.tg", MODULE_A_DESCRIBE),
            ("b.tg", MODULE_B_DESCRIBE),
            ("main.tg", main),
        ],
        "main.tg",
    );
    assert!(findings.is_empty(), "{findings:?}");
}

/// `--json` output round-trips through serde, and the human render names
/// unit, referenced name, every candidate, and the winner.
#[test]
fn json_round_trips_and_human_render_names_everything() {
    let findings = vec![AmbiguousReference {
        unit: "main__tungsten_main".to_string(),
        mono_instance: None,
        referenced_name: "describe".to_string(),
        candidates: vec![
            AmbiguityCandidate {
                def_key: "a__describe::describe".to_string(),
                llvm_symbol: "a__describe__describe".to_string(),
                clobber_winner: false,
            },
            AmbiguityCandidate {
                def_key: "b__describe::describe".to_string(),
                llvm_symbol: "b__describe__describe".to_string(),
                clobber_winner: true,
            },
        ],
    }];
    let json = serde_json::to_string(&findings).unwrap();
    let round_tripped: Vec<AmbiguousReference> = serde_json::from_str(&json).unwrap();
    assert_eq!(round_tripped.len(), 1);
    assert_eq!(round_tripped[0].unit, findings[0].unit);
    assert_eq!(
        round_tripped[0].referenced_name,
        findings[0].referenced_name
    );
    assert_eq!(
        candidate_keys(&round_tripped[0]),
        candidate_keys(&findings[0])
    );
    assert_eq!(
        clobber_winner_symbols(&round_tripped[0]),
        ["b__describe__describe"]
    );

    let human = render_human(&findings, 4);
    assert!(human.contains("main__tungsten_main"));
    assert!(human.contains("`describe`"));
    assert!(human.contains("a__describe::describe"));
    assert!(human.contains("b__describe::describe"));
    assert!(human.contains("clobber-last picks this"));
}

/// Exit code is non-zero iff findings exist (D1). The dirty fixture uses an
/// UNIMPORTED colliding reference — imported ones now resolve (12.7.26a).
#[test]
fn exit_code_gates_on_findings() {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::write(dir.path().join("a.tg"), MODULE_A_DESCRIBE).unwrap();
    std::fs::write(dir.path().join("b.tg"), MODULE_B_DESCRIBE).unwrap();
    std::fs::write(
        dir.path().join("main.tg"),
        "mod a;\nmod b;\n\nfn main() -> Nat {\n    describe()\n}\n",
    )
    .unwrap();
    let dirty = cmd_check_extern_map_ambiguity(&dir.path().join("main.tg"), false, false, 20);
    assert_eq!(dirty, ExitCode::FAILURE);

    let clean_dir = tempfile::TempDir::new().unwrap();
    std::fs::write(clean_dir.path().join("a.tg"), MODULE_A_DESCRIBE).unwrap();
    std::fs::write(clean_dir.path().join("b.tg"), MODULE_B_DESCRIBE).unwrap();
    std::fs::write(
        clean_dir.path().join("main.tg"),
        "mod a;\nmod b;\n\nuse a::{describe};\n\nfn main() -> Nat {\n    describe()\n}\n",
    )
    .unwrap();
    let clean = cmd_check_extern_map_ambiguity(&clean_dir.path().join("main.tg"), false, false, 20);
    assert_eq!(clean, ExitCode::SUCCESS);
}
