//! ADR 12.7.26a — per-unit extern-name-map colliding-import resolution.
//!
//! End-to-end compile+run coverage: each test builds a small project where
//! two modules define the same function name, compiles it to a native
//! binary, runs it, and asserts on the printed main result. Distinct return
//! values (11 vs 22, 1 vs 2) prove which module's def the emitted code
//! called — before the fix these resolved clobber-last (silently calling
//! `b`'s def regardless of the import).

use super::mono_depot_externs::compile_and_run;
use crate::compile::{cmd_compile, CompileFlags};
use std::fs;
use std::process::ExitCode;
use tempfile::TempDir;

const MODULE_A_DESCRIBE: &str = "pub fn describe() -> Nat {\n    11\n}\n";
const MODULE_B_DESCRIBE: &str = "pub fn describe() -> Nat {\n    22\n}\n";

/// AC 1 (the reported bug): `use a::{describe}` with both `a::describe` and
/// `b::describe` defined must call a's def — the pre-fix clobber-last map
/// emitted a call to `b__describe__describe`.
#[test]
fn imported_colliding_name_resolves_to_imported_module() {
    let main = "mod a;\nmod b;\n\nuse a::{describe};\n\nfn main() -> Nat {\n    describe()\n}\n";
    assert_eq!(
        compile_and_run(
            &[
                ("a.tg", MODULE_A_DESCRIBE),
                ("b.tg", MODULE_B_DESCRIBE),
                ("main.tg", main),
            ],
            "main.tg"
        ),
        "11"
    );
}

/// AC 1 (IR half): the emitted main unit calls `a__describe__describe`,
/// never the clobber-last `b__describe__describe`.
#[test]
fn emitted_unit_calls_the_imported_modules_symbol() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.tg"), MODULE_A_DESCRIBE).unwrap();
    fs::write(dir.path().join("b.tg"), MODULE_B_DESCRIBE).unwrap();
    fs::write(
        dir.path().join("main.tg"),
        "mod a;\nmod b;\n\nuse a::{describe};\n\nfn main() -> Nat {\n    describe()\n}\n",
    )
    .unwrap();
    let ll_dir = dir.path().join("ll");
    let flags = CompileFlags {
        max_errors: 20,
        codegen_jobs: 1,
        emit_llvm: true,
        ..CompileFlags::default()
    };
    assert_eq!(
        cmd_compile(&dir.path().join("main.tg"), Some(&ll_dir), &flags),
        ExitCode::SUCCESS
    );
    let main_ll = fs::read_to_string(ll_dir.join("main").join("tungsten_main.ll")).unwrap();
    assert!(
        main_ll.contains("call i64 @a__describe__describe("),
        "main must call a's def:\n{main_ll}"
    );
    assert!(
        !main_ll.contains("call i64 @b__describe__describe("),
        "main must not call the clobber-last def:\n{main_ll}"
    );
}

/// AC 2: an aliased import (`use a::{describe as d}`) resolves to `a`'s def —
/// the import table is keyed by the original name `Term::Global` carries.
#[test]
fn aliased_import_resolves_to_imported_module() {
    let main = "mod a;\nmod b;\n\nuse a::{describe as d};\n\nfn main() -> Nat {\n    d()\n}\n";
    assert_eq!(
        compile_and_run(
            &[
                ("a.tg", MODULE_A_DESCRIBE),
                ("b.tg", MODULE_B_DESCRIBE),
                ("main.tg", main),
            ],
            "main.tg"
        ),
        "11"
    );
}

/// AC 3: `use c::{describe}` where `c` has `pub use a::describe` resolves
/// through the re-export chain to `a`'s def.
#[test]
fn reexport_chain_resolves_to_defining_module() {
    let main =
        "mod a;\nmod b;\nmod c;\n\nuse c::{describe};\n\nfn main() -> Nat {\n    describe()\n}\n";
    assert_eq!(
        compile_and_run(
            &[
                ("a.tg", MODULE_A_DESCRIBE),
                ("b.tg", MODULE_B_DESCRIBE),
                ("c.tg", "pub use a::describe;\n"),
                ("main.tg", main),
            ],
            "main.tg"
        ),
        "11"
    );
}

/// AC 4: a glob import (`use a::*`) resolves the colliding name to `a`'s def.
#[test]
fn glob_import_resolves_to_imported_module() {
    let main = "mod a;\nmod b;\n\nuse a::*;\n\nfn main() -> Nat {\n    describe()\n}\n";
    assert_eq!(
        compile_and_run(
            &[
                ("a.tg", MODULE_A_DESCRIBE),
                ("b.tg", MODULE_B_DESCRIBE),
                ("main.tg", main),
            ],
            "main.tg"
        ),
        "11"
    );
}

/// AC 5 (D5, depot path): a generic body calling an IMPORTED colliding name
/// resolves through the instance module's import table in `__mono`.
#[test]
fn depot_instance_imported_colliding_name_resolves() {
    let util_a = "pub fn pick() -> Nat {\n    1\n}\n";
    let util_b = "pub fn pick() -> Nat {\n    2\n}\n";
    let generic = "use util_a::{pick};\n\npub fn choose<T>(x: T) -> Nat {\n    pick()\n}\n";
    let main = "mod util_a;\nmod util_b;\nmod gen;\n\nuse gen::{choose};\n\nfn main() -> Nat {\n    choose(5)\n}\n";
    assert_eq!(
        compile_and_run(
            &[
                ("util_a.tg", util_a),
                ("util_b.tg", util_b),
                ("gen.tg", generic),
                ("main.tg", main),
            ],
            "main.tg"
        ),
        "1"
    );
}

/// D4 end-to-end: a `gen/mod.tg` module's generic body calling its OWN
/// colliding `pick` resolves to the own-module def — the old
/// `module_path.join("__")` owner-key reconstruction missed for `mod.tg`
/// modules and silently fell back to the clobbered entry.
#[test]
fn depot_instance_mod_tg_own_module_colliding_name_resolves() {
    let gen_mod =
        "pub fn pick() -> Nat {\n    1\n}\n\npub fn choose<T>(x: T) -> Nat {\n    pick()\n}\n";
    let other = "pub fn pick() -> Nat {\n    2\n}\n";
    let main =
        "mod gen;\nmod other;\n\nuse gen::{choose};\n\nfn main() -> Nat {\n    choose(5)\n}\n";
    assert_eq!(
        compile_and_run(
            &[
                ("gen/mod.tg", gen_mod),
                ("other.tg", other),
                ("main.tg", main),
            ],
            "main.tg"
        ),
        "1"
    );
}

/// AC (workspace-sibling two-prefix, §2.1): compiling entry `tool.tg` next to
/// a sibling `main.tg` registers `parser.tg` under TWO prefixes — `parser`
/// (the entry's tree walk) and `main::parser` (workspace sibling discovery of
/// `main.tg`'s `mod parser;`). The colliding import must still resolve: both
/// the extracted import target and `DefInfo.module_path` pass through
/// `canonicalize_path`, so they compare in one path space. A mismatch here
/// would surface as the D1 hard error (lookup miss) instead of "11".
#[test]
fn workspace_sibling_two_prefix_registration_resolves() {
    let sibling_main = "mod parser;\nmod other;\n\nfn main() -> Nat {\n    0\n}\n";
    let parser = "pub fn describe() -> Nat {\n    11\n}\n";
    let other = "pub fn describe() -> Nat {\n    22\n}\n";
    let tool = "mod parser;\nmod other;\n\nuse parser::{describe};\n\nfn main() -> Nat {\n    describe()\n}\n";
    assert_eq!(
        compile_and_run(
            &[
                ("main.tg", sibling_main),
                ("parser.tg", parser),
                ("other.tg", other),
                ("tool.tg", tool),
            ],
            "tool.tg"
        ),
        "11"
    );
}

/// AC 6a: an UNIMPORTED reference to a colliding name is a hard compile
/// error (D1) instead of a silent 50/50 miscompile. (The §2.3 message shape
/// is pinned LLVM-free in `per_module/tests/collision_resolution.rs`.)
#[test]
fn unimported_colliding_reference_fails_to_compile() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.tg"), MODULE_A_DESCRIBE).unwrap();
    fs::write(dir.path().join("b.tg"), MODULE_B_DESCRIBE).unwrap();
    fs::write(
        dir.path().join("main.tg"),
        "mod a;\nmod b;\n\nfn main() -> Nat {\n    describe()\n}\n",
    )
    .unwrap();
    let flags = CompileFlags {
        max_errors: 20,
        codegen_jobs: 1,
        ..CompileFlags::default()
    };
    assert_eq!(
        cmd_compile(
            &dir.path().join("main.tg"),
            Some(&dir.path().join("prog")),
            &flags
        ),
        ExitCode::FAILURE,
        "an unimported colliding reference must hard-error, not miscompile"
    );
}

/// AC 6b: double-alias imports of the same original name (`use a::{describe
/// as da}; use b::{describe as db}`) hard-error — elaboration erases which
/// alias an occurrence used (D3), so codegen refuses to guess.
#[test]
fn double_alias_colliding_imports_fail_to_compile() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.tg"), MODULE_A_DESCRIBE).unwrap();
    fs::write(dir.path().join("b.tg"), MODULE_B_DESCRIBE).unwrap();
    fs::write(
        dir.path().join("main.tg"),
        "mod a;\nmod b;\n\nuse a::{describe as da};\nuse b::{describe as db};\n\nfn main() -> Nat {\n    da() + db()\n}\n",
    )
    .unwrap();
    let flags = CompileFlags {
        max_errors: 20,
        codegen_jobs: 1,
        ..CompileFlags::default()
    };
    assert_eq!(
        cmd_compile(
            &dir.path().join("main.tg"),
            Some(&dir.path().join("prog")),
            &flags
        ),
        ExitCode::FAILURE
    );
}
