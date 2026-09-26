//! Tests for `tungsten doctor check extern-coverage`.

use std::path::PathBuf;
use std::process::ExitCode;

use super::{cmd_check_extern_coverage, partition_by_executability, render_report, DeclaredExtern};

fn declared(symbol: &str) -> DeclaredExtern {
    DeclaredExtern {
        symbol: symbol.to_string(),
        file: "m.tg".to_string(),
        offset: 7,
    }
}

/// Write `source` to a temp `.tg` and run the check on it.
fn check(source: &str) -> (ExitCode, PathBuf, tempfile::TempDir) {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("m.tg");
    std::fs::write(&path, source).unwrap();
    let code = cmd_check_extern_coverage(&path, false);
    (code, path, dir)
}

/// The classification: registered externs are executable, unregistered ones
/// are not.
#[test]
fn partition_splits_on_the_registry() {
    let (executable, stuck) =
        partition_by_executability(&[declared("tg_println"), declared("tg_not_a_real_extern")]);
    assert_eq!(executable.len(), 1);
    assert_eq!(executable[0].symbol, "tg_println");
    assert_eq!(stuck.len(), 1);
    assert_eq!(stuck[0].symbol, "tg_not_a_real_extern");
}

/// A file declaring only executable externs is clean, exit 0.
#[test]
fn a_fully_supported_file_passes() {
    let (code, ..) = check(
        "extern \"C\" fn tg_println(s: Nat, len: Nat) -> Unit\n\
         extern \"C\" fn tg_free_string(s: Nat) -> Unit\n\
         fn main() -> Nat { 0 }\n",
    );
    assert_eq!(code, ExitCode::SUCCESS);
}

/// An unsupported extern is a finding, exit 2 — distinct from a hard failure.
#[test]
fn an_unsupported_extern_is_a_finding() {
    let (code, ..) = check(
        "extern \"C\" fn tg_no_such_primitive(x: Nat) -> Unit\n\
         fn main() -> Nat { 0 }\n",
    );
    assert_eq!(code, ExitCode::from(2));
}

/// A file with no externs at all is clean — the check must not invent findings.
#[test]
fn a_file_without_externs_passes() {
    let (code, ..) = check("fn main() -> Nat { 0 }\n");
    assert_eq!(code, ExitCode::SUCCESS);
}

/// An unparseable file is a hard failure (1), not a finding (2) — the two must
/// stay distinguishable for CI.
#[test]
fn an_unreadable_file_is_a_hard_failure() {
    let dir = tempfile::TempDir::new().unwrap();
    let missing = dir.path().join("absent.tg");
    assert_eq!(
        cmd_check_extern_coverage(&missing, false),
        ExitCode::FAILURE
    );
}

/// The `symbol` override is what the evaluator dispatches on, so a declaration
/// renaming a supported symbol is still supported.
#[test]
fn the_symbol_override_is_what_gets_matched() {
    let (code, ..) = check(
        "extern \"C\" fn my_print = \"tg_println\"(s: Nat, len: Nat) -> Unit\n\
         fn main() -> Nat { 0 }\n",
    );
    // If the parser does not accept this form the file fails to parse (exit 1);
    // what must never happen is a false CLEAN on an unsupported symbol.
    assert_ne!(
        code,
        ExitCode::from(2),
        "a declaration aliased to a supported symbol must not be reported stuck"
    );
}

/// The report names the affected symbol and its offset, so `map-span` can
/// locate it.
#[test]
fn the_report_names_the_symbol_and_offset() {
    let rendered = render_report(
        &PathBuf::from("m.tg"),
        &[],
        &[declared("tg_mystery")],
        false,
    );
    assert!(rendered.contains("tg_mystery"));
    assert!(rendered.contains("offset 7"));
}

/// The report states the failure is SILENT and scopes it to the evaluator —
/// without both, a reader cannot tell whether it matters to them.
#[test]
fn the_report_explains_silence_and_scope() {
    let rendered = render_report(
        &PathBuf::from("m.tg"),
        &[],
        &[declared("tg_mystery")],
        false,
    );
    assert!(rendered.contains("silently Stuck"));
    assert!(
        rendered.contains("Native codegen is unaffected"),
        "must scope the finding, or it reads as a native bug"
    );
    assert!(
        rendered.contains("registry.rs"),
        "must name both edit sites, or a fixer updates dispatch and not the registry"
    );
}

/// Every source path the remediation names must still exist.
///
/// It stopped being true once already: the report kept pointing at
/// `extern_call.rs` / `extern_console.rs` / `extern_registry.rs` long after
/// those became `externs/call.rs` and siblings, so the one instruction a
/// blocked reader follows sent them to three files that were not there. A
/// string assertion cannot catch that — only resolving the path can (ADR
/// 7.8.26c).
#[test]
fn the_remediation_names_paths_that_exist() {
    let rendered = render_report(
        &PathBuf::from("m.tg"),
        &[],
        &[declared("tg_mystery")],
        false,
    );
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the bootstrap crate sits one level below the workspace root")
        .to_path_buf();
    // Self-skip where the corpus it resolves against is absent (ADR 31.7.26c).
    // `mutant-schemata` copies the workspace with `docs/` in `SKIPPED_AT_ROOT`,
    // so this failed the schema BASELINE there — and a failed baseline aborts
    // the whole crate's sweep before a single mutant runs, which is how the
    // bootstrap mutation lane sat unrunnable unnoticed (nothing had pulled
    // `bootstrap/**` into a diff window since the doc it names landed).
    // Guarding on the DIRECTORY, not the file, keeps the assertion sharp
    // in-tree: a remediation naming a missing file still fails here.
    if !workspace_root.join("docs/repo-memory").is_dir() {
        return;
    }

    let named: Vec<&str> = rendered
        .split(|c: char| !(c.is_alphanumeric() || "._/-".contains(c)))
        .filter(|token| token.contains('/') && (token.ends_with(".rs") || token.ends_with(".md")))
        .collect();
    assert!(
        !named.is_empty(),
        "the remediation must name at least one file, or there is nothing to follow"
    );

    for path in named {
        assert!(
            workspace_root.join(path).exists(),
            "the remediation points at `{path}`, which does not exist — a reader \
             following it lands nowhere"
        );
    }
}

/// A clean report says so and does not print the remediation wall.
#[test]
fn a_clean_report_is_quiet() {
    let rendered = render_report(
        &PathBuf::from("m.tg"),
        &[declared("tg_println")],
        &[],
        false,
    );
    assert!(rendered.contains("✓"));
    assert!(!rendered.contains("silently Stuck"));
}

/// No declarations at all is reported distinctly from "all supported" — they
/// mean different things to a reader.
#[test]
fn no_declarations_reports_distinctly() {
    let rendered = render_report(&PathBuf::from("m.tg"), &[], &[], false);
    assert!(rendered.contains("No `extern \"C\"` declarations"));
}

/// `--verbose` lists the executable ones with their kind; the default does not.
#[test]
fn verbose_lists_the_executable_externs() {
    let quiet = render_report(
        &PathBuf::from("m.tg"),
        &[declared("tg_println")],
        &[],
        false,
    );
    let loud = render_report(&PathBuf::from("m.tg"), &[declared("tg_println")], &[], true);
    assert!(!quiet.contains("[console]"));
    assert!(loud.contains("Executable:"));
    assert!(loud.contains("[console]"));
}

/// The whole console chain in a realistic `println` wrapper is clean — the
/// regression guard for ADR 28.7.26a's finding.
#[test]
fn the_console_chain_is_reported_supported() {
    let (code, ..) = check(
        "extern \"C\" fn tg_println(s: Nat, len: Nat) -> Unit\n\
         extern \"C\" fn tg_string_to_cstring(s: String) -> Nat\n\
         extern \"C\" fn tg_string_len_internal(s: String) -> Nat\n\
         extern \"C\" fn tg_free_string(s: Nat) -> Unit\n\
         fn main() -> Nat { 0 }\n",
    );
    assert_eq!(
        code,
        ExitCode::SUCCESS,
        "the console chain must be fully executable (ADR 28.7.26a)"
    );
}

/// A mostly-unsupported file gets the "normal for native codegen" note — the
/// self-hosted compiler reports 140 of 150 and must not read as a broken check.
#[test]
fn a_mostly_unsupported_file_gets_the_proportionality_note() {
    let rendered = render_report(
        &PathBuf::from("m.tg"),
        &[declared("tg_println")],
        &[declared("tg_read_file"), declared("tg_write_file")],
        false,
    );
    assert!(
        rendered.contains("normal for a"),
        "expected the NOTE, got:\n{rendered}"
    );
    assert!(rendered.contains("NATIVE codegen"));
}

/// A file with only one unsupported extern among many does NOT get the note —
/// there the finding is the signal, and the caveat would dilute it.
#[test]
fn a_mostly_supported_file_omits_the_proportionality_note() {
    let rendered = render_report(
        &PathBuf::from("m.tg"),
        &[
            declared("tg_println"),
            declared("tg_print"),
            declared("tg_free_string"),
        ],
        &[declared("tg_mystery")],
        false,
    );
    assert!(
        !rendered.contains("NATIVE codegen"),
        "a single finding should not be softened by the note"
    );
}

/// Declarations in a SUBMODULE are found too — the check walks the whole module
/// tree, and the console chain typically lives in an `ffi/` submodule rather
/// than the entry file (`src/compiler/driver/ffi/process/mod.tg` is where the real
/// one is), so a root-only walk would report a clean bill on every real project.
#[test]
fn declarations_in_submodules_are_collected() {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("m.tg"),
        "mod helper;\nfn main() -> Nat { 0 }\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("helper.tg"),
        "extern \"C\" fn tg_hidden_in_a_submodule(x: Nat) -> Unit\n",
    )
    .unwrap();

    assert_eq!(
        cmd_check_extern_coverage(&dir.path().join("m.tg"), false),
        ExitCode::from(2),
        "an unsupported extern in a submodule must still be reported"
    );
}

/// The `ExternCoverage` variant routes through `dispatch_check` to this check —
/// a mis-wired arm would leave the subcommand silently unreachable.
#[test]
fn the_check_command_dispatches_here() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("m.tg");
    std::fs::write(
        &path,
        "extern \"C\" fn tg_no_such_primitive(x: Nat) -> Unit\nfn main() -> Nat { 0 }\n",
    )
    .unwrap();

    let cmd = crate::doctor::DoctorCommands::Check(crate::doctor::CheckCommands::ExternCoverage {
        file: path,
    });
    assert_eq!(
        crate::doctor::cmd_doctor(cmd, false),
        ExitCode::from(2),
        "dispatch must reach the check and preserve its findings exit code"
    );
}

/// The proportionality note's threshold is "MOST", i.e. strictly more than
/// half — an exactly-half file does not get it.
///
/// This is the boundary the `stuck * 2 > total` test guards: at 1 of 2, `> `
/// says no note (correct) while `>=` or `stuck + 2` would say yes. Without a
/// case sitting exactly on the boundary, both of those are indistinguishable
/// from the real comparison.
#[test]
fn an_exactly_half_unsupported_file_omits_the_note() {
    let rendered = render_report(
        &PathBuf::from("m.tg"),
        &[declared("tg_println")],
        &[declared("tg_mystery")],
        false,
    );
    assert!(
        !rendered.contains("NATIVE codegen"),
        "half is not 'most' — the note should be withheld, got:\n{rendered}"
    );
}
