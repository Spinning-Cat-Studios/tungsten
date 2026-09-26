//! End-to-end tests for `tungsten diff exec` (ADR 3.7.26d AC4).
//!
//! Spawns the real CLI binary. The parity test compiles and runs the
//! 3.7.26a regression fixture on both sides — the exact comparison that
//! caught defect 2, now automated. Requires the codegen feature (native
//! compile path), so these run in CI's LLVM `cargo test` matrix.
#![cfg(feature = "codegen")]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

fn tungsten() -> Command {
    Command::new(env!("CARGO_BIN_EXE_tungsten"))
}

/// The remedy every staleness report names (ADR 18.9.26b). A direct build of
/// the crate is the only thing that uplifts the unhashed archive.
const STALE_STATICLIB_REMEDY: &str = "cargo build -p tungsten_core";

/// The right-hand paths of the dep-info rule whose target is the crate's
/// staticlib — `…/deps/libtungsten_core-<hash>.a` — or nothing (ADR 18.9.26b).
///
/// A `tungsten_core-<hash>.d` cargo wrote for the crate's own `#[cfg(test)]`
/// harness has only an unprefixed target (`…/deps/tungsten_core-<hash>`) and
/// lists every test-only file, so it yields nothing here; after a test-only
/// edit it is the NEWEST dep-info, which is why the rule's target chooses and
/// not the file's age. Plain Makefile syntax: one `target: sources` line per
/// rule, then an empty `source:` rule per source, which has no right-hand side
/// and is never the archive.
fn dep_info_sources(dep_info: &str) -> Vec<&str> {
    for rule in dep_info.lines() {
        let Some((target, sources)) = rule.split_once(':') else {
            continue;
        };
        let target_file = target.rsplit('/').next().unwrap_or(target);
        if target_file.starts_with("libtungsten_core-") && target_file.ends_with(".a") {
            return sources.split_whitespace().collect();
        }
    }
    Vec::new()
}

/// Stale iff a source rustc read for the archive is newer than the archive.
/// Equal mtimes are fresh: a no-op rebuild re-clones the uplift without moving
/// its mtime, so `>=` would report a staleness the remedy cannot clear.
fn staticlib_is_stale(archive_mtime: SystemTime, newest_source_mtime: SystemTime) -> bool {
    newest_source_mtime > archive_mtime
}

fn mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// The newest `deps/tungsten_core-<hash>.d` that carries a staticlib rule, as
/// the sources that rule lists. `None` when no dep-info in `deps` does.
fn newest_staticlib_dep_info_sources(deps: &Path) -> Option<Vec<String>> {
    let mut newest: Option<(SystemTime, Vec<String>)> = None;
    for entry in std::fs::read_dir(deps).ok()?.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !(name.starts_with("tungsten_core-") && name.ends_with(".d")) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        let sources = dep_info_sources(&text);
        if sources.is_empty() {
            continue;
        }
        let Some(written) = mtime(&entry.path()) else {
            continue;
        };
        if newest.as_ref().is_none_or(|(t, _)| written > *t) {
            newest = Some((written, sources.iter().map(|s| s.to_string()).collect()));
        }
    }
    newest.map(|(_, sources)| sources)
}

/// ADR 18.9.26b: the spawned `tungsten` links the UNHASHED
/// `target/<profile>/libtungsten_core.a` beside itself, which `cargo test`
/// never refreshes — only a direct build of the crate uplifts it. Left stale,
/// a new `tg_*` symbol fails these tests at the linker with `Undefined
/// symbols`, naming nothing that would fix it. So before anything is spawned:
/// the archive must exist, and no source rustc read for it (per its dep-info,
/// the same input cargo's fingerprint is derived from) may be newer than it.
/// Both halves of the decision are pure and tested below; this is the effect.
fn require_fresh_runtime_staticlib() {
    let profile_dir = Path::new(env!("CARGO_BIN_EXE_tungsten"))
        .parent()
        .expect("CARGO_BIN_EXE_tungsten has a parent directory");
    let archive = profile_dir.join("libtungsten_core.a");
    let Some(archive_mtime) = mtime(&archive) else {
        panic!(
            "runtime staticlib {} is missing; run `{STALE_STATICLIB_REMEDY}` \
             (the spawned tungsten links the unhashed archive, which only a \
             direct build of the crate writes)",
            archive.display()
        );
    };
    // No dep-info with a staticlib rule means nothing to measure against —
    // the archive exists, so let the link speak for itself.
    let Some(sources) = newest_staticlib_dep_info_sources(&profile_dir.join("deps")) else {
        return;
    };
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    // A listed path that no longer exists is skipped: the source moved, and
    // the remedy will rewrite the dep-info anyway.
    let newest_source = sources
        .iter()
        .filter_map(|source| mtime(&repo_root.join(source)))
        .max();
    if let Some(newest_source) = newest_source {
        assert!(
            !staticlib_is_stale(archive_mtime, newest_source),
            "runtime staticlib {} is older than a tungsten_core source it was built from; \
             run `{STALE_STATICLIB_REMEDY}` (cargo test refreshes only the hashed \
             deps/ archive, not the unhashed one the spawned tungsten links)",
            archive.display()
        );
    }
}

fn fixture() -> String {
    repo_fixture("dead_arm_letelse_run.tg")
}

fn repo_fixture(name: &str) -> String {
    format!("{}/../tests/{name}", env!("CARGO_MANIFEST_DIR"))
}

/// Keep each standalone fixture's discovery and cache under its own directory.
/// Otherwise concurrent parity tests each pre-parse every `.tg` file in
/// `tests/` and write to the same cache, adding contention against the
/// evaluator's 60-second CI timeout.
fn isolated_fixture(name: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::TempDir::new().expect("failed to create fixture directory");
    let path = dir.path().join(name);
    std::fs::copy(repo_fixture(name), &path).expect("failed to copy fixture");
    (dir, path)
}

/// ADR 28.7.26a AC 4: the §2.2 capture sink is opt-in, so an *unmodified*
/// native run must still write to the process streams byte-for-byte.
///
/// This needs a fixture that actually prints. Nothing in `examples/` does —
/// `hello.tg` returns a String rather than printing one — so before
/// `console_println_run.tg` existed, the sink could have broken every console
/// write and `diff exec` would still have reported parity. It doubles as the
/// evaluator-side check: the console externs go silently `Stuck` without the
/// arms in `eval/env/handlers/extern_console.rs`, which shows up here as the
/// evaluator printing nothing while native prints four lines.
#[test]
fn parity_on_console_println_fixture() {
    require_fresh_runtime_staticlib();
    let (_dir, fixture) = isolated_fixture("console_println_run.tg");
    let out = tungsten()
        .args(["diff", "exec"])
        .arg(&fixture)
        .output()
        .expect("failed to spawn tungsten");
    assert_eq!(
        out.status.code(),
        Some(0),
        "expected console-output parity (exit 0)\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

/// ADR 14.9.26a AC 4: the `StringBuilder` runtime, its evaluator arms and the
/// codegen's marshalling of a by-value `String` argument and return must agree
/// byte-for-byte.
///
/// This is the load-bearing check for a representation change: every
/// single-path test passes on a builder that is consistently *wrong*, and
/// only running both paths on one program can see the three-way
/// disagreement. It already caught one — the evaluator's call-by-need memo
/// aliasing every `string_builder_new` in a body to one handle (fixed in
/// `EvalEnv::lookup`), which showed up here as the evaluator aborting on a
/// consumed handle while native printed five lines.
#[test]
fn parity_on_string_builder_fixture() {
    require_fresh_runtime_staticlib();
    let (_dir, fixture) = isolated_fixture("string_builder_run.tg");
    let out = tungsten()
        .args(["diff", "exec"])
        .arg(&fixture)
        .output()
        .expect("failed to spawn tungsten");
    assert_eq!(
        out.status.code(),
        Some(0),
        "expected StringBuilder parity (exit 0)\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

/// ADR 14.9.26b AC 4: with `TUNGSTEN_ARENA` unset the runtime is in mode
/// `off`, and a program that reaches every surface-reachable allocation
/// class — recursive ADT nodes, escaping closures, owned-left string
/// concatenation and a regrowing builder — prints what the evaluator prints.
#[test]
fn parity_on_arena_mode_fixture_in_mode_off() {
    require_fresh_runtime_staticlib();
    let (_dir, fixture) = isolated_fixture("arena_mode_run.tg");
    let out = tungsten()
        .args(["diff", "exec"])
        .arg(&fixture)
        .env_remove("TUNGSTEN_ARENA")
        .output()
        .expect("failed to spawn tungsten");
    assert_eq!(
        out.status.code(),
        Some(0),
        "expected parity in mode off (exit 0)\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

/// ADR 14.9.26b AC 5: the same program under `TUNGSTEN_ARENA=bump:1`, so the
/// native side bump-allocates from 1 MiB chunks and the 3 MB builder capacity
/// forces an oversized chunk and a rollover. The evaluator has no arena, so a
/// divergence here is the arena's — misalignment, a `grow_last` that copied
/// the wrong extent, or a rollover that lost the cursor. The variable reaches
/// the native child through `diff exec`'s inherited environment; the
/// bootstrap itself never calls `__tungsten_arena_init` and stays `off`.
#[test]
fn parity_on_arena_mode_fixture_in_mode_bump() {
    require_fresh_runtime_staticlib();
    let (_dir, fixture) = isolated_fixture("arena_mode_run.tg");
    let out = tungsten()
        .args(["diff", "exec"])
        .arg(&fixture)
        .env("TUNGSTEN_ARENA", "bump:1")
        .output()
        .expect("failed to spawn tungsten");
    assert_eq!(
        out.status.code(),
        Some(0),
        "expected parity in mode bump (exit 0)\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Real parity on the 3.7.26a fixture: evaluator and native binary agree →
/// exit 0. On a compiler that reintroduces the sret-discard miscompile this
/// exits 1 with both outputs in the report.
#[test]
fn parity_on_dead_arm_fixture() {
    require_fresh_runtime_staticlib();
    let (_dir, fixture) = isolated_fixture("dead_arm_letelse_run.tg");
    let out = tungsten()
        .args(["diff", "exec"])
        .arg(&fixture)
        .output()
        .expect("failed to spawn tungsten");
    assert_eq!(
        out.status.code(),
        Some(0),
        "expected parity (exit 0)\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

/// The hidden env-var override hook is wired through the CLI: forcing both
/// sides to synthetic commands classifies without compiling anything.
#[test]
fn env_override_hook_reaches_dispatch() {
    let out = tungsten()
        .args(["diff", "exec", &fixture()])
        .env("TUNGSTEN_DIFF_EXEC_NATIVE_OVERRIDE", "echo 99")
        .env("TUNGSTEN_DIFF_EXEC_EVAL_OVERRIDE", "echo 99")
        .output()
        .expect("failed to spawn tungsten");
    assert_eq!(out.status.code(), Some(0));

    let out = tungsten()
        .args(["diff", "exec", &fixture()])
        .env("TUNGSTEN_DIFF_EXEC_NATIVE_OVERRIDE", "echo garbage")
        .env("TUNGSTEN_DIFF_EXEC_EVAL_OVERRIDE", "echo 151")
        .output()
        .expect("failed to spawn tungsten");
    assert_eq!(out.status.code(), Some(1), "divergence must exit 1");
}

/// A file that fails elaboration is a compile error: neither side ran →
/// exit 3.
#[test]
fn compile_error_exits_3() {
    let dir = tempfile::TempDir::new().unwrap();
    let bad = dir.path().join("bad.tg");
    std::fs::write(&bad, "fn main() -> Nat { \"not a nat\" }").unwrap();
    let out = tungsten()
        .args(["diff", "exec", bad.to_str().unwrap()])
        .output()
        .expect("failed to spawn tungsten");
    assert_eq!(
        out.status.code(),
        Some(3),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

// ADR 18.9.26b AC 1: the staleness decision is two pure functions, asserted
// here with no filesystem — the shapes below are the ones observed in
// `target/debug/deps/` on the host on 18 Sep.

/// The dep-info of a LIB variant: an `.rlib` rule, the `.a` rule, then one
/// empty rule per source.
const LIB_DEP_INFO: &str = "/w/target/debug/deps/tungsten_core-3357.d: tungsten_core/src/lib.rs tungsten_core/src/ffi.rs\n\
/w/target/debug/deps/libtungsten_core-3357.rlib: tungsten_core/src/lib.rs tungsten_core/src/ffi.rs\n\
/w/target/debug/deps/libtungsten_core-3357.a: tungsten_core/src/lib.rs tungsten_core/src/ffi.rs\n\
tungsten_core/src/lib.rs:\n\
tungsten_core/src/ffi.rs:\n";

/// The dep-info of the crate's own `#[cfg(test)]` harness: its only target is
/// unprefixed, and it lists a test-only file the lib never read.
const HARNESS_DEP_INFO: &str = "/w/target/debug/deps/tungsten_core-0e0f.d: tungsten_core/src/lib.rs tungsten_core/src/terms/int_tests.rs\n\
/w/target/debug/deps/tungsten_core-0e0f: tungsten_core/src/lib.rs tungsten_core/src/terms/int_tests.rs\n\
tungsten_core/src/lib.rs:\n\
tungsten_core/src/terms/int_tests.rs:\n";

/// ADR 18.9.26b AC 1: the archive rule's right-hand side, and only that — not
/// the `.rlib` rule's (identical today, but the `.a` is what is linked) and
/// not the empty per-source rules.
#[test]
fn dep_info_sources_returns_the_staticlib_rules_right_hand_side() {
    assert_eq!(
        dep_info_sources(LIB_DEP_INFO),
        vec!["tungsten_core/src/lib.rs", "tungsten_core/src/ffi.rs"]
    );
}

/// ADR 18.9.26b AC 1: a harness dep-info has no `lib`-prefixed target, so it
/// contributes nothing — its test-only sources must never make the archive
/// read stale.
#[test]
fn dep_info_sources_is_empty_for_the_unit_test_harness() {
    assert!(dep_info_sources(HARNESS_DEP_INFO).is_empty());
}

/// ADR 18.9.26b AC 1: text with no rule at all is nothing, not a panic.
#[test]
fn dep_info_sources_is_empty_without_a_rule() {
    assert!(dep_info_sources("").is_empty());
    assert!(dep_info_sources("no colon on this line\n").is_empty());
}

/// ADR 18.9.26b AC 1: a `.a` target must be the crate's staticlib by name; a
/// dependent's archive in the same directory is not the reference point.
#[test]
fn dep_info_sources_ignores_another_crates_staticlib_rule() {
    let other = "/w/target/debug/deps/libother_crate-1234.a: other/src/lib.rs\n";
    assert!(dep_info_sources(other).is_empty());
}

/// ADR 18.9.26b AC 1: stale iff a source is newer; equal mtimes are fresh,
/// because a no-op rebuild does not move the uplift's mtime.
#[test]
fn staticlib_is_stale_only_when_a_source_is_newer() {
    let archive = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000);
    let older = archive - std::time::Duration::from_secs(1);
    let newer = archive + std::time::Duration::from_secs(1);
    assert!(staticlib_is_stale(archive, newer));
    assert!(!staticlib_is_stale(archive, older));
    assert!(!staticlib_is_stale(archive, archive));
}
