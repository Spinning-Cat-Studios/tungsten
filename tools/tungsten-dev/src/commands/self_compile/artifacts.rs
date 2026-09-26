//! Central artifact resolver + executable-format guard (ADR 2.7.26b T4).
//!
//! All self-compile stages resolve their binaries and inputs through
//! [`Artifacts`], which produces absolute, cwd-independent paths anchored at
//! the workspace root. Every compiler artifact is format-checked immediately
//! before it is executed: the bind-mounted `target/` is shared host↔container,
//! so a host (macOS) build can clobber a container ELF mid-session — the
//! symptom is `exec format error` on any stage binary, not just the bootstrap
//! compiler.

use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Absolute paths for every binary/input a self-compile touches.
pub struct Artifacts {
    /// Workspace root (directory containing `src/compiler/main.tg`).
    pub root: PathBuf,
    /// Cargo build directory: `$CARGO_TARGET_DIR` when set (the isolated
    /// container-local volume, ADR 24.7.26b), else `<root>/target`.
    pub target_dir: PathBuf,
    /// Bootstrap compiler: `<target_dir>/release/tungsten`.
    pub tungsten_bin: PathBuf,
    /// Self-compiled output: `<root>/tungsten1`.
    pub output_binary: PathBuf,
    /// Static runtime archive: `<target_dir>/release/libtungsten_core.a`.
    pub static_lib: PathBuf,
    /// Smoke-check input: `<root>/examples/hello.tg`.
    pub smoke_input: PathBuf,
    /// Entry file: `<root>/src/compiler/main.tg`.
    pub entry: PathBuf,
    /// Scratch IR directory (already absolute).
    pub ir_dir: PathBuf,
    /// Primary log dir (`/var/log/tungsten`, bind-mounted to `.devcontainer/logs`).
    pub log_dir: PathBuf,
    /// Workspace-local fallback when the primary is unwritable (host runs).
    pub fallback_log_dir: PathBuf,
}

impl Artifacts {
    /// Resolve from the current working directory by walking up to the
    /// workspace root, reading `$CARGO_TARGET_DIR` for the build directory.
    pub fn resolve() -> Result<Self, String> {
        let cwd = std::env::current_dir().map_err(|e| format!("cannot read cwd: {e}"))?;
        let root = find_workspace_root(&cwd)?;
        let target_dir = target_dir_from_env(&root, std::env::var_os("CARGO_TARGET_DIR"));
        Ok(Self::at_root(root, target_dir))
    }

    /// Build the artifact set for a known root + build directory (test seam).
    ///
    /// The two ABI-sensitive build artifacts (`tungsten_bin`, `static_lib`)
    /// live under `target_dir` — which may be *outside* the workspace when
    /// the container isolates its build dir (ADR 24.7.26b). Everything else
    /// (source inputs, the self-compiled `tungsten1`, logs) stays under `root`.
    pub fn at_root(root: PathBuf, target_dir: PathBuf) -> Self {
        Self {
            tungsten_bin: CanonicalBootstrapBuild::for_target_dir(&target_dir).artifact,
            output_binary: root.join("tungsten1"),
            static_lib: target_dir.join("release/libtungsten_core.a"),
            smoke_input: root.join("examples/hello.tg"),
            entry: root.join("src/compiler/main.tg"),
            ir_dir: PathBuf::from("/tmp/tungsten1_ll"),
            log_dir: PathBuf::from("/var/log/tungsten"),
            fallback_log_dir: root.join(".devcontainer/logs"),
            target_dir,
            root,
        }
    }
}

// ============================================================================
// Canonical bootstrap build (ADR 24.9.26a)
// ============================================================================

/// The one build that makes the bootstrap compiler fresh, and the artifact it
/// produces — both derived from the same target dir, so a gate cannot build
/// into one directory and run a binary from another.
///
/// Freshness is Cargo's answer, never an mtime scan: Cargo knows every input
/// (manifests, lockfile, features, dependencies), so running the build is a
/// no-op when nothing changed and a rebuild when anything did. There is
/// deliberately no way to skip it — an opt-out is the stale path with a name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalBootstrapBuild {
    /// Full argv, program first: `cargo build --release -p tungsten_bootstrap
    /// -p tungsten_core --target-dir <dir>`, each package with its default
    /// features. `tungsten_core` is here because Stage 3 links its static
    /// archive — a self-compile drives that artifact too.
    pub argv: Vec<OsString>,
    /// The bootstrap compiler the build produces: `<dir>/release/tungsten`.
    pub artifact: PathBuf,
}

impl CanonicalBootstrapBuild {
    pub fn for_target_dir(target_dir: &Path) -> Self {
        let argv = [
            "cargo",
            "build",
            "--release",
            "-p",
            "tungsten_bootstrap",
            "-p",
            "tungsten_core",
            "--target-dir",
        ]
        .iter()
        .map(OsString::from)
        .chain(std::iter::once(target_dir.as_os_str().to_owned()))
        .collect();
        Self {
            argv,
            artifact: target_dir.join("release/tungsten"),
        }
    }

    /// The argv as one printable line, for the failure message.
    pub fn command_line(&self) -> String {
        self.argv
            .iter()
            .map(|a| a.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Judge the build's exit: success, or a stop naming the command and the
    /// artifact that would have run — never a fall-back to the file on disk.
    /// `exit_code` is `None` when the build was killed by a signal.
    pub fn check_outcome(&self, exit_code: Option<i32>) -> Result<(), String> {
        if exit_code == Some(0) {
            return Ok(());
        }
        let exit = exit_code.map_or_else(|| "a signal".to_string(), |c| format!("exit {c}"));
        Err(format!(
            "canonical bootstrap build failed ({exit}): `{}` — refusing to run the \
             existing {} rather than a fresh one (ADR 24.9.26a)",
            self.command_line(),
            self.artifact.display(),
        ))
    }
}

/// One step of a self-compile, in execution order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelfCompileStep {
    /// Make the bootstrap (and runtime archive) fresh.
    BuildBootstrap(CanonicalBootstrapBuild),
    /// Stage 1: the bootstrap emits LLVM IR.
    EmitIr { compiler: PathBuf },
    /// Stage 2: `llc` compiles the IR.
    CompileIr,
    /// Stage 3: link against the runtime archive.
    Link,
    /// `--direct`: the bootstrap emits objects and links in-process.
    DirectCompile { compiler: PathBuf },
}

/// The ordered step list for a self-compile: the canonical build always
/// first, and every step that runs the bootstrap takes the build's artifact.
pub fn self_compile_steps(target_dir: &Path, direct: bool) -> Vec<SelfCompileStep> {
    let build = CanonicalBootstrapBuild::for_target_dir(target_dir);
    let compiler = build.artifact.clone();
    let mut steps = vec![SelfCompileStep::BuildBootstrap(build)];
    if direct {
        steps.push(SelfCompileStep::DirectCompile { compiler });
    } else {
        steps.push(SelfCompileStep::EmitIr { compiler });
        steps.push(SelfCompileStep::CompileIr);
        steps.push(SelfCompileStep::Link);
    }
    steps
}

/// Run `steps` in order through `run_step`, stopping at the first error —
/// so a failed canonical build ends the self-compile before Stage 1.
pub fn run_steps(
    steps: &[SelfCompileStep],
    mut run_step: impl FnMut(&SelfCompileStep) -> Result<(), String>,
) -> Result<(), String> {
    steps.iter().try_for_each(|step| run_step(step))
}

/// Resolve the Cargo build directory: `$CARGO_TARGET_DIR` when set to a
/// non-empty value (the isolated container-local volume, ADR 24.7.26b), else
/// `<root>/target` (host builds keep the bind-mounted workspace `target/`).
/// Pure over the env value so the fallback branch stays unit-testable.
fn target_dir_from_env(root: &Path, env: Option<OsString>) -> PathBuf {
    match env {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => root.join("target"),
    }
}

/// Walk up from `start` to the directory containing `src/compiler/main.tg`.
fn find_workspace_root(start: &Path) -> Result<PathBuf, String> {
    let mut dir = start.to_path_buf();
    loop {
        if dir.join("src/compiler/main.tg").is_file() {
            return Ok(dir);
        }
        if !dir.pop() {
            return Err(format!(
                "could not locate workspace root: no src/compiler/main.tg at or above {}",
                start.display()
            ));
        }
    }
}

/// Executable formats we can distinguish by magic bytes.
#[derive(Debug, PartialEq, Eq)]
pub enum ExecFormat {
    Elf,
    MachO,
    Unknown,
}

/// Classify an executable's leading magic bytes.
pub fn classify_magic(magic: &[u8]) -> ExecFormat {
    match magic {
        [0x7f, b'E', b'L', b'F', ..] => ExecFormat::Elf,
        // Mach-O thin (32/64, both endiannesses) and fat binaries.
        [0xfe, 0xed, 0xfa, 0xce | 0xcf, ..]
        | [0xce | 0xcf, 0xfa, 0xed, 0xfe, ..]
        | [0xca, 0xfe, 0xba, 0xbe | 0xbf, ..] => ExecFormat::MachO,
        _ => ExecFormat::Unknown,
    }
}

/// The format the current platform can execute.
fn native_format() -> ExecFormat {
    if cfg!(target_os = "macos") {
        ExecFormat::MachO
    } else {
        ExecFormat::Elf
    }
}

/// Hard error when `path` is not executable on this platform (ADR 2.7.26b
/// T4b). Called immediately before EVERY compiler-artifact exec — the
/// bootstrap `tungsten` and each generated stage binary.
pub fn check_executable_format(path: &Path) -> Result<(), String> {
    let magic =
        read_magic(path).map_err(|e| format!("cannot read executable {}: {e}", path.display()))?;
    check_magic_for(path, &magic, native_format())
}

/// Testable core: verify `magic` against an expected native format.
pub fn check_magic_for(path: &Path, magic: &[u8], expected: ExecFormat) -> Result<(), String> {
    let actual = classify_magic(magic);
    if actual == expected {
        return Ok(());
    }
    Err(format!(
        "{} is a {:?} binary but this platform executes {:?} — the bind-mounted \
         target/ is SHARED host↔container, so a build from the other environment \
         has clobbered it. Rebuild in the environment you are running in \
         (container: `cargo build --release`) and retry. (ADR 2.7.26b T4b)",
        path.display(),
        actual,
        expected,
    ))
}

fn read_magic(path: &Path) -> std::io::Result<Vec<u8>> {
    let mut f = std::fs::File::open(path)?;
    let mut buf = [0u8; 4];
    let n = f.read(&mut buf)?;
    Ok(buf[..n].to_vec())
}

// ============================================================================
// Static-archive format guard (ADR 11.7.26a)
// ============================================================================

/// `ar` global header magic shared by GNU and BSD archives.
const AR_MAGIC: &[u8] = b"!<arch>\n";

/// Hard error when `libtungsten_core.a` cannot be linked on this platform.
///
/// The shared `target/` means a host macOS `cargo build` leaves a BSD-ar
/// archive (first member `__.SYMDEF`) that GNU ld rejects with the cryptic
/// `archive has no index; run ranlib` — this guard turns that into the
/// clobber diagnosis + fix *before* the link runs. Linux-only enforcement:
/// `tungsten-dev` links exclusively inside the devcontainer.
pub fn check_static_lib_format(path: &Path) -> Result<(), String> {
    if cfg!(target_os = "macos") {
        return Ok(());
    }
    let mut f = std::fs::File::open(path)
        .map_err(|e| format!("cannot read static library {}: {e}", path.display()))?;
    let mut head = [0u8; 24]; // 8-byte magic + first 16 bytes of member header (the name field)
    let n = f.read(&mut head).map_err(|e| e.to_string())?;
    check_archive_head_for(path, &head[..n])
}

/// Testable core: verify an archive's magic + first-member name field is a
/// GNU-indexed archive (first member `/` or `/SYM64/`).
pub fn check_archive_head_for(path: &Path, head: &[u8]) -> Result<(), String> {
    let rebuild_fix = "Rebuild it in the container (`cargo build --release -p \
                       tungsten_core`) and retry. (ADR 11.7.26a)";
    if head.len() < AR_MAGIC.len() + 1 || &head[..AR_MAGIC.len()] != AR_MAGIC {
        return Err(format!(
            "{} is not a static archive (bad `!<arch>` magic) — the bind-mounted \
             target/ is SHARED host↔container and something has clobbered it. {rebuild_fix}",
            path.display(),
        ));
    }
    let name_field = &head[AR_MAGIC.len()..];
    // BSD ar (macOS host build): symbol index member is `__.SYMDEF[ SORTED]`,
    // spelled directly or via the `#1/<len>` extended-name form.
    if name_field.starts_with(b"__.SYMDEF") || name_field.starts_with(b"#1/") {
        return Err(format!(
            "{} is a BSD-ar (macOS-built) archive — GNU ld will fail with `archive \
             has no index; run ranlib`. The bind-mounted target/ is SHARED \
             host↔container and a host build has clobbered it (host-side quality \
             gates like `make coverage-diff-gate` do this). {rebuild_fix}",
            path.display(),
        ));
    }
    // GNU index member is `/` (or `/SYM64/` for huge archives), space-padded.
    // A leading `//` is the long-name table — an archive starting with it has
    // no symbol index and still fails to link.
    if name_field.starts_with(b"/SYM64/") || name_field.starts_with(b"/ ") {
        return Ok(());
    }
    Err(format!(
        "{} has no GNU symbol index as its first archive member — GNU ld will \
         fail with `archive has no index; run ranlib`. {rebuild_fix}",
        path.display(),
    ))
}

// Tests: artifacts_tests.rs
#[cfg(test)]
#[path = "artifacts_tests.rs"]
mod tests;
