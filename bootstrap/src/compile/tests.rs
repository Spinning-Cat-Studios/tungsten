//! Tests for the compile command's --no-codegen flag.

use super::{cmd_compile, CompileFlags, DiagnosticFlags, TraceFlags};
use std::fs;
use std::process::ExitCode;
use tempfile::TempDir;

/// Default flags with everything off.
fn default_flags() -> CompileFlags {
    CompileFlags {
        emit_llvm: false,
        verbose: false,
        max_errors: 20,
        dump_types: false,
        debug_info: false,
        sanitize: false,
        named_lambdas: false,
        no_codegen: false,
        diagnostics: DiagnosticFlags {
            dump_ir: None,
            trace_types: None,
            dump_encoding: None,
            codegen_backtrace: false,
            check_tyvar_escape: false,
            alloc_profile: None,
            tracing: TraceFlags {
                trace_adt_ops: None,
                trace_encoding: None,
                trace_normalization: None,
                trace_constructor_registration: false,
                trace_musttail: false,
                trace_escape: false,
                trace_mono: false,
            },
        },
        codegen_jobs: 1,
    }
}

/// Write a minimal .tg source file and return its path + temp dir handle.
fn write_test_file(source: &str) -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test.tg");
    fs::write(&path, source).unwrap();
    (dir, path)
}

#[test]
fn no_codegen_succeeds_on_valid_source() {
    let (_dir, path) = write_test_file("fn main() -> Nat { 42 }");
    let flags = CompileFlags {
        no_codegen: true,
        ..default_flags()
    };
    assert_eq!(cmd_compile(&path, None, &flags), ExitCode::SUCCESS);
}

#[test]
fn no_codegen_fails_on_type_error() {
    // "hello" is a String, not a Nat — should fail during elaboration
    let (_dir, path) = write_test_file("fn main() -> Nat { \"hello\" }");
    let flags = CompileFlags {
        no_codegen: true,
        ..default_flags()
    };
    assert_eq!(cmd_compile(&path, None, &flags), ExitCode::FAILURE);
}

#[test]
fn no_codegen_and_emit_llvm_incompatible() {
    let (_dir, path) = write_test_file("fn main() -> Nat { 0 }");
    let flags = CompileFlags {
        no_codegen: true,
        emit_llvm: true,
        ..default_flags()
    };
    assert_eq!(cmd_compile(&path, None, &flags), ExitCode::FAILURE);
}

#[test]
fn no_codegen_with_dump_ir_succeeds() {
    let (_dir, path) = write_test_file("fn main() -> Nat { 0 }");
    let mut flags = CompileFlags {
        no_codegen: true,
        ..default_flags()
    };
    flags.diagnostics.dump_ir = Some("main".to_string());
    assert_eq!(cmd_compile(&path, None, &flags), ExitCode::SUCCESS);
}

#[test]
fn no_codegen_with_dump_encoding_succeeds() {
    let (_dir, path) = write_test_file("type Option<T> = None | Some(T)\nfn main() -> Nat { 0 }");
    let mut flags = CompileFlags {
        no_codegen: true,
        ..default_flags()
    };
    flags.diagnostics.dump_encoding = Some("Option".to_string());
    assert_eq!(cmd_compile(&path, None, &flags), ExitCode::SUCCESS);
}

#[test]
fn no_codegen_without_main_succeeds() {
    // Without --no-codegen this would fail (no main function).
    // --no-codegen exits before the main function check.
    let (_dir, path) = write_test_file("fn helper() -> Nat { 42 }");
    let flags = CompileFlags {
        no_codegen: true,
        ..default_flags()
    };
    assert_eq!(cmd_compile(&path, None, &flags), ExitCode::SUCCESS);
}

// ═══════════════════════════════════════════════════════════════════════
// extern_wrap_name tests (ADR 6.5.26d §2.1)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn extern_wrap_name_returns_none_for_non_extern() {
    use tungsten_core::Term;
    // A plain Zero term is not an extern wrapper
    let result = super::extern_wrap_name("my_fn", &Term::Zero);
    assert_eq!(result, None);
}

#[test]
fn extern_wrap_name_returns_pair_for_extern_call() {
    use tungsten_core::Term;
    // ExternCall with __c_ prefix
    let term = Term::ExternCall("__c_puts".to_string(), vec![]);
    let result = super::extern_wrap_name("tg_puts", &term);
    assert_eq!(
        result,
        Some(("tg_puts".to_string(), "__wrap_puts".to_string()))
    );
}

#[test]
fn extern_wrap_name_strips_c_prefix_only() {
    use tungsten_core::Term;
    // ExternCall without __c_ prefix — wrap uses full symbol name
    let term = Term::ExternCall("malloc".to_string(), vec![]);
    let result = super::extern_wrap_name("alloc", &term);
    assert_eq!(
        result,
        Some(("alloc".to_string(), "__wrap_malloc".to_string()))
    );
}

#[test]
fn extern_wrap_name_through_lambda() {
    use tungsten_core::{Term, Type};
    // Lambda wrapping an ExternCall
    let body = Box::new(Term::ExternCall("__c_exit".to_string(), vec![]));
    let term = Term::Lambda("x".to_string(), Type::Nat, body);
    let result = super::extern_wrap_name("tg_exit", &term);
    assert_eq!(
        result,
        Some(("tg_exit".to_string(), "__wrap_exit".to_string()))
    );
}

#[test]
fn extern_wrap_name_renames_main() {
    use tungsten_core::Term;
    // "main" def should become "tungsten_main" in the original name
    let term = Term::ExternCall("__c_main".to_string(), vec![]);
    let result = super::extern_wrap_name("main", &term);
    assert_eq!(
        result,
        Some(("tungsten_main".to_string(), "__wrap_main".to_string()))
    );
}

// ═══════════════════════════════════════════════════════════════════════
// parse_codegen_jobs tests (ADR 9.5.26e §P3)
// ═══════════════════════════════════════════════════════════════════════

// Env var tests must be serialized — concurrent set_var/remove_var is UB.
use std::sync::Mutex;
static ENV_MUTEX: Mutex<()> = Mutex::new(());

#[test]
fn parse_codegen_jobs_default_is_positive() {
    let _lock = ENV_MUTEX.lock().unwrap();
    // Without env var set, should return a positive number (num_cpus or fallback 4)
    std::env::remove_var("TUNGSTEN_CODEGEN_JOBS");
    let jobs = super::parse_codegen_jobs();
    assert!(
        jobs >= 1,
        "default codegen_jobs should be >= 1, got {}",
        jobs
    );
}

#[test]
fn parse_codegen_jobs_reads_env_var() {
    let _lock = ENV_MUTEX.lock().unwrap();
    std::env::set_var("TUNGSTEN_CODEGEN_JOBS", "3");
    let jobs = super::parse_codegen_jobs();
    std::env::remove_var("TUNGSTEN_CODEGEN_JOBS");
    assert_eq!(jobs, 3);
}

#[test]
fn parse_codegen_jobs_clamps_zero_to_one() {
    let _lock = ENV_MUTEX.lock().unwrap();
    std::env::set_var("TUNGSTEN_CODEGEN_JOBS", "0");
    let jobs = super::parse_codegen_jobs();
    std::env::remove_var("TUNGSTEN_CODEGEN_JOBS");
    assert_eq!(jobs, 1, "TUNGSTEN_CODEGEN_JOBS=0 should clamp to 1");
}

#[test]
fn parse_codegen_jobs_invalid_falls_back_to_default() {
    let _lock = ENV_MUTEX.lock().unwrap();
    std::env::set_var("TUNGSTEN_CODEGEN_JOBS", "banana");
    let jobs = super::parse_codegen_jobs();
    std::env::remove_var("TUNGSTEN_CODEGEN_JOBS");
    assert!(
        jobs >= 1,
        "invalid env var should fall back to default (>= 1), got {}",
        jobs
    );
}
