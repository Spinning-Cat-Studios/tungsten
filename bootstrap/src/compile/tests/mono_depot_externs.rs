//! ADR 1.7.26f — the `__mono` depot declares the non-generic globals its
//! mono-instances call.
//!
//! End-to-end compile+run coverage: each test builds a small project where a
//! monomorphized generic body references a non-generic top-level function,
//! compiles it to a native binary, runs it, and asserts on the printed main
//! result. Before the fix every one of these failed with
//! `mono depot define failed … global '<name>' not found` (pre-rename:
//! "prelude mono define failed").

use crate::compile::{cmd_compile, CompileFlags};
use std::fs;
use std::process::ExitCode;
use tempfile::TempDir;

/// The linker resolves `libtungsten_core.a` next to the running executable
/// (`linking/mod.rs` `lib_dir`). Under `cargo test` that is
/// `target/<profile>/deps/`, where cargo only puts hashed artifacts — mirror
/// the unhashed staticlib in from `target/<profile>/` so link tests work
/// in-harness. Atomic rename so parallel tests can race safely.
fn ensure_runtime_staticlib_beside_test_exe() {
    // OnceLock<Result>: parallel test threads share the process id, so
    // unsynchronized copies would interleave into a corrupt archive.
    // Cross-process races are handled by the pid-unique staging name +
    // atomic rename. A Result (not a bare Once) so that when the mirror
    // fails, EVERY test reports the original error — a panicking Once
    // poisoned itself and turned one missing-staticlib cause into a wall of
    // opaque "Once instance has previously been poisoned" failures
    // (ADR 21.7.26e close-out).
    static STATICLIB_MIRROR: std::sync::OnceLock<Result<(), String>> = std::sync::OnceLock::new();
    if let Err(mirror_error) = STATICLIB_MIRROR.get_or_init(mirror_runtime_staticlib) {
        panic!("runtime staticlib mirror failed: {mirror_error}");
    }
}

fn mirror_runtime_staticlib() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let deps_dir = exe
        .parent()
        .ok_or_else(|| "test exe has no parent dir".to_string())?;
    let dest = deps_dir.join("libtungsten_core.a");
    let src = deps_dir
        .parent()
        .ok_or_else(|| "deps dir has no parent dir".to_string())?
        .join("libtungsten_core.a");
    if !src.exists() {
        return Err(format!(
            "libtungsten_core.a not found at {} — run `cargo build -p tungsten_core` first \
             (a fresh target/debug only gets the unhashed staticlib from a direct build)",
            src.display()
        ));
    }
    let src_mtime = fs::metadata(&src)
        .and_then(|m| m.modified())
        .map_err(|e| e.to_string())?;
    if let Ok(dest_meta) = fs::metadata(&dest) {
        if dest_meta.modified().map_err(|e| e.to_string())? >= src_mtime {
            return Ok(());
        }
    }
    let staging = deps_dir.join(format!("libtungsten_core.a.tmp-{}", std::process::id()));
    fs::copy(&src, &staging).map_err(|e| e.to_string())?;
    fs::rename(&staging, &dest).map_err(|e| e.to_string())?;
    Ok(())
}

/// Write `files` into a temp dir, compile `entry` to a binary, run it, and
/// return trimmed stdout (the printed `main` result).
/// Shared with `colliding_imports.rs` (ADR 12.7.26a).
pub(super) fn compile_and_run(files: &[(&str, &str)], entry: &str) -> String {
    ensure_runtime_staticlib_beside_test_exe();
    let dir = TempDir::new().unwrap();
    for (name, source) in files {
        let path = dir.path().join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, source).unwrap();
    }
    let binary = dir.path().join("prog");
    let flags = CompileFlags {
        max_errors: 20,
        codegen_jobs: 1,
        ..CompileFlags::default()
    };
    assert_eq!(
        cmd_compile(&dir.path().join(entry), Some(&binary), &flags),
        ExitCode::SUCCESS,
        "compile of '{entry}' failed"
    );
    let output = std::process::Command::new(&binary).output().unwrap();
    assert!(
        output.status.success(),
        "binary exited nonzero: {:?}",
        output
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// AC 1 (primary): a generic body calling a non-generic global compiles and
/// runs, across modules, with two instantiations (scalar + struct payload).
#[test]
fn generic_body_calling_nongeneric_global_compiles_and_runs() {
    let helpers = r#"
extern "C" fn tg_assert_eq_bool(left: Nat, right: Nat) -> Unit

pub fn assert(cond: Bool) -> Unit {
    tg_assert_eq_bool(if cond { 1 } else { 0 }, 1)
}

pub fn eq_via<T>(a: T, b: T) -> Unit {
    assert(true)
}
"#;
    let main = r#"
mod helpers;

use helpers::{eq_via};

fn main() -> Nat {
    eq_via(3, 3);
    eq_via((1, 2), (1, 2));
    0
}
"#;
    assert_eq!(
        compile_and_run(&[("helpers.tg", helpers), ("main.tg", main)], "main.tg"),
        "0"
    );
}

/// AC 2 (R6): a generic body calling an `extern "C"` function directly.
#[test]
fn generic_body_calling_extern_c_directly_compiles_and_runs() {
    let util = r#"
extern "C" fn tg_assert_eq_nat(left: Nat, right: Nat) -> Unit

pub fn checked_pair<T>(a: T, b: T) -> Nat {
    tg_assert_eq_nat(1, 1);
    2
}
"#;
    let main = r#"
mod util;

use util::{checked_pair};

fn main() -> Nat {
    checked_pair(3, 3)
}
"#;
    assert_eq!(
        compile_and_run(&[("util.tg", util), ("main.tg", main)], "main.tg"),
        "2"
    );
}

/// AC 3 (integration, 29.6.26f-shaped): a generic `assert_eq<T>` calling
/// `assert`/`is_equal` at a scalar `T` compiles and runs to the correct exit.
#[test]
fn generic_assert_eq_at_scalar_type_compiles_and_runs() {
    let asserts = r#"
extern "C" fn tg_assert_eq_nat(left: Nat, right: Nat) -> Unit

fn is_equal(left: Nat, right: Nat) -> Bool {
    left == right
}

pub fn assert(cond: Bool) -> Unit {
    tg_assert_eq_nat(if cond { 1 } else { 0 }, 1)
}

pub fn assert_eq<T>(left: T, right: T) -> Unit {
    assert(is_equal(1, 1))
}
"#;
    let main = r#"
mod asserts;

use asserts::{assert_eq};

fn main() -> Nat {
    assert_eq(3, 3);
    0
}
"#;
    assert_eq!(
        compile_and_run(&[("asserts.tg", asserts), ("main.tg", main)], "main.tg"),
        "0"
    );
}

/// AC 5 (R1): two modules define same-named `describe`; each module's mono
/// instance must call its OWN module's def — 11 + 22 proves neither instance
/// resolved to the clobber-last extern-map entry.
#[test]
fn colliding_named_global_resolves_to_own_module() {
    let module_a = r#"
pub fn describe() -> Nat {
    11
}

pub fn tag_of<T>(x: T) -> Nat {
    describe()
}
"#;
    let module_b = r#"
pub fn describe() -> Nat {
    22
}

pub fn label_of<T>(x: T) -> Nat {
    describe()
}
"#;
    let main = r#"
mod a;
mod b;

use a::{tag_of};
use b::{label_of};

fn main() -> Nat {
    tag_of(true) + label_of(7)
}
"#;
    assert_eq!(
        compile_and_run(
            &[("a.tg", module_a), ("b.tg", module_b), ("main.tg", main)],
            "main.tg"
        ),
        "33"
    );
}

/// AC 6: a transitively-referenced non-generic chain (instance → f → g) links —
/// the depot only needs the direct prototype; `f`'s own unit declares `g`.
#[test]
fn transitively_referenced_globals_link() {
    let util = r#"
fn base_seed() -> Nat {
    5
}

pub fn base_value() -> Nat {
    base_seed() + 1
}

pub fn chained<T>(x: T) -> Nat {
    base_value() + 1
}
"#;
    let main = r#"
mod util;

use util::{chained};

fn main() -> Nat {
    chained(true)
}
"#;
    assert_eq!(
        compile_and_run(&[("util.tg", util), ("main.tg", main)], "main.tg"),
        "7"
    );
}

/// AC 7 (R4): a source-`private` module-local helper called from a generic
/// body gets an externally-linkable symbol — a regression here is a link error.
#[test]
fn private_helper_called_from_generic_body_links() {
    let util = r#"
fn secret() -> Nat {
    40
}

pub fn via_secret<T>(x: T) -> Nat {
    secret()
}
"#;
    let main = r#"
mod util;

use util::{via_secret};

fn main() -> Nat {
    via_secret((1, 2))
}
"#;
    assert_eq!(
        compile_and_run(&[("util.tg", util), ("main.tg", main)], "main.tg"),
        "40"
    );
}

/// The single-file path routes through the same per-function-unit + depot
/// pipeline (P0 finding: it was equally affected, contra the ADR's §1.3 guess).
#[test]
fn single_file_generic_body_calling_global_compiles_and_runs() {
    let single = r#"
extern "C" fn tg_assert_eq_bool(left: Nat, right: Nat) -> Unit

pub fn assert(cond: Bool) -> Unit {
    tg_assert_eq_bool(if cond { 1 } else { 0 }, 1)
}

pub fn eq_via<T>(a: T, b: T) -> Unit {
    assert(true)
}

fn main() -> Nat {
    eq_via(3, 3);
    eq_via((1, 2), (1, 2));
    0
}
"#;
    assert_eq!(compile_and_run(&[("single.tg", single)], "single.tg"), "0");
}

/// ADR 21.7.26e: a non-generic helper reached only TRANSITIVELY through a
/// private generic callee (root → mid → plain helper) still gets an external
/// prototype in the depot. Private generic callees receive no owned mono
/// instances — the owned root's compile defines their specializations
/// inline — so the depot's referenced-globals collection must walk generic
/// callee bodies, not just the root body. Pre-fix this failed the depot
/// define with "top-level function 'pick_larger' referenced but not
/// declared in unit '__mono'" (found by the strmap_insert →
/// strmap_rebalance → strmap_node → taller_of chain in the self-compile).
#[test]
fn transitive_private_helper_of_generic_callee_gets_depot_prototype() {
    let single = r#"
fn pick_larger(a: Nat, b: Nat) -> Nat {
    if a > b { a } else { b }
}

fn generic_mid<V>(witness: V, a: Nat, b: Nat) -> Nat {
    pick_larger(a, b)
}

pub fn generic_root<V>(witness: V, a: Nat, b: Nat) -> Nat {
    generic_mid(witness, a, b)
}

fn main() -> Nat {
    generic_root("witness", 3, 4)
}
"#;
    assert_eq!(compile_and_run(&[("single.tg", single)], "single.tg"), "4");
}
