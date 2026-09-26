//! Tests for `artifacts.rs` (format guards + `$CARGO_TARGET_DIR` resolution).
//! Kept in a `#[path]` sibling so `artifacts.rs` stays under the 400-LOC gate.

use super::*;
use std::fs;

const ELF: &[u8] = &[0x7f, b'E', b'L', b'F', 2, 1, 1, 0];
const MACHO64: &[u8] = &[0xcf, 0xfa, 0xed, 0xfe, 7, 0, 0, 1];

#[test]
fn classify_known_magics() {
    assert_eq!(classify_magic(ELF), ExecFormat::Elf);
    assert_eq!(classify_magic(MACHO64), ExecFormat::MachO);
    assert_eq!(classify_magic(&[0xfe, 0xed, 0xfa, 0xce]), ExecFormat::MachO);
    assert_eq!(classify_magic(&[0xca, 0xfe, 0xba, 0xbe]), ExecFormat::MachO);
    assert_eq!(classify_magic(b"#!/b"), ExecFormat::Unknown);
    assert_eq!(classify_magic(b""), ExecFormat::Unknown);
}

/// ADR 11.7.26a: helper to build a 24-byte archive head (magic + name field).
fn archive_head(first_member_name: &[u8]) -> Vec<u8> {
    let mut head = AR_MAGIC.to_vec();
    let mut name = first_member_name.to_vec();
    name.resize(16, b' ');
    head.extend_from_slice(&name);
    head
}

#[test]
fn gnu_indexed_archive_passes() {
    let p = Path::new("libtungsten_core.a");
    assert!(check_archive_head_for(p, &archive_head(b"/")).is_ok());
    assert!(check_archive_head_for(p, &archive_head(b"/SYM64/")).is_ok());
}

#[test]
fn bsd_archive_is_diagnosed_as_host_clobber() {
    let p = Path::new("libtungsten_core.a");
    for name in [&b"__.SYMDEF"[..], &b"__.SYMDEF SORTED"[..], &b"#1/20"[..]] {
        let err = check_archive_head_for(p, &archive_head(name)).unwrap_err();
        assert!(err.contains("BSD-ar"), "diagnosis names BSD-ar: {err}");
        assert!(
            err.contains("cargo build --release -p tungsten_core"),
            "fix names the in-container rebuild: {err}"
        );
    }
}

#[test]
fn non_archive_and_indexless_archives_are_rejected() {
    let p = Path::new("libtungsten_core.a");
    let err = check_archive_head_for(p, ELF).unwrap_err();
    assert!(err.contains("not a static archive"), "{err}");
    // Long-name table first (no symbol index) must not ride the `/` prefix.
    let err = check_archive_head_for(p, &archive_head(b"//")).unwrap_err();
    assert!(err.contains("no GNU symbol index"), "{err}");
    // Plain object-member-first archive (never ranlib'd).
    let err = check_archive_head_for(p, &archive_head(b"foo.o")).unwrap_err();
    assert!(err.contains("no GNU symbol index"), "{err}");
    // Truncated file shorter than the magic.
    let err = check_archive_head_for(p, b"!<arch>").unwrap_err();
    assert!(err.contains("not a static archive"), "{err}");
}

/// T4b: the guard rejects a wrong-format binary at the bootstrap-compiler
/// position with the "rebuild" guidance.
#[test]
fn bootstrap_artifact_wrong_format_is_hard_error() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    fs::create_dir_all(root.join("target/release")).unwrap();
    let arts = Artifacts::at_root(root.clone(), root.join("target"));
    fs::write(&arts.tungsten_bin, MACHO64).unwrap();

    let magic = read_magic(&arts.tungsten_bin).unwrap();
    let err = check_magic_for(&arts.tungsten_bin, &magic, ExecFormat::Elf).unwrap_err();
    assert!(
        err.contains("MachO"),
        "err should name actual format: {err}"
    );
    assert!(
        err.contains("Rebuild"),
        "err should tell the user to rebuild: {err}"
    );
    assert!(
        err.contains("target/release/tungsten"),
        "err should name the artifact: {err}"
    );
}

/// T4b: the guard also rejects a clobbered *generated stage binary*, not
/// only the bootstrap compiler.
#[test]
fn stage_artifact_wrong_format_is_hard_error() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let arts = Artifacts::at_root(root.clone(), root.join("target"));
    fs::write(&arts.output_binary, MACHO64).unwrap();

    let magic = read_magic(&arts.output_binary).unwrap();
    let err = check_magic_for(&arts.output_binary, &magic, ExecFormat::Elf).unwrap_err();
    assert!(
        err.contains("tungsten1"),
        "err should name the stage binary: {err}"
    );
}

#[test]
fn matching_format_passes() {
    let dir = tempfile::tempdir().unwrap();
    let bin = dir.path().join("ok-bin");
    fs::write(&bin, ELF).unwrap();
    let magic = read_magic(&bin).unwrap();
    assert!(check_magic_for(&bin, &magic, ExecFormat::Elf).is_ok());
}

/// Source/output/log artifacts stay under the workspace root; the two
/// ABI-sensitive build artifacts follow `target_dir`, which may live
/// *outside* the workspace when the container isolates its build dir
/// (ADR 24.7.26b).
#[test]
fn artifacts_split_between_root_and_target_dir() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    // An external, isolated build dir (the /build/target volume in prod).
    let target_dir = PathBuf::from("/build/target");
    let arts = Artifacts::at_root(root.clone(), target_dir.clone());

    // The resolved build dir is recorded verbatim (surfaced by the
    // self-compile `[target] build dir:` diagnostic, ADR 24.7.26b).
    assert_eq!(arts.target_dir, target_dir);

    // Source + output + log artifacts remain workspace-rooted.
    for p in [
        &arts.output_binary,
        &arts.smoke_input,
        &arts.entry,
        &arts.fallback_log_dir,
    ] {
        assert!(p.is_absolute(), "{} not absolute", p.display());
        assert!(p.starts_with(&root), "{} not under root", p.display());
    }

    // Build artifacts follow the (external) target dir, NOT the root.
    for p in [&arts.tungsten_bin, &arts.static_lib] {
        assert!(p.is_absolute(), "{} not absolute", p.display());
        assert!(
            p.starts_with(&target_dir),
            "{} not under target_dir",
            p.display()
        );
        assert!(
            !p.starts_with(&root),
            "{} unexpectedly under root — isolation defeated",
            p.display()
        );
    }
    assert!(arts.ir_dir.is_absolute());
    assert!(arts.log_dir.is_absolute());
}

/// The default (host / unset `$CARGO_TARGET_DIR`) path keeps build
/// artifacts under `<root>/target`, so host behaviour is unchanged.
#[test]
fn default_target_dir_keeps_build_artifacts_under_root() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let target_dir = target_dir_from_env(&root, None);
    let arts = Artifacts::at_root(root.clone(), target_dir);
    assert_eq!(arts.target_dir, root.join("target"));
    for p in [&arts.tungsten_bin, &arts.static_lib] {
        assert!(p.starts_with(&root), "{} not under root", p.display());
    }
}

/// `target_dir_from_env`: a non-empty `$CARGO_TARGET_DIR` wins; an unset
/// OR empty value falls back to `<root>/target`.
#[test]
fn target_dir_from_env_honors_and_falls_back() {
    let root = Path::new("/ws");
    assert_eq!(
        target_dir_from_env(root, Some(OsString::from("/build/target"))),
        PathBuf::from("/build/target"),
        "non-empty CARGO_TARGET_DIR must win",
    );
    assert_eq!(
        target_dir_from_env(root, None),
        PathBuf::from("/ws/target"),
        "unset must fall back to <root>/target",
    );
    assert_eq!(
        target_dir_from_env(root, Some(OsString::new())),
        PathBuf::from("/ws/target"),
        "empty must fall back to <root>/target",
    );
}

#[test]
fn workspace_root_found_from_nested_dir() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    fs::create_dir_all(root.join("src/compiler")).unwrap();
    fs::write(root.join("src/compiler/main.tg"), "").unwrap();
    let nested = root.join("tools/deep/nested");
    fs::create_dir_all(&nested).unwrap();

    let found = find_workspace_root(&nested).unwrap();
    // Compare canonicalized (tempdirs may traverse symlinks on macOS).
    assert_eq!(found.canonicalize().unwrap(), root.canonicalize().unwrap());
}

#[test]
fn workspace_root_missing_is_error() {
    let dir = tempfile::tempdir().unwrap();
    assert!(find_workspace_root(dir.path()).is_err());
}

// ---------------------------------------------------------------------------
// Canonical bootstrap build + step plan (ADR 24.9.26a)
// ---------------------------------------------------------------------------

fn stage1_compiler(steps: &[SelfCompileStep]) -> &Path {
    steps
        .iter()
        .find_map(|s| match s {
            SelfCompileStep::EmitIr { compiler } | SelfCompileStep::DirectCompile { compiler } => {
                Some(compiler.as_path())
            }
            _ => None,
        })
        .expect("a plan has a step that runs the bootstrap")
}

/// ADR 24.9.26a AC1: the canonical build is first, and Stage 1 runs exactly
/// the artifact it built — for the default and a custom (`$CARGO_TARGET_DIR`)
/// target dir, in both the llc and the direct modes.
#[test]
fn canonical_build_precedes_stage1_and_stage1_runs_its_artifact() {
    let root = Path::new("/ws");
    for target_dir in [
        target_dir_from_env(root, None),
        target_dir_from_env(root, Some(OsString::from("/build/target"))),
    ] {
        let arts = Artifacts::at_root(root.to_path_buf(), target_dir.clone());
        for direct in [false, true] {
            let steps = self_compile_steps(&target_dir, direct);
            let SelfCompileStep::BuildBootstrap(build) = &steps[0] else {
                panic!("first step must be the canonical build: {steps:?}");
            };
            assert_eq!(build.artifact, target_dir.join("release/tungsten"));
            assert_eq!(stage1_compiler(&steps), build.artifact);
            assert_eq!(arts.tungsten_bin, build.artifact, "Artifacts agrees");
            let argv: Vec<_> = build.argv.iter().map(|a| a.to_string_lossy()).collect();
            assert_eq!(
                argv[..argv.len() - 1],
                [
                    "cargo",
                    "build",
                    "--release",
                    "-p",
                    "tungsten_bootstrap",
                    "-p",
                    "tungsten_core",
                    "--target-dir"
                ]
            );
            assert_eq!(
                *argv.last().unwrap(),
                target_dir.to_string_lossy(),
                "the build writes where the artifact is read"
            );
        }
    }
}

#[test]
fn step_plans_have_the_expected_shape() {
    let target_dir = Path::new("/build/target");
    let build = CanonicalBootstrapBuild::for_target_dir(target_dir);
    let compiler = build.artifact.clone();
    assert_eq!(
        self_compile_steps(target_dir, false),
        vec![
            SelfCompileStep::BuildBootstrap(build.clone()),
            SelfCompileStep::EmitIr {
                compiler: compiler.clone()
            },
            SelfCompileStep::CompileIr,
            SelfCompileStep::Link,
        ]
    );
    assert_eq!(
        self_compile_steps(target_dir, true),
        vec![
            SelfCompileStep::BuildBootstrap(build),
            SelfCompileStep::DirectCompile { compiler },
        ]
    );
}

/// ADR 24.9.26a AC2: a non-zero canonical build ends the self-compile before
/// Stage 1, and the message carries the argv and the resolved artifact path.
#[test]
fn failed_canonical_build_stops_before_stage1() {
    let target_dir = Path::new("/build/target");
    let steps = self_compile_steps(target_dir, false);
    let mut ran = Vec::new();
    let err = run_steps(&steps, |step| {
        ran.push(step.clone());
        match step {
            SelfCompileStep::BuildBootstrap(build) => build.check_outcome(Some(101)),
            _ => Ok(()),
        }
    })
    .unwrap_err();
    assert_eq!(ran, steps[..1], "nothing after the build ran");
    assert!(err.contains("exit 101"), "{err}");
    assert!(
        err.contains(
            "cargo build --release -p tungsten_bootstrap -p tungsten_core --target-dir /build/target"
        ),
        "argv named: {err}"
    );
    assert!(
        err.contains("/build/target/release/tungsten"),
        "artifact named: {err}"
    );
}

#[test]
fn successful_build_lets_every_step_run() {
    let steps = self_compile_steps(Path::new("/t"), false);
    let mut ran = 0;
    run_steps(&steps, |step| {
        ran += 1;
        match step {
            SelfCompileStep::BuildBootstrap(build) => build.check_outcome(Some(0)),
            _ => Ok(()),
        }
    })
    .unwrap();
    assert_eq!(ran, steps.len());
}

#[test]
fn a_signalled_build_is_a_failure_too() {
    let build = CanonicalBootstrapBuild::for_target_dir(Path::new("/t"));
    let err = build.check_outcome(None).unwrap_err();
    assert!(err.contains("a signal"), "{err}");
    assert!(build.check_outcome(Some(1)).unwrap_err().contains("exit 1"));
}
