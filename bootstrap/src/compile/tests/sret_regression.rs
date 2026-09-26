//! ADR 3.7.26d AC2 — automated sret-stores regression on the 3.7.26a fixture.
//!
//! Emits LLVM IR for `tests/dead_arm_letelse_run.tg` (the program that the
//! pre-3.7.26a compiler silently miscompiled by discarding an sret result)
//! and runs the `doctor check ir sret-stores` audit over it: zero findings
//! on the fixed compiler, and at least one sret function actually audited
//! (guards against a vacuous pass from parser/format drift).

use crate::compile::{cmd_compile, CompileFlags};
use std::path::PathBuf;
use std::process::ExitCode;
use tungsten_bootstrap::doctor::checks::check_sret_stores::audit_ir;

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tests/dead_arm_letelse_run.tg")
}

fn collect_ll(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_ll(&path, out);
        } else if path.extension().is_some_and(|e| e == "ll") {
            out.push(path);
        }
    }
}

#[test]
fn dead_arm_fixture_ir_has_no_sret_findings() {
    let fixture = fixture_path();
    assert!(fixture.exists(), "fixture missing: {}", fixture.display());

    let out = tempfile::TempDir::new().unwrap();
    let flags = CompileFlags {
        emit_llvm: true,
        max_errors: 20,
        codegen_jobs: 1,
        ..CompileFlags::default()
    };
    assert_eq!(
        cmd_compile(&fixture, Some(out.path()), &flags),
        ExitCode::SUCCESS,
        "emit-llvm compile of the fixture failed"
    );

    let mut files = Vec::new();
    collect_ll(out.path(), &mut files);
    assert!(
        !files.is_empty(),
        "no .ll files emitted under {}",
        out.path().display()
    );

    let mut sret_functions = 0usize;
    let mut findings = Vec::new();
    for path in &files {
        let text = std::fs::read_to_string(path).unwrap();
        let summary = audit_ir(&text);
        sret_functions += summary.sret_functions();
        for f in summary.findings {
            findings.push((path.display().to_string(), f));
        }
    }
    assert!(
        findings.is_empty(),
        "sret-stores findings on the fixed compiler: {findings:?}"
    );
    assert!(
        sret_functions > 0,
        "vacuous pass: no sret functions audited across {} .ll files — parser/format drift?",
        files.len()
    );
}
