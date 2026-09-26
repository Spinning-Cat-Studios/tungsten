//! ADR 23.7.26c (D3) — depot instances must not self-recurse through their
//! own closure-returning wrapper.
//!
//! A monomorphized instance of a recursive generic function gets two symbols:
//! the closure-returning wrapper (allocates an environment per application
//! step) and the `$direct` uncurried entry. Pre-fix, the instance's saturated
//! self-call fell off the direct-call path (its head is a `TyApp`, not a bare
//! `Global`) and recursed through the wrapper — on the self-compiled self-check's
//! hottest loop that per-step allocation integrated to ~36 GB in the
//! never-freeing runtime. These tests emit the `__mono` depot IR for small
//! recursive-generic fixtures and assert the recursion targets `$direct`,
//! never the wrapper.

use super::mono_depot_externs::compile_and_run;
use crate::compile::{cmd_compile, CompileFlags};
use std::fs;
use std::process::ExitCode;
use tempfile::TempDir;

/// Compile `files` with `--emit-llvm` into a temp dir and return the contents
/// of the emitted `__mono.ll` depot unit.
fn emit_depot_ir(files: &[(&str, &str)], entry: &str) -> String {
    let dir = TempDir::new().unwrap();
    for (name, source) in files {
        let path = dir.path().join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, source).unwrap();
    }
    let ll_dir = dir.path().join("ll_out");
    let flags = CompileFlags {
        emit_llvm: true,
        max_errors: 20,
        codegen_jobs: 1,
        ..CompileFlags::default()
    };
    assert_eq!(
        cmd_compile(&dir.path().join(entry), Some(&ll_dir), &flags),
        ExitCode::SUCCESS,
        "emit-llvm compile of '{entry}' failed"
    );
    fs::read_to_string(ll_dir.join("__mono.ll")).expect("__mono.ll not emitted")
}

/// The depot-defined instance wrapper symbols: `define`d functions whose name
/// carries the `_I_` instance mangling, excluding `$direct`/`$direct_mt`
/// entries and compiler-generated inner lambdas.
fn depot_instance_wrapper_symbols(mono_ll: &str) -> Vec<String> {
    let mut symbols = Vec::new();
    for line in mono_ll.lines() {
        if !line.starts_with("define ") {
            continue;
        }
        let Some(at_pos) = line.find('@') else {
            continue;
        };
        let rest = &line[at_pos + 1..];
        let name = if let Some(stripped) = rest.strip_prefix('"') {
            let end = stripped.find('"').unwrap_or(stripped.len());
            &stripped[..end]
        } else {
            let end = rest.find('(').unwrap_or(rest.len());
            &rest[..end]
        };
        if name.contains("_I_") && !name.contains('$') {
            symbols.push(name.to_string());
        }
    }
    symbols
}

/// Assert no `call` instruction in the depot targets the given wrapper symbol
/// (taking the wrapper's *address* for a closure struct is fine — only calls
/// re-enter the per-step-allocating curried chain), and that its `$direct`
/// twin is called at least once (the recursion must have gone somewhere).
fn assert_no_wrapper_calls(mono_ll: &str, wrapper_symbol: &str) {
    let wrapper_call = format!("@{wrapper_symbol}(");
    let direct_call = format!("@\"{wrapper_symbol}$direct\"(");
    let mut saw_direct_call = false;
    for line in mono_ll.lines() {
        if !line.contains(" call ") {
            continue;
        }
        assert!(
            !line.contains(&wrapper_call),
            "depot instance '{wrapper_symbol}' is called through its \
             closure-returning wrapper (per-step env allocation):\n  {line}"
        );
        if line.contains(&direct_call) {
            saw_direct_call = true;
        }
    }
    assert!(
        saw_direct_call,
        "no call to '{wrapper_symbol}$direct' found in the depot — \
         the saturated-generic direct path did not engage"
    );
}

/// A 3-arg recursive generic (the `strmap_insert<CtorBucket>` shape): its
/// depot instance's self-recursion must target `$direct`, not the wrapper.
#[test]
fn recursive_generic_instance_self_calls_direct() {
    let single = r#"
// Partial (ADR 11.8.26b): a `Nat` countdown — this fixture is about the
// depot instance's call convention, not termination.
#[partial]
fn spin_generic<T>(depth: Nat, salt: Nat, payload: T) -> Nat {
    if depth == 0 {
        salt
    } else {
        spin_generic(depth - 1, salt + 1, payload)
    }
}

fn main() -> Nat {
    spin_generic(5, 0, true)
}
"#;
    let mono_ll = emit_depot_ir(&[("single.tg", single)], "single.tg");
    let wrappers = depot_instance_wrapper_symbols(&mono_ll);
    assert!(
        !wrappers.is_empty(),
        "expected at least one depot instance wrapper in __mono.ll"
    );
    for wrapper in &wrappers {
        assert_no_wrapper_calls(&mono_ll, wrapper);
    }
}

/// Cross-module shape (the self-compiled reality: generic defined in a library module,
/// instantiated from another): same guard on the emitted depot.
#[test]
fn cross_module_recursive_generic_instance_self_calls_direct() {
    let store = r#"
// Partial (ADR 11.8.26b): a `Nat` countdown — this fixture is about the
// depot instance's call convention, not termination.
#[partial]
pub fn count_up<V>(steps: Nat, acc: Nat, witness: V) -> Nat {
    if steps == 0 {
        acc
    } else {
        count_up(steps - 1, acc + 2, witness)
    }
}
"#;
    let main = r#"
mod store;

use store::{count_up};

fn main() -> Nat {
    count_up(4, 0, "witness")
}
"#;
    let mono_ll = emit_depot_ir(&[("store.tg", store), ("main.tg", main)], "main.tg");
    let wrappers = depot_instance_wrapper_symbols(&mono_ll);
    assert!(
        !wrappers.is_empty(),
        "expected at least one depot instance wrapper in __mono.ll"
    );
    for wrapper in &wrappers {
        assert_no_wrapper_calls(&mono_ll, wrapper);
    }
}

/// Runtime parity: the direct-path recursion computes the same result the
/// wrapper path did.
#[test]
fn recursive_generic_instance_runs_correctly() {
    let single = r#"
// Partial (ADR 11.8.26b): a `Nat` countdown — this fixture is about the
// depot instance's call convention, not termination.
#[partial]
fn spin_generic<T>(depth: Nat, salt: Nat, payload: T) -> Nat {
    if depth == 0 {
        salt
    } else {
        spin_generic(depth - 1, salt + 1, payload)
    }
}

fn main() -> Nat {
    spin_generic(50, 0, (1, 2))
}
"#;
    assert_eq!(compile_and_run(&[("single.tg", single)], "single.tg"), "50");
}
