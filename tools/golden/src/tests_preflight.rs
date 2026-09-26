//! Tests for the run-level binary preflight (ADR 21.7.26f / D3).
//!
//! Each case builds a stub "compiler" that reproduces one of the skew shapes
//! the runner used to conflate. Tests: <this file>.

use super::{probe, Preflight};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Write an executable shell script standing in for the compiler binary.
fn stub_compiler(dir: &TempDir, body: &str) -> PathBuf {
    let path = dir.path().join("tungsten");
    let mut f = fs::File::create(&path).unwrap();
    writeln!(f, "#!/bin/sh\n{body}").unwrap();
    drop(f);
    make_executable(&path);
    path
}

fn make_executable(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
}

#[test]
fn a_binary_listing_compile_is_healthy() {
    let dir = TempDir::new().unwrap();
    let stub = stub_compiler(
        &dir,
        "echo 'check Check a file'\necho 'compile Compile a file'",
    );
    assert_eq!(probe(&stub), Preflight::Healthy);
}

#[test]
fn a_binary_without_compile_is_a_codegen_less_build() {
    // The feature-clobber shape: the binary runs fine, it just has no
    // `compile` subcommand.
    let dir = TempDir::new().unwrap();
    let stub = stub_compiler(&dir, "echo 'check Check a file'\necho 'test Run tests'");
    assert_eq!(probe(&stub), Preflight::NoCompileSubcommand);
}

#[test]
fn a_subcommand_named_like_compile_does_not_count() {
    // `compile-only` is a different subcommand; matching on a bare prefix
    // would silently accept a build that cannot compile.
    let dir = TempDir::new().unwrap();
    let stub = stub_compiler(&dir, "echo 'compiler-info Show info'");
    assert_eq!(probe(&stub), Preflight::NoCompileSubcommand);
}

#[test]
fn a_non_executable_file_is_not_runnable() {
    // Stands in for the wrong-platform ELF in the bind-mounted target/release.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("tungsten");
    fs::write(&path, "not a binary").unwrap();
    assert!(matches!(probe(&path), Preflight::NotRunnable(_)));
}

#[test]
fn a_missing_file_is_not_runnable() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("does-not-exist");
    assert!(matches!(probe(&path), Preflight::NotRunnable(_)));
}

#[test]
fn a_binary_that_cannot_list_its_commands_is_not_runnable() {
    // Running but failing is not a codegen-less build — no category could use
    // this binary either, so it must abort the run rather than skip a category.
    let dir = TempDir::new().unwrap();
    let stub = stub_compiler(&dir, "echo 'boom' >&2\nexit 2");
    match probe(&stub) {
        Preflight::NotRunnable(reason) => assert!(reason.contains("boom"), "{reason}"),
        other => panic!("expected NotRunnable, got {other:?}"),
    }
}

// ── consequences ────────────────────────────────────────────────────────────

#[test]
fn only_a_healthy_binary_enables_the_compile_category() {
    assert!(Preflight::Healthy.codegen_available());
    assert!(!Preflight::NoCompileSubcommand.codegen_available());
    assert!(!Preflight::NotRunnable("x".into()).codegen_available());
}

#[test]
fn only_an_unrunnable_binary_aborts_the_run() {
    // A codegen-less build must keep today's exit code — skipping the compile
    // category is legitimate on an LLVM-less host.
    assert!(Preflight::NotRunnable("x".into()).is_fatal());
    assert!(!Preflight::NoCompileSubcommand.is_fatal());
    assert!(!Preflight::Healthy.is_fatal());
}

// ── banners ─────────────────────────────────────────────────────────────────

#[test]
fn the_healthy_banner_is_just_the_probe_line() {
    let banner = Preflight::Healthy.banner(Path::new("./target/release/tungsten"));
    assert_eq!(banner, "[golden] preflight: ./target/release/tungsten");
}

#[test]
fn the_unrunnable_banner_names_the_skew_and_its_remedy() {
    let banner =
        Preflight::NotRunnable("exec format error".into()).banner(Path::new("./t/tungsten"));
    assert!(banner.contains("./t/tungsten"), "{banner}");
    assert!(banner.contains("exec format error"), "{banner}");
    assert!(banner.contains("ADR 2.7.26b"), "{banner}");
    assert!(banner.contains("make release"), "{banner}");
}

#[test]
fn the_codegen_less_banner_names_the_feature_clobber_and_its_remedy() {
    let banner = Preflight::NoCompileSubcommand.banner(Path::new("./t/tungsten"));
    assert!(banner.contains("codegen-less build"), "{banner}");
    assert!(banner.contains("feature clobber"), "{banner}");
    assert!(
        banner.contains("cargo build -p tungsten_bootstrap --features codegen --bin tungsten"),
        "{banner}"
    );
}
