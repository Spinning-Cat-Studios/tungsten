//! Self-compile orchestration (IR → llc → link → smoke).
//!
//! ADR 2.7.26b T4 hardening: every compiler artifact is resolved to an
//! absolute path via [`artifacts::Artifacts`] and format-checked immediately
//! before exec; every mode ends with a mandatory smoke run of the product
//! ([`smoke::smoke_check`]); and each stage's max-RSS is logged
//! ([`telemetry::RssLogger`]).

pub mod artifacts;
mod llc;
pub mod smoke;
pub mod telemetry;
mod util;

use artifacts::{
    check_executable_format, run_steps, self_compile_steps, Artifacts, CanonicalBootstrapBuild,
    SelfCompileStep,
};
use clap::Args;
use llc::compile_ir;
use std::io;
use std::path::Path;
use std::process::Command;
use telemetry::{run_with_rusage, RssLogger};
use util::{find_files, resolve_parallelism};

#[derive(Args)]
pub struct SelfCompileArgs {
    /// Use -O0 for fast routine verification (default)
    #[arg(long)]
    pub fast: bool,

    /// Use -O2 for release verification
    #[arg(long, conflicts_with = "fast")]
    pub opt: bool,

    /// Emit .o files in-process (no llc), single-stage
    #[arg(long, conflicts_with_all = ["fast", "opt"])]
    pub direct: bool,

    /// Cross-compile for x86_64
    #[arg(long)]
    pub x86: bool,

    /// Parallelism level for codegen (default: nproc/2, OOM-safe).
    /// Precedence: --parallelism > TUNGSTEN_CODEGEN_JOBS env > nproc/2.
    #[arg(short = 'P', long)]
    pub parallelism: Option<usize>,

    /// Build the product with allocation profiling hooks (ADR 2.7.26a).
    /// Passes --alloc-profile to the Stage 1 compile so the produced
    /// binary reports per-function/per-class allocation attribution.
    #[arg(long)]
    pub alloc_profile: bool,
}

fn mode_name(args: &SelfCompileArgs) -> &'static str {
    if args.direct {
        "self-compile-direct"
    } else if args.opt {
        "self-compile-opt"
    } else {
        "self-compile-fast"
    }
}

pub fn run(args: SelfCompileArgs) -> Result<(), String> {
    let opt_level = if args.opt { "-O2" } else { "-O0" };
    let parallelism = resolve_parallelism(args.parallelism);
    let arts = Artifacts::resolve()?;
    // Surface the resolved build directory (ADR 24.7.26b): an isolated
    // container volume via $CARGO_TARGET_DIR (e.g. /build/target) vs the
    // host-shared <root>/target fallback. If isolation is misconfigured this
    // prints `…/target` instead of the volume path, making the fault loud
    // before the (slow) self-compile rather than at a later format-guard trip.
    eprintln!("  [target] build dir: {}", arts.target_dir.display());
    let rss = RssLogger::create(mode_name(&args), &arts.log_dir, &arts.fallback_log_dir);

    // ADR 24.9.26a: the plan opens with the canonical bootstrap build, and
    // every stage that runs the bootstrap takes that build's artifact.
    let steps = self_compile_steps(&arts.target_dir, args.direct);
    run_steps(&steps, |step| match step {
        SelfCompileStep::BuildBootstrap(build) => run_canonical_build(build, &arts.root),
        SelfCompileStep::DirectCompile { compiler } => {
            run_direct(&arts, compiler, parallelism, args.alloc_profile, &rss)
        }
        SelfCompileStep::EmitIr { compiler } => {
            eprintln!(
                "=== Stage 1/3: Generating LLVM IR (per-file, TUNGSTEN_CODEGEN_JOBS={parallelism}) ==="
            );
            emit_ir(&arts, compiler, parallelism, args.alloc_profile, &rss)
        }
        SelfCompileStep::CompileIr => {
            eprintln!(
                "\n=== Stage 2/3: Compiling IR to object files (llc {opt_level}, -P{parallelism}) ==="
            );
            compile_ir(&arts, opt_level, parallelism, &rss)
        }
        SelfCompileStep::Link => {
            eprintln!("\n=== Stage 3/3: Linking ===");
            link(&arts, &rss)?;
            cleanup_ir(&arts)
        }
    })?;

    finish(
        &arts,
        if args.direct {
            "direct .o emission"
        } else {
            opt_level
        },
    )
}

/// `tungsten-dev ensure-bootstrap`: the canonical bootstrap build alone, so
/// make recipes reach the same command without restating it (ADR 24.9.26a).
pub fn ensure_bootstrap() -> Result<(), String> {
    let arts = Artifacts::resolve()?;
    let build = CanonicalBootstrapBuild::for_target_dir(&arts.target_dir);
    run_canonical_build(&build, &arts.root)?;
    eprintln!("  [bootstrap] fresh: {}", build.artifact.display());
    Ok(())
}

/// Run the canonical build from the workspace root, inheriting stdio so
/// Cargo's progress is visible; a failure stops the caller.
fn run_canonical_build(build: &CanonicalBootstrapBuild, root: &Path) -> Result<(), String> {
    eprintln!("=== Bootstrap freshness: {} ===", build.command_line());
    let status = Command::new(&build.argv[0])
        .args(&build.argv[1..])
        .current_dir(root)
        .status()
        .map_err(|e| {
            format!(
                "could not start `{}`: {e} — refusing to run the existing {} (ADR 24.9.26a)",
                build.command_line(),
                build.artifact.display()
            )
        })?;
    build.check_outcome(status.code())
}

/// Mandatory post-link gate: format-check + smoke-run the product before
/// declaring success (ADR 2.7.26b T4a/T4b — a binary that crashed in 19 ms
/// once shipped behind "✓ Built").
fn finish(arts: &Artifacts, mode: &str) -> Result<(), String> {
    check_executable_format(&arts.output_binary)?;
    smoke::smoke_check(&arts.output_binary, &arts.smoke_input)?;
    eprintln!(
        "\n✓ Built {} ({mode}, smoke-checked)",
        arts.output_binary.display()
    );
    Ok(())
}

fn run_direct(
    arts: &Artifacts,
    compiler: &Path,
    parallelism: usize,
    alloc_profile: bool,
    rss: &RssLogger,
) -> Result<(), String> {
    eprintln!("=== Direct compile (in-process .o emission, {parallelism} jobs) ===");

    check_executable_format(compiler)?;
    let mut cmd = Command::new(compiler);
    cmd.current_dir(&arts.root)
        .arg("compile")
        .arg(&arts.entry)
        .arg("-o")
        .arg(&arts.output_binary)
        .arg("-v")
        .env("TUNGSTEN_CODEGEN_JOBS", parallelism.to_string());
    if alloc_profile {
        cmd.arg("--alloc-profile");
    }

    let output = run_with_rusage(&mut cmd).map_err(|e| format!("failed to run tungsten: {e}"))?;
    rss.log_stage("direct-compile", output.max_rss_kb);

    if !output.stdout.is_empty() {
        io::Write::write_all(&mut io::stdout(), &output.stdout)
            .map_err(|e| format!("write stdout: {e}"))?;
    }

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "direct compile failed (exit {}): {stderr}",
            output.status.code().unwrap_or(-1)
        ));
    }

    Ok(())
}

fn emit_ir(
    arts: &Artifacts,
    compiler: &Path,
    parallelism: usize,
    alloc_profile: bool,
    rss: &RssLogger,
) -> Result<(), String> {
    // Clean and recreate IR directory
    let _ = std::fs::remove_dir_all(&arts.ir_dir);
    std::fs::create_dir_all(&arts.ir_dir)
        .map_err(|e| format!("failed to create {}: {e}", arts.ir_dir.display()))?;

    let log_path = arts.log_dir.join("self-compile-fast.stderr.log");

    check_executable_format(compiler)?;
    let mut cmd = Command::new(compiler);
    cmd.current_dir(&arts.root)
        .arg("compile")
        .arg(&arts.entry)
        .arg("--emit-llvm")
        .arg("-o")
        .arg(&arts.ir_dir)
        .arg("-v");
    if alloc_profile {
        cmd.arg("--alloc-profile");
    }
    // Forward parallelism cap to Stage 1 (in-process LLVM codegen).
    // Without this, the compiler saturates all cores and OOM-kills on Docker Desktop.
    cmd.env("TUNGSTEN_CODEGEN_JOBS", parallelism.to_string());

    let output = run_with_rusage(&mut cmd).map_err(|e| format!("failed to run tungsten: {e}"))?;
    rss.log_stage("emit-ir", output.max_rss_kb);

    // Capture stderr to log file
    if !output.stderr.is_empty() {
        let _ = std::fs::write(&log_path, &output.stderr);
    }

    // Stream stdout
    if !output.stdout.is_empty() {
        io::Write::write_all(&mut io::stdout(), &output.stdout)
            .map_err(|e| format!("write stdout: {e}"))?;
    }

    if !output.status.success() {
        return Err(format!(
            "IR generation failed (exit {}). See {}",
            output.status.code().unwrap_or(-1),
            log_path.display()
        ));
    }

    Ok(())
}

fn link(arts: &Artifacts, rss: &RssLogger) -> Result<(), String> {
    let obj_files =
        find_files(&arts.ir_dir, "o").map_err(|e| format!("failed to find .o files: {e}"))?;

    if obj_files.is_empty() {
        return Err("no .o files found for linking".to_string());
    }

    // A host-clobbered (BSD-ar) libtungsten_core.a fails here with a cryptic
    // GNU ld error — diagnose it before spawning the linker (ADR 11.7.26a).
    artifacts::check_static_lib_format(&arts.static_lib)?;

    let mut cmd = Command::new("cc");
    cmd.arg("-o").arg(&arts.output_binary);
    for f in &obj_files {
        cmd.arg(f);
    }
    // Static archive linking (ADR 18.5.26e).
    // Linux-only deps: tungsten-dev runs exclusively inside the devcontainer.
    cmd.arg(&arts.static_lib);
    // Set stack size to 128 MB — elaboration of large programs needs deep recursion.
    // Matches the flag used in bootstrap/src/compile/linking/mod.rs.
    cmd.args(["-Wl,-z,stack-size=134217728"]);
    cmd.args([
        "-lgcc_s",
        "-lutil",
        "-lrt",
        "-lpthread",
        "-lm",
        "-ldl",
        "-lc",
    ]);

    let output = run_with_rusage(&mut cmd).map_err(|e| format!("failed to run linker: {e}"))?;
    rss.log_stage("link", output.max_rss_kb);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("linking failed: {stderr}"));
    }

    Ok(())
}

fn cleanup_ir(arts: &Artifacts) -> Result<(), String> {
    std::fs::remove_dir_all(&arts.ir_dir)
        .map_err(|e| format!("failed to clean {}: {e}", arts.ir_dir.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn mode_names_cover_all_modes() {
        let mk = |fast, opt, direct| SelfCompileArgs {
            fast,
            opt,
            direct,
            x86: false,
            parallelism: None,
            alloc_profile: false,
        };
        assert_eq!(mode_name(&mk(true, false, false)), "self-compile-fast");
        assert_eq!(mode_name(&mk(false, true, false)), "self-compile-opt");
        assert_eq!(mode_name(&mk(false, false, true)), "self-compile-direct");
        // default (no flags) is fast
        assert_eq!(mode_name(&mk(false, false, false)), "self-compile-fast");
    }

    /// T4a plumbing: the smoke gate fails the build when the product EXISTS,
    /// is EXECUTABLE, but exits non-zero — surfacing command + exit status.
    /// (The §6.6 regression was a binary that ran and crashed, not a missing
    /// path.)
    #[test]
    fn smoke_gate_fails_on_crashing_product() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        fs::create_dir_all(root.join("examples")).unwrap();
        fs::write(root.join("examples/hello.tg"), "").unwrap();
        let arts = Artifacts::at_root(root.clone(), root.join("target"));

        fs::write(&arts.output_binary, "#!/bin/sh\necho segv >&2\nexit 9\n").unwrap();
        fs::set_permissions(&arts.output_binary, fs::Permissions::from_mode(0o755)).unwrap();

        let err = smoke::smoke_check(&arts.output_binary, &arts.smoke_input).unwrap_err();
        assert!(err.contains("exited with 9"), "{err}");
        assert!(err.contains("check"), "{err}");
        assert!(err.contains("hello.tg"), "{err}");
    }

    /// T4b plumbing: `finish` format-checks the product BEFORE executing it —
    /// a clobbered (wrong-platform) stage binary is a hard error.
    #[test]
    fn finish_rejects_wrong_format_product_before_exec() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        fs::create_dir_all(root.join("examples")).unwrap();
        fs::write(root.join("examples/hello.tg"), "").unwrap();
        let arts = Artifacts::at_root(root.clone(), root.join("target"));

        let wrong_magic: &[u8] = if cfg!(target_os = "macos") {
            &[0x7f, b'E', b'L', b'F'] // ELF on macOS
        } else {
            &[0xcf, 0xfa, 0xed, 0xfe] // Mach-O on Linux
        };
        fs::write(&arts.output_binary, wrong_magic).unwrap();

        let err = finish(&arts, "test").unwrap_err();
        assert!(err.contains("SHARED"), "clobber guidance surfaced: {err}");
    }
}
